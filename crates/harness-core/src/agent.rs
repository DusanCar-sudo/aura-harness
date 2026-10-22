//! The chat loop: stream an answer, run any tool calls, repeat until `stop`.
//!
//! This is the piece that turns `api::stream_chat` (one completion) into a
//! conversation with plugins. Command tools pause for user approval; MCP tools
//! run straight through.

use crate::api::{stream_chat, ChatOptions, Message, StreamEvent, ToolCall};
use crate::error::{Error, Result};
use crate::mcp::McpConnection;
use crate::tools::{ToolExecutor, ToolRegistry};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};

/// Cap on stream → tools → stream rounds, so a confused model cannot spin.
pub const MAX_TOOL_ROUNDS: usize = 150;

/// What the UI must decide before a command tool runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Approval {
    Approve,
    Reject,
}

/// Events the loop emits for the UI to render.
#[derive(Debug)]
pub enum LoopEvent {
    /// Partial reasoning text (chain of thought).
    Reasoning(String),
    /// Partial answer text.
    Content(String),
    /// A tool call the model asked for; carries a rendered one-line summary.
    ToolRequested {
        call_id: String,
        name: String,
        summary: String,
        needs_approval: bool,
    },
    /// A tool finished. `output` is already truncated for display.
    ToolFinished {
        call_id: String,
        name: String,
        output: String,
        is_error: bool,
    },
    /// One full assistant turn is complete (message appended to history).
    TurnComplete(Message),
    /// The whole loop is done; no more events will arrive.
    Finished { rounds: usize },
    /// Terminal error; the loop stops here.
    Failed(String),
}

/// Inputs captured once at the start of a run.
pub struct LoopInput {
    pub http: reqwest::Client,
    pub api_key: String,
    pub history: Vec<Message>,
    pub opts: ChatOptions,
    pub registry: ToolRegistry,
    /// Live MCP connections, shared with the app so tools resolve by name and
    /// the servers stay up across turns.
    pub mcp: Arc<Mutex<BTreeMap<String, McpConnection>>>,
    /// Where approval decisions come back from the UI.
    pub approvals: mpsc::Receiver<Approval>,
}

/// Truncate tool output for the transcript without losing the shape of it.
pub fn truncate_for_display(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… [{} bytes total]", &s[..end], s.len())
}

/// A short human-readable line describing a tool call for the approval card.
pub fn summarize_call(name: &str, arguments: &str) -> String {
    let parsed: serde_json::Value =
        serde_json::from_str(arguments).unwrap_or(serde_json::Value::Null);
    match parsed {
        serde_json::Value::Null => format!("{name}()"),
        other => {
            let rendered = other.to_string();
            format!("{name}({})", truncate_for_display(&rendered, 200))
        }
    }
}

/// Accumulates streamed tool-call deltas by index until the turn ends.
#[derive(Debug, Default)]
struct PendingCalls {
    by_index: BTreeMap<usize, ToolCall>,
}

impl PendingCalls {
    fn apply(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::ToolCallStart { index, id, name } => {
                let entry = self
                    .by_index
                    .entry(*index)
                    .or_insert_with(|| ToolCall::new(id.clone(), *index, name.clone(), ""));
                if !id.is_empty() {
                    entry.id = id.clone();
                }
                if !name.is_empty() {
                    entry.function.name = name.clone();
                }
            }
            StreamEvent::ToolCallArgs { index, delta } => {
                let entry = self
                    .by_index
                    .entry(*index)
                    .or_insert_with(|| ToolCall::new(String::new(), *index, "", ""));
                entry.function.arguments.push_str(delta);
            }
            _ => {}
        }
    }

    fn into_sorted(self) -> Vec<ToolCall> {
        self.by_index.into_values().collect()
    }
}

/// Run one tool, honouring the approval gate for command tools.
async fn execute_tool(
    registry: &ToolRegistry,
    mcp: &Arc<Mutex<BTreeMap<String, McpConnection>>>,
    call: &ToolCall,
    approvals: &mut mpsc::Receiver<Approval>,
) -> Result<String> {
    let name = call.function.name.clone();
    let registered = registry.get(&name)?;
    match &registered.executor {
        ToolExecutor::Builtin(tool) => {
            // Mutating built-ins (shell, write_file, edit_file) wait for the
            // user's verdict; read-only ones run straight through. In full
            // access mode needs_approval() is false for every tool.
            let gated = registry.needs_approval(&name).unwrap_or(false);
            if gated {
                let verdict = approvals.recv().await.unwrap_or(Approval::Reject);
                if verdict == Approval::Reject {
                    return Ok(format!("[user rejected running {name}]"));
                }
            }
            tool.run(&call.function.arguments, registry.cwd()).await
        }
        ToolExecutor::Command(tool) => {
            // Wait for the user's verdict; a closed channel means "reject".
            // Full access mode skips the wait entirely.
            if !registry.needs_approval(&name).unwrap_or(false) {
                return crate::cmdtool::run(tool, &call.function.arguments).await;
            }
            let verdict = approvals.recv().await.unwrap_or(Approval::Reject);
            if verdict == Approval::Reject {
                return Ok(format!("[user rejected running {name}]"));
            }
            crate::cmdtool::run(tool, &call.function.arguments).await
        }
        ToolExecutor::Mcp { server, tool } => {
            let mut conns = mcp.lock().await;
            let conn = conns
                .get_mut(server)
                .ok_or_else(|| Error::Mcp(format!("{server} is not connected")))?;
            let result = conn.call(tool, &call.function.arguments).await?;
            if result.is_error {
                Ok(format!("[tool error]\n{}", result.text))
            } else {
                Ok(result.text)
            }
        }
    }
}

/// Drive the loop to completion, emitting events as they happen.
pub async fn run(input: LoopInput, events: mpsc::Sender<LoopEvent>) -> Result<()> {
    let LoopInput {
        http,
        api_key,
        mut history,
        mut opts,
        registry,
        mcp,
        mut approvals,
    } = input;

    opts.tools = registry.definitions();

    for round in 0..MAX_TOOL_ROUNDS {
        // Reasoning and session timestamps are ours; never echo them back.
        let wire: Vec<Message> = history.iter().map(Message::to_api).collect();
        let mut stream = stream_chat(http.clone(), &api_key, wire, opts.clone());

        let mut answer = String::new();
        let mut reasoning = String::new();
        let mut pending = PendingCalls::default();
        let mut finish_reason: Option<String> = None;

        while let Some(item) = stream.recv().await {
            let event = match item {
                Ok(e) => e,
                Err(e) => {
                    let _ = events.send(LoopEvent::Failed(e.to_string())).await;
                    return Err(e);
                }
            };
            match &event {
                StreamEvent::Reasoning(r) => {
                    reasoning.push_str(r);
                    if events.send(LoopEvent::Reasoning(r.clone())).await.is_err() {
                        return Ok(());
                    }
                }
                StreamEvent::Content(c) => {
                    answer.push_str(c);
                    if events.send(LoopEvent::Content(c.clone())).await.is_err() {
                        return Ok(());
                    }
                }
                StreamEvent::ToolCallStart { .. } | StreamEvent::ToolCallArgs { .. } => {
                    pending.apply(&event);
                }
                StreamEvent::Done { finish_reason: fr, .. } => {
                    finish_reason = fr.clone();
                }
            }
        }

        let calls = pending.into_sorted();
        // Reasoning is display-only: the API rejects it echoed back, so the
        // copy kept for the UI carries it and the history copy does not.
        let display_turn = Message {
            role: "assistant".into(),
            content: answer,
            images: Vec::new(),
            reasoning_content: reasoning,
            tool_calls: calls.clone(),
            tool_call_id: None,
            ts: Some(chrono::Utc::now()),
        };
        let for_history = Message {
            reasoning_content: String::new(),
            ..display_turn.clone()
        };
        history.push(for_history);
        let _ = events.send(LoopEvent::TurnComplete(display_turn)).await;

        let wants_tools = finish_reason.as_deref() == Some("tool_calls") || !calls.is_empty();
        if !wants_tools {
            let _ = events.send(LoopEvent::Finished { rounds: round + 1 }).await;
            return Ok(());
        }

        // Report every requested call, then run them in order.
        for call in &calls {
            let name = call.function.name.clone();
            let needs_approval = registry.needs_approval(&name).unwrap_or(false);
            let summary = summarize_call(&name, &call.function.arguments);
            let _ = events
                .send(LoopEvent::ToolRequested {
                    call_id: call.id.clone(),
                    name: name.clone(),
                    summary,
                    needs_approval,
                })
                .await;

            let outcome = execute_tool(&registry, &mcp, call, &mut approvals).await;
            let (output, is_error) = match outcome {
                Ok(text) => (text, false),
                Err(e) => (e.to_string(), true),
            };
            let _ = events
                .send(LoopEvent::ToolFinished {
                    call_id: call.id.clone(),
                    name: name.clone(),
                    output: truncate_for_display(&output, 4000),
                    is_error,
                })
                .await;

            history.push(Message::tool_result(call.id.clone(), output));
        }
    }

    let message = format!("stopped after {MAX_TOOL_ROUNDS} tool rounds");
    let _ = events.send(LoopEvent::Failed(message.clone())).await;
    Err(Error::Invalid(message))
}

/// Convenience for the UI: run the loop on a fresh channel, returning the
/// receiver plus the join handle.
pub fn spawn(input: LoopInput) -> (mpsc::Receiver<LoopEvent>, tokio::task::JoinHandle<Result<()>>) {
    let (tx, rx) = mpsc::channel(256);
    let handle = tokio::spawn(async move { run(input, tx).await });
    (rx, handle)
}

/// Build a channel the UI uses to answer pending approvals.
pub fn approval_channel() -> (mpsc::Sender<Approval>, mpsc::Receiver<Approval>) {
    mpsc::channel(4)
}

/// Await one approval, treating a dropped sender as a rejection.
pub async fn await_approval(rx: &mut oneshot::Receiver<Approval>) -> Approval {
    rx.await.unwrap_or(Approval::Reject)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ToolDef;
    use crate::config::CommandTool;
    use crate::tools::RegisteredTool;

    fn call(name: &str, arguments: &str) -> ToolCall {
        ToolCall::new("c1", 0, name, arguments)
    }

    #[test]
    fn accumulates_tool_call_deltas_by_index() {
        let mut p = PendingCalls::default();
        p.apply(&StreamEvent::ToolCallStart {
            index: 0,
            id: "call_1".into(),
            name: "cmd_echo".into(),
        });
        p.apply(&StreamEvent::ToolCallArgs { index: 0, delta: "{\"text\":".into() });
        p.apply(&StreamEvent::ToolCallArgs { index: 0, delta: "\"hi\"}".into() });
        let calls = p.into_sorted();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].function.name, "cmd_echo");
        assert_eq!(calls[0].function.arguments, "{\"text\":\"hi\"}");
    }

    #[test]
    fn accumulates_parallel_tool_calls() {
        let mut p = PendingCalls::default();
        p.apply(&StreamEvent::ToolCallStart { index: 1, id: "b".into(), name: "two".into() });
        p.apply(&StreamEvent::ToolCallStart { index: 0, id: "a".into(), name: "one".into() });
        p.apply(&StreamEvent::ToolCallArgs { index: 1, delta: "{}".into() });
        let calls = p.into_sorted();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].function.name, "one");
        assert_eq!(calls[1].function.arguments, "{}");
    }

    #[test]
    fn truncates_on_char_boundary() {
        let s = "é".repeat(10); // 20 bytes
        let out = truncate_for_display(&s, 5);
        assert!(out.contains("bytes total"));
        assert!(out.starts_with('é'));
        assert_eq!(truncate_for_display("short", 100), "short");
    }

    #[test]
    fn summarizes_calls_readably() {
        assert_eq!(
            summarize_call("cmd_echo", "{\"text\":\"hi\"}"),
            "cmd_echo({\"text\":\"hi\"})"
        );
        assert_eq!(summarize_call("cmd_ping", ""), "cmd_ping()");
        assert_eq!(summarize_call("cmd_ping", "not-json"), "cmd_ping()");
    }

    fn no_mcp() -> Arc<Mutex<BTreeMap<String, McpConnection>>> {
        Arc::new(Mutex::new(BTreeMap::new()))
    }

    fn echo_registry() -> ToolRegistry {
        let mut reg = ToolRegistry::new(false);
        reg.register(RegisteredTool {
            def: ToolDef {
                name: "cmd_echo".into(),
                description: "echo".into(),
                parameters: serde_json::json!({"type":"object"}),
            },
            executor: ToolExecutor::Command(CommandTool {
                command: "/bin/echo".into(),
                args: vec!["{{text}}".into()],
                description: "echo".into(),
                schema: toml::Table::new(),
                enabled: true,
            }),
        });
        reg
    }

    #[tokio::test]
    async fn command_tool_runs_only_after_approval() {
        let registry = echo_registry();
        let mcp = no_mcp();
        let (tx, mut rx) = approval_channel();
        tx.send(Approval::Approve).await.unwrap();
        let out = execute_tool(&registry, &mcp, &call("cmd_echo", "{\"text\":\"go\"}"), &mut rx)
            .await
            .unwrap();
        assert_eq!(out.trim(), "go");
    }

    #[tokio::test]
    async fn rejected_command_tool_reports_rejection() {
        let registry = echo_registry();
        let mcp = no_mcp();
        let (tx, mut rx) = approval_channel();
        tx.send(Approval::Reject).await.unwrap();
        let out = execute_tool(&registry, &mcp, &call("cmd_echo", "{\"text\":\"go\"}"), &mut rx)
            .await
            .unwrap();
        assert!(out.contains("rejected"));
    }

    #[tokio::test]
    async fn closed_approval_channel_rejects() {
        let registry = echo_registry();
        let mcp = no_mcp();
        let (tx, mut rx) = approval_channel();
        drop(tx);
        let out = execute_tool(&registry, &mcp, &call("cmd_echo", "{}"), &mut rx)
            .await
            .unwrap();
        assert!(out.contains("rejected"));
    }

    #[tokio::test]
    async fn unknown_tool_is_an_error() {
        let registry = ToolRegistry::new(false);
        let mcp = no_mcp();
        let (_tx, mut rx) = approval_channel();
        assert!(execute_tool(&registry, &mcp, &call("nope", "{}"), &mut rx)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn builtin_shell_runs_only_after_approval() {
        let registry = ToolRegistry::new(false);
        let mcp = no_mcp();
        let (tx, mut rx) = approval_channel();
        tx.send(Approval::Approve).await.unwrap();
        let out = execute_tool(&registry, &mcp, &call("shell", r#"{"command":"echo builtin-ok"}"#), &mut rx)
            .await
            .unwrap();
        assert_eq!(out.trim(), "builtin-ok");
    }

    #[tokio::test]
    async fn builtin_tools_run_in_the_registry_folder() {
        let dir = std::env::temp_dir().join("agent-cwd-test");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let mut registry = ToolRegistry::new(true);
        registry.set_cwd(&dir);
        let mcp = no_mcp();
        let (_tx, mut rx) = approval_channel();

        let pwd = execute_tool(&registry, &mcp, &call("shell", r#"{"command":"pwd"}"#), &mut rx)
            .await
            .unwrap();
        assert_eq!(pwd.trim(), dir.canonicalize().unwrap().to_str().unwrap());

        execute_tool(
            &registry,
            &mcp,
            &call("write_file", r#"{"path":"index.html","content":"ok"}"#),
            &mut rx,
        )
        .await
        .unwrap();
        assert!(dir.join("index.html").is_file());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn builtin_read_runs_without_approval() {
        let registry = ToolRegistry::new(false);
        let mcp = no_mcp();
        let (_tx, mut rx) = approval_channel(); // never answered
        let dir = std::env::temp_dir().join("agent-builtin-test");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let file = dir.join("x.txt");
        tokio::fs::write(&file, "hello\n").await.unwrap();
        let out = execute_tool(
            &registry,
            &mcp,
            &call("read_file", &format!(r#"{{"path":"{}"}}"#, file.display())),
            &mut rx,
        )
        .await
        .unwrap();
        assert!(out.contains("hello"), "{out}");
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn builtin_write_is_gated() {
        let registry = ToolRegistry::new(false);
        let mcp = no_mcp();
        let (tx, mut rx) = approval_channel();
        tx.send(Approval::Reject).await.unwrap();
        let out = execute_tool(
            &registry,
            &mcp,
            &call("write_file", r#"{"path":"/tmp/should-not-exist-aura","content":"x"}"#),
            &mut rx,
        )
        .await
        .unwrap();
        assert!(out.contains("rejected"), "{out}");
        assert!(!std::path::Path::new("/tmp/should-not-exist-aura").exists());
    }

    #[tokio::test]
    async fn mcp_tool_without_connection_errors() {
        let mut registry = ToolRegistry::new(false);
        registry.register(RegisteredTool {
            def: ToolDef {
                name: "mcp_ghost_ping".into(),
                description: "ping".into(),
                parameters: serde_json::json!({"type":"object"}),
            },
            executor: ToolExecutor::Mcp {
                server: "ghost".into(),
                tool: "ping".into(),
            },
        });
        let mcp = no_mcp();
        let (_tx, mut rx) = approval_channel();
        let err = execute_tool(&registry, &mcp, &call("mcp_ghost_ping", "{}"), &mut rx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not connected"));
    }
}
