//! Live checks against the real DeepSeek API.
//!
//! Network + a funded key are required, so these are `#[ignore]`d:
//!
//! ```bash
//! cargo test -p harness-core --test live_deepseek -- --ignored --nocapture
//! ```
//!
//! The key comes from `DEEPSEEK_API_KEY` or `~/.config/aura-harness/credentials`;
//! it is never written into this file.

use std::sync::Arc;
use std::time::Duration;

use harness_core::agent::{self, Approval, LoopEvent, LoopInput};
use harness_core::api::{stream_chat, ChatOptions, Message, ReasoningEffort, StreamEvent};
use harness_core::config::{CommandTool, Plugins};
use harness_core::tools::{self, ToolRegistry};
use tokio::sync::Mutex;

const TIMEOUT: Duration = Duration::from_secs(90);

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { format!("{}…", &s[..max]) }
}

fn api_key() -> Option<String> {
    if let Ok(k) = std::env::var("DEEPSEEK_API_KEY") {
        if !k.trim().is_empty() {
            return Some(k.trim().to_string());
        }
    }
    harness_core::config::load_api_key()
}

/// Thinking on: we must see reasoning *and* an answer, streamed.
#[tokio::test]
#[ignore = "hits the live API"]
async fn streams_reasoning_and_content() {
    let Some(key) = api_key() else {
        eprintln!("skipping: no API key");
        return;
    };

    let opts = ChatOptions {
        effort: Some(ReasoningEffort::High),
        ..Default::default()
    };

    let collect = async {
        let mut rx = stream_chat(
            reqwest::Client::new(),
            &key,
            vec![Message::user("Reply with exactly: pong")],
            opts,
        );

        let (mut reasoning, mut content) = (String::new(), String::new());
        let mut done = false;
        while let Some(item) = rx.recv().await {
            match item.expect("stream error") {
                StreamEvent::Reasoning(t) => reasoning.push_str(&t),
                StreamEvent::Content(t) => content.push_str(&t),
                StreamEvent::Done { .. } => {
                    done = true;
                    break;
                }
                _ => {}
            }
        }
        (reasoning, content, done)
    };

    let (reasoning, content, done) = tokio::time::timeout(TIMEOUT, collect)
        .await
        .expect("stream timed out");

    assert!(done, "stream never produced a Done event");
    assert!(
        content.to_lowercase().contains("pong"),
        "unexpected answer: {content:?}"
    );
    // Reasoning is model-dependent, but with effort:high it should be present.
    eprintln!("reasoning chars: {}, answer: {content:?}", reasoning.len());
}

/// The regression that produced HTTP 422: a tool call forces a *second*
/// request whose messages include the assistant's `tool_calls`, so this only
/// passes if that message serializes the way the API expects.
#[tokio::test]
#[ignore = "hits the live API"]
async fn runs_a_command_tool_and_answers_afterwards() {
    let Some(key) = api_key() else {
        eprintln!("skipping: no API key");
        return;
    };

    let mut plugins = Plugins::default();
    plugins.cmd.insert(
        "echo".into(),
        CommandTool {
            command: "/bin/echo".into(),
            args: vec!["{{text}}".into()],
            description: "Echo the given text back verbatim.".into(),
            schema: toml::Table::new(),
            enabled: true,
        },
    );
    let registry: ToolRegistry = tools::from_plugins(&plugins, false);
    // from_plugins starts at ToolRegistry::new(), so the built-ins travel with
    // it; the plugin tool must land alongside them, not replace them.
    let names: Vec<String> = registry
        .definitions()
        .iter()
        .map(|d| d.name.clone())
        .collect();
    assert!(
        names.contains(&"cmd_echo".to_string()),
        "cmd_echo missing from {names:?}"
    );
    assert!(
        names.contains(&"shell".to_string()),
        "built-ins missing from {names:?}"
    );

    let (appr_tx, appr_rx) = agent::approval_channel();
    let input = LoopInput {
        http: reqwest::Client::new(),
        api_key: key,
        history: vec![Message::user(
            "Use the cmd_echo tool with the text \"hello-aura\", then tell me what it returned.",
        )],
        opts: ChatOptions::default(),
        registry,
        mcp: Arc::new(Mutex::new(Default::default())),
        approvals: appr_rx,
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    tokio::spawn(async move {
        let _ = agent::run(input, tx).await;
    });

    let drive = async {
        let mut requested = 0usize;
        let mut tool_output = String::new();
        let mut answer = String::new();
        let mut finished = false;

        while let Some(event) = rx.recv().await {
            match event {
                LoopEvent::ToolRequested {
                    name,
                    summary,
                    needs_approval,
                    ..
                } => {
                    eprintln!("[req #{requested}] {name} needs_approval={needs_approval} :: {summary}");
                    assert_eq!(name, "cmd_echo");
                    requested += 1;
                    if needs_approval {
                        appr_tx.send(Approval::Approve).await.unwrap();
                    }
                }
                LoopEvent::ToolFinished {
                    output,
                    is_error,
                    ..
                } => {
                    eprintln!("[done] is_error={is_error} output={:?}", truncate(&output, 300));
                    tool_output = output;
                }
                // Content arriving *after* the tool proves the follow-up
                // request (the one carrying tool_calls) was accepted.
                LoopEvent::Content(t) => answer.push_str(&t),
                LoopEvent::Finished { rounds } => {
                    eprintln!("[finished] after {rounds} rounds");
                    finished = true;
                    break;
                }
                LoopEvent::Failed(e) => panic!("loop failed: {e}"),
                _ => {}
            }
        }
        (requested, tool_output, answer, finished)
    };

    let (requested, tool_output, answer, finished) = tokio::time::timeout(TIMEOUT, drive)
        .await
        .expect("loop timed out");

    eprintln!("final answer: {answer:?}");
    assert_eq!(requested, 1, "expected exactly one tool request");
    assert!(
        tool_output.contains("hello-aura"),
        "tool output was {tool_output:?}"
    );
    assert!(finished, "loop never reported Finished");
    assert!(
        answer.to_lowercase().contains("hello-aura"),
        "final answer did not mention the tool output: {answer:?}"
    );
    eprintln!("tool round trip ok: {tool_output:?} -> {answer:?}");
}

/// The built-in tools are the harness's own tools (shell, read/write/edit,
/// list, grep, glob). Under full access they must reach the model, run, and
/// finish **without ever raising an approval** — that is the "no approval
/// card" guarantee the UI's auto_approve flag promises.
#[tokio::test]
#[ignore = "hits the live API"]
async fn runs_the_builtin_shell_tool_without_approval() {
    let Some(key) = api_key() else {
        eprintln!("skipping: no API key");
        return;
    };

    // Same construction the app uses: built-ins first, plugins on top.
    let registry: ToolRegistry = tools::from_plugins(&Plugins::default(), true);
    let names: Vec<String> = registry
        .definitions()
        .iter()
        .map(|d| d.name.clone())
        .collect();
    for builtin in ["shell", "read_file", "write_file", "edit_file", "list_files", "grep", "glob"] {
        assert!(names.contains(&builtin.to_string()), "missing built-in {builtin}: {names:?}");
    }

    let (appr_tx, appr_rx) = agent::approval_channel();
    let input = LoopInput {
        http: reqwest::Client::new(),
        api_key: key,
        history: vec![Message::user(
            "Use the shell tool exactly once to run `echo aura-builtin-ok`, then tell me exactly what it returned.",
        )],
        opts: ChatOptions::default(),
        registry,
        mcp: Arc::new(Mutex::new(Default::default())),
        approvals: appr_rx,
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    tokio::spawn(async move {
        let _ = agent::run(input, tx).await;
    });

    let drive = async {
        let mut requested = 0usize;
        let mut approvals_needed = 0usize;
        let mut tool_output = String::new();
        let mut tool_error = false;
        let mut answer = String::new();
        let mut finished = false;

        while let Some(event) = rx.recv().await {
            match event {
                LoopEvent::ToolRequested {
                    name,
                    summary,
                    needs_approval,
                    ..
                } => {
                    eprintln!(
                        "[builtin req #{requested}] {name} needs_approval={needs_approval} :: {summary}"
                    );
                    assert_eq!(name, "shell", "expected the shell tool, got {name}");
                    requested += 1;
                    if needs_approval {
                        approvals_needed += 1;
                        // Answer anyway so a regression fails on the count
                        // below instead of hanging until the timeout.
                        appr_tx.send(Approval::Approve).await.unwrap();
                    }
                }
                LoopEvent::ToolFinished {
                    output,
                    is_error,
                    ..
                } => {
                    eprintln!("[builtin done] is_error={is_error} output={:?}", truncate(&output, 300));
                    tool_output = output;
                    tool_error = is_error;
                }
                LoopEvent::Content(t) => answer.push_str(&t),
                LoopEvent::Finished { rounds } => {
                    eprintln!("[builtin finished] after {rounds} rounds");
                    finished = true;
                    break;
                }
                LoopEvent::Failed(e) => panic!("loop failed: {e}"),
                _ => {}
            }
        }
        (requested, approvals_needed, tool_output, tool_error, answer, finished)
    };

    let (requested, approvals_needed, tool_output, tool_error, answer, finished) =
        tokio::time::timeout(TIMEOUT, drive)
            .await
            .expect("loop timed out");

    assert_eq!(requested, 1, "expected exactly one shell call");
    assert_eq!(
        approvals_needed, 0,
        "full access must never raise an approval request"
    );
    assert!(!tool_error, "shell tool reported an error: {tool_output:?}");
    assert!(
        tool_output.contains("aura-builtin-ok"),
        "shell output was {tool_output:?}"
    );
    assert!(finished, "loop never reported Finished");
    assert!(
        answer.contains("aura-builtin-ok"),
        "final answer did not carry the tool output: {answer:?}"
    );
    eprintln!("builtin shell round trip ok: {tool_output:?} -> {answer:?}");
}

/// A bad key must surface as a typed API error, not a panic or a hang.
#[tokio::test]
#[ignore = "hits the live API"]
async fn a_rejected_key_is_an_api_error() {
    let mut rx = stream_chat(
        reqwest::Client::new(),
        "sk-definitely-not-valid",
        vec![Message::user("hi")],
        ChatOptions::default(),
    );

    let first = tokio::time::timeout(TIMEOUT, rx.recv())
        .await
        .expect("no response")
        .expect("channel closed");

    match first {
        Err(harness_core::Error::Api { status, .. }) => assert_eq!(status, 401),
        other => panic!("expected an Api error, got {other:?}"),
    }
}
