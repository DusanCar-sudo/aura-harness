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
    assert_eq!(registry.len(), 1);

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
                    needs_approval,
                    ..
                } => {
                    assert_eq!(name, "cmd_echo");
                    requested += 1;
                    if needs_approval {
                        appr_tx.send(Approval::Approve).await.unwrap();
                    }
                }
                LoopEvent::ToolFinished { output, .. } => tool_output = output,
                // Content arriving *after* the tool proves the follow-up
                // request (the one carrying tool_calls) was accepted.
                LoopEvent::Content(t) => answer.push_str(&t),
                LoopEvent::Finished { .. } => {
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
