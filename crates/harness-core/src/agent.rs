//! The chat loop: stream an answer, run any tool calls, repeat until `stop`.
//!
//! This is the piece that turns `api::stream_chat` (one completion) into a
//! conversation with plugins. Command tools pause for user approval; MCP tools
//! run straight through.

use crate::api::{stream_chat, ChatOptions, Message, StreamEvent, ToolCall};
use crate::builtins::BuiltinTool;
use crate::error::{Error, Result};
use crate::mcp::McpConnection;
use crate::tools::{ToolExecutor, ToolRegistry};
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};

/// Largest tool result kept in history; the UI copy is truncated separately.
const HISTORY_TOOL_OUTPUT_CAP: usize = 30_000;
/// Total history bytes before the oldest tool outputs are elided.
const HISTORY_BUDGET: usize = 400_000;
/// Tighter budgets for local backends (42k-ctx small GGUFs on LM Studio / Ollama).
const LOCAL_HISTORY_BUDGET: usize = 40_000;
/// Newest tool results that compaction never touches.
const KEEP_RECENT_TOOL_RESULTS: usize = 6;
const LOCAL_KEEP_RECENT_TOOL_RESULTS: usize = 3;
const LOCAL_KEEP_RECENT_MESSAGES: usize = 20;
const LOCAL_MAX_TOOLS: usize = 48;
/// Subagent recursion & round safety bounds.
const MAX_SUBAGENT_DEPTH: usize = 1;
const MAX_SUBAGENT_ROUNDS: usize = 14;
const SUBAGENT_TIMEOUT_SECS: u64 = 180;

/// One structured step published by the AGY-style loop checklist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStep {
    pub title: String,
    pub status: String,
}

/// Optional profile configuration passed into subagent spawning.
#[derive(Debug, Clone, Default)]
pub struct SubagentProfile {
    pub id: String,
    pub name: String,
    pub brief: String,
    pub model_id: String,
}

/// Model & delegation routing available to `spawn_subagent` calls.
#[derive(Debug, Clone, Default)]
pub struct SubagentConfig {
    /// Empty means follow the parent conversation's model.
    pub default_model_id: String,
    pub profiles: Vec<SubagentProfile>,
    pub depth: usize,
}

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
    /// Updated execution checklist from `update_steps`.
    StepsUpdated(Vec<AgentStep>),
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
    /// Optional subagent model routing for `spawn_subagent`.
    pub subagent: Option<SubagentConfig>,
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

/// Keep the head and tail of a long tool output so the model sees both the
/// start and the final lines (errors, exit codes) without flooding context.
pub fn cap_for_history(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let half = max / 2;
    let mut head = half;
    while !s.is_char_boundary(head) {
        head -= 1;
    }
    let mut tail = s.len() - half;
    while !s.is_char_boundary(tail) {
        tail += 1;
    }
    format!(
        "{}\n[... {} bytes elided ...]\n{}",
        &s[..head],
        tail - head,
        &s[tail..]
    )
}

/// Streamed tool arguments cut off mid-JSON (token limit, dropped stream)
/// must not run: a half-written `write_file` would clobber a file.
pub fn arguments_are_valid(arguments: &str) -> bool {
    arguments.trim().is_empty() || serde_json::from_str::<serde_json::Value>(arguments).is_ok()
}

/// Shrink the oldest tool outputs once history passes `budget`, keeping the
/// newest `keep_recent` intact. Messages stay so call/result pairing holds.
pub fn compact_history(history: &mut [Message], budget: usize, keep_recent: usize) {
    let mut total: usize = history.iter().map(|m| m.content.len()).sum();
    let tool_idx: Vec<usize> = history
        .iter()
        .enumerate()
        .filter(|(_, m)| m.role == "tool")
        .map(|(i, _)| i)
        .collect();
    let old = tool_idx.len().saturating_sub(keep_recent);
    for &i in &tool_idx[..old] {
        if total <= budget {
            break;
        }
        let before = history[i].content.len();
        let note = format!("[older tool output elided: {before} bytes]");
        total -= before.saturating_sub(note.len());
        history[i].content = note;
    }
}

fn is_local_model(model_name: Option<&str>) -> bool {
    model_name.is_some_and(|m| {
        matches!(
            crate::provider::provider_family(m),
            "lmstudio" | "aura-local" | "ollama"
        )
    })
}

/// Remove dead empty assistant messages, drop orphan tool messages, and close any
/// dangling `tool_calls` left behind by a stopped or crashed turn.
pub fn sanitize_history(history: &mut Vec<Message>) {
    let old = std::mem::take(history);
    let mut out: Vec<Message> = Vec::with_capacity(old.len());
    let mut pending_ids: Vec<String> = Vec::new();

    let flush_pending = |pending: &mut Vec<String>, out: &mut Vec<Message>| {
        for id in pending.drain(..) {
            out.push(Message::tool_result(
                id,
                "[tool execution stopped before returning a result]",
            ));
        }
    };

    for m in old {
        if m.role == "tool" {
            if let Some(id) = m.tool_call_id.as_deref() {
                if let Some(pos) = pending_ids.iter().position(|p| p == id) {
                    pending_ids.remove(pos);
                    out.push(m);
                }
            }
            continue;
        }

        flush_pending(&mut pending_ids, &mut out);

        if m.role == "assistant" && m.content.trim().is_empty() && m.tool_calls.is_empty() {
            continue;
        }

        if m.role == "assistant" && !m.tool_calls.is_empty() {
            for tc in &m.tool_calls {
                if !tc.id.is_empty() {
                    pending_ids.push(tc.id.clone());
                }
            }
        }

        // Strict alternation templates (Gemma, Anthropic, LM Studio Jinja) fail on
        // consecutive user turns (e.g. after a failed/retried send). Coalesce them.
        if m.role == "user" {
            if let Some(prev) = out.last_mut().filter(|p| p.role == "user") {
                if !m.content.is_empty() {
                    if !prev.content.is_empty() {
                        prev.content.push_str("\n\n");
                    }
                    prev.content.push_str(&m.content);
                }
                prev.images.extend(m.images);
                continue;
            }
        }

        out.push(m);
    }
    flush_pending(&mut pending_ids, &mut out);
    *history = out;
}

/// Trim oldest non-system conversation turns on small-context local models so
/// long sessions fit comfortably within LM Studio / llama.cpp context limits.
pub fn trim_history_to_budget(history: &mut Vec<Message>, max_bytes: usize, keep_recent_msgs: usize) {
    let sys_end = history
        .iter()
        .take_while(|m| m.role == "system")
        .count();
    loop {
        if sys_end + 1 >= history.len() {
            break;
        }
        let non_sys_len = history.len().saturating_sub(sys_end);
        let total_bytes: usize = history.iter().map(|m| m.content.len()).sum();
        if non_sys_len <= keep_recent_msgs && total_bytes <= max_bytes {
            break;
        }
        // Find the next `user` turn strictly after `sys_end` so we drop a full
        // turn boundary and never orphan `assistant.tool_calls` from `tool`.
        let next_user = history[sys_end + 1..]
            .iter()
            .position(|m| m.role == "user")
            .map(|rel| sys_end + 1 + rel);
        let Some(cut) = next_user else {
            break;
        };
        if history[cut..].iter().filter(|m| m.role == "user").count() == 0 {
            break;
        }
        history.drain(sys_end..cut);
    }
}

/// Keep built-in, command, and core local MCP tools within a compact schema
/// budget for small local models so hundreds of MCP schemas don't fill context.
pub fn select_tools_for_model(defs: Vec<crate::api::ToolDef>, model_name: Option<&str>) -> Vec<crate::api::ToolDef> {
    if !is_local_model(model_name) || defs.len() <= LOCAL_MAX_TOOLS {
        return defs;
    }
    let priority = |name: &str| -> u8 {
        if !name.starts_with("mcp_") && !name.starts_with("cmd_") {
            0
        } else if name.starts_with("cmd_") {
            1
        } else if [
            "mcp_mnemai_",
            "mcp_aura_chrome_",
            "mcp_filesystem_",
            "mcp_memory_",
            "mcp_git_",
        ]
        .iter()
        .any(|pfx| name.starts_with(pfx))
        {
            2
        } else {
            3
        }
    };
    let mut ranked: Vec<_> = defs.into_iter().collect();
    ranked.sort_by(|a, b| priority(&a.name).cmp(&priority(&b.name)).then_with(|| a.name.cmp(&b.name)));
    ranked.truncate(LOCAL_MAX_TOOLS);
    ranked
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
            StreamEvent::ToolCallSignature { index, signature } => {
                let entry = self
                    .by_index
                    .entry(*index)
                    .or_insert_with(|| ToolCall::new(String::new(), *index, "", ""));
                entry.thought_signature = Some(signature.clone());
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


fn run_subagent_boxed(
    input: LoopInput,
    events: mpsc::Sender<LoopEvent>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> {
    Box::pin(run(input, events))
}

/// Drive the loop to completion, emitting events as they happen.
///
/// Implements parallel execution waves for independent read-only / checklist /
/// subagent calls, bounded `spawn_subagent` delegation using configured role
/// or default subagent models, and quick transient stream retries.
pub async fn run(input: LoopInput, events: mpsc::Sender<LoopEvent>) -> Result<()> {
    let LoopInput {
        http,
        api_key,
        mut history,
        mut opts,
        registry,
        mcp,
        mut approvals,
        subagent,
    } = input;

    let sub_depth = subagent.as_ref().map(|s| s.depth).unwrap_or(0);
    let defs = if sub_depth >= MAX_SUBAGENT_DEPTH {
        registry
            .definitions()
            .into_iter()
            .filter(|d| d.name != BuiltinTool::SpawnSubagent.name())
            .collect()
    } else {
        registry.definitions()
    };
    opts.tools = select_tools_for_model(defs, opts.model_name.as_deref());
    let local_model = is_local_model(opts.model_name.as_deref());

    let mut round: usize = 0;
    loop {
        if sub_depth > 0 && round >= MAX_SUBAGENT_ROUNDS {
            let final_note = "Subagent round limit reached; returning collected findings.".to_string();
            let _ = events.send(LoopEvent::Content(final_note.clone())).await;
            let _ = events.send(LoopEvent::Finished { rounds: round }).await;
            return Ok(());
        }
        sanitize_history(&mut history);
        if local_model {
            compact_history(&mut history, LOCAL_HISTORY_BUDGET, LOCAL_KEEP_RECENT_TOOL_RESULTS);
            trim_history_to_budget(&mut history, LOCAL_HISTORY_BUDGET, LOCAL_KEEP_RECENT_MESSAGES);
        } else {
            compact_history(&mut history, HISTORY_BUDGET, KEEP_RECENT_TOOL_RESULTS);
        }
        // Reasoning and session timestamps are ours; never echo them back.
        let wire: Vec<Message> = history.iter().map(Message::to_api).collect();

        let (answer, reasoning, calls, finish_reason) =
            stream_round_with_retry(&http, &api_key, &wire, &opts, &events).await?;

        let wants_tools = finish_reason.as_deref() == Some("tool_calls") || !calls.is_empty();

        // Never treat an empty `round == 0` reply as silent completion; surface
        // a clear error instead so the UI does not falsely say "Task completed."
        let mut final_answer = answer;
        if !wants_tools && final_answer.trim().is_empty() {
            if !reasoning.trim().is_empty() {
                final_answer = reasoning.clone();
                let _ = events.send(LoopEvent::Content(final_answer.clone())).await;
            } else if round > 0 {
                final_answer = "Task completed successfully.".to_string();
                let _ = events.send(LoopEvent::Content(final_answer.clone())).await;
            } else {
                let err = Error::Api {
                    status: 0,
                    message: "model returned an empty response with no content or tool calls (check local server status, context length, or enabled MCP tools)".into(),
                };
                let _ = events.send(LoopEvent::Failed(err.to_string())).await;
                return Err(err);
            }
        }

        // Reasoning is display-only: the API rejects it echoed back, so the
        // copy kept for the UI carries it and the history copy does not.
        let display_turn = Message {
            role: "assistant".into(),
            content: final_answer,
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

        if !wants_tools {
            let _ = events.send(LoopEvent::Finished { rounds: round + 1 }).await;
            return Ok(());
        }

        // Report every requested call first so all cards appear immediately.
        for call in &calls {
            let name = call.function.name.clone();
            let needs_approval = registry.needs_approval(&name).unwrap_or(false);
            let summary = summarize_call(&name, &call.function.arguments);
            let _ = events
                .send(LoopEvent::ToolRequested {
                    call_id: call.id.clone(),
                    name,
                    summary,
                    needs_approval,
                })
                .await;
        }

        // Partition into parallel-safe waves vs sequential mutating/gated/MCP calls.
        let sub_ctx = SubagentExecCtx {
            http: http.clone(),
            parent_api_key: api_key.clone(),
            parent_opts: opts.clone(),
            registry: registry.clone(),
            mcp: Arc::clone(&mcp),
            subagent: subagent.clone(),
        };

        let mut i = 0;
        while i < calls.len() {
            if is_parallel_safe(&registry, &calls[i]) {
                let start = i;
                while i < calls.len() && is_parallel_safe(&registry, &calls[i]) {
                    i += 1;
                }
                let futs = calls[start..i].iter().map(|call| {
                    execute_non_gated(&registry, call, &sub_ctx)
                });
                let wave_results = join_all(futs).await;
                for (call, (outcome, steps_update)) in
                    calls[start..i].iter().zip(wave_results)
                {
                    if let Some(steps) = steps_update {
                        let _ = events.send(LoopEvent::StepsUpdated(steps)).await;
                    }
                    record_tool_result(&events, &mut history, call, outcome).await;
                }
            } else {
                let call = &calls[i];
                let name = call.function.name.as_str();
                let outcome = if arguments_are_valid(&call.function.arguments) {
                    execute_tool(&registry, &mcp, call, &mut approvals).await
                } else {
                    Ok(format!(
                        "[not run: the arguments for {name} were cut off and are not valid JSON. \
                         Retry with a shorter call, e.g. write large files in several smaller steps.]"
                    ))
                };
                record_tool_result(&events, &mut history, call, outcome).await;
                i += 1;
            }
        }
        round += 1;
    }
}

#[derive(Clone)]
struct SubagentExecCtx {
    http: reqwest::Client,
    parent_api_key: String,
    parent_opts: ChatOptions,
    registry: ToolRegistry,
    mcp: Arc<Mutex<BTreeMap<String, McpConnection>>>,
    subagent: Option<SubagentConfig>,
}

fn is_retryable_stream_error(err: &Error) -> bool {
    let s = err.to_string().to_lowercase();
    s.contains("429")
        || s.contains("500")
        || s.contains("502")
        || s.contains("503")
        || s.contains("504")
        || s.contains("timed out")
        || s.contains("timeout")
        || s.contains("connection reset")
        || s.contains("temporarily")
}

async fn stream_round_with_retry(
    http: &reqwest::Client,
    api_key: &str,
    wire: &[Message],
    opts: &ChatOptions,
    events: &mpsc::Sender<LoopEvent>,
) -> Result<(String, String, Vec<ToolCall>, Option<String>)> {
    let backoffs_ms = [700u64, 1500u64];
    for attempt in 0..=backoffs_ms.len() {
        let mut stream = stream_chat(http.clone(), api_key, wire.to_vec(), opts.clone());
        let mut answer = String::new();
        let mut reasoning = String::new();
        let mut pending = PendingCalls::default();
        let mut finish_reason: Option<String> = None;
        let mut stream_err: Option<Error> = None;

        while let Some(item) = stream.recv().await {
            match item {
                Ok(event) => match &event {
                    StreamEvent::Reasoning(r) => {
                        reasoning.push_str(r);
                        if events.send(LoopEvent::Reasoning(r.clone())).await.is_err() {
                            return Ok((answer, reasoning, Vec::new(), finish_reason));
                        }
                    }
                    StreamEvent::Content(c) => {
                        answer.push_str(c);
                        if events.send(LoopEvent::Content(c.clone())).await.is_err() {
                            return Ok((answer, reasoning, Vec::new(), finish_reason));
                        }
                    }
                    StreamEvent::ToolCallStart { .. }
                    | StreamEvent::ToolCallArgs { .. }
                    | StreamEvent::ToolCallSignature { .. } => {
                        pending.apply(&event);
                    }
                    StreamEvent::Done { finish_reason: fr, .. } => {
                        finish_reason = fr.clone();
                    }
                },
                Err(e) => {
                    stream_err = Some(e);
                    break;
                }
            }
        }

        if let Some(e) = stream_err {
            let nothing_emitted =
                answer.is_empty() && reasoning.is_empty() && pending.by_index.is_empty();
            if nothing_emitted && attempt < backoffs_ms.len() && is_retryable_stream_error(&e) {
                tokio::time::sleep(std::time::Duration::from_millis(backoffs_ms[attempt])).await;
                continue;
            }
            let _ = events.send(LoopEvent::Failed(e.to_string())).await;
            return Err(e);
        }

        return Ok((answer, reasoning, pending.into_sorted(), finish_reason));
    }
    unreachable!()
}

fn is_parallel_safe(registry: &ToolRegistry, call: &ToolCall) -> bool {
    if !arguments_are_valid(&call.function.arguments) {
        return false;
    }
    let name = call.function.name.as_str();
    if registry.needs_approval(name).unwrap_or(false) {
        return false;
    }
    let Ok(reg) = registry.get(name) else {
        return false;
    };
    matches!(
        reg.executor,
        ToolExecutor::Builtin(
            BuiltinTool::ReadFile
                | BuiltinTool::ListFiles
                | BuiltinTool::Grep
                | BuiltinTool::Glob
                | BuiltinTool::UpdateSteps
                | BuiltinTool::SpawnSubagent
        )
    )
}

async fn execute_non_gated(
    registry: &ToolRegistry,
    call: &ToolCall,
    sub_ctx: &SubagentExecCtx,
) -> (Result<String>, Option<Vec<AgentStep>>) {
    let name = call.function.name.as_str();
    if name == BuiltinTool::UpdateSteps.name() {
        let steps = parse_steps(&call.function.arguments);
        let outcome = BuiltinTool::UpdateSteps
            .run(&call.function.arguments, registry.cwd())
            .await;
        return (outcome, Some(steps));
    }
    if name == BuiltinTool::SpawnSubagent.name() {
        let outcome = run_subagent(&call.function.arguments, sub_ctx).await;
        return (outcome, None);
    }
    let res = match registry.get(name) {
        Ok(reg) => match &reg.executor {
            ToolExecutor::Builtin(tool) => tool.run(&call.function.arguments, registry.cwd()).await,
            _ => Err(Error::Invalid(format!("{name} must run sequentially"))),
        },
        Err(e) => Err(e),
    };
    (res, None)
}

fn parse_steps(arguments: &str) -> Vec<AgentStep> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(arguments) else {
        return Vec::new();
    };
    let Some(arr) = val.get("steps").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let title = item.get("title")?.as_str()?.trim();
            if title.is_empty() {
                return None;
            }
            let status = item
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("pending")
                .to_string();
            Some(AgentStep {
                title: title.to_string(),
                status,
            })
        })
        .collect()
}

async fn run_subagent(arguments: &str, ctx: &SubagentExecCtx) -> Result<String> {
    let args: serde_json::Value =
        serde_json::from_str(arguments).unwrap_or(serde_json::Value::Null);
    let task = args
        .get("task")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if task.is_empty() {
        return Err(Error::Invalid("spawn_subagent requires a non-empty task".into()));
    }
    let role_arg = args.get("role").and_then(|v| v.as_str()).unwrap_or("").trim();
    let model_arg = args
        .get("model_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();

    let depth = ctx.subagent.as_ref().map(|s| s.depth).unwrap_or(0);
    if depth >= MAX_SUBAGENT_DEPTH {
        return Ok("[subagent depth limit reached: run built-in tools directly]".into());
    }

    let matched_profile = ctx.subagent.as_ref().and_then(|cfg| {
        if role_arg.is_empty() {
            return None;
        }
        let lower = role_arg.to_lowercase();
        cfg.profiles.iter().find(|p| {
            p.id.to_lowercase() == lower
                || p.name.to_lowercase() == lower
                || p.name.to_lowercase().contains(&lower)
        })
    });

    let chosen_model = if !model_arg.is_empty() {
        Some(model_arg.to_string())
    } else if let Some(p) = matched_profile.filter(|p| !p.model_id.trim().is_empty()) {
        Some(p.model_id.trim().to_string())
    } else if let Some(def) = ctx
        .subagent
        .as_ref()
        .map(|s| s.default_model_id.trim())
        .filter(|m| !m.is_empty())
    {
        Some(def.to_string())
    } else {
        ctx.parent_opts.model_name.clone()
    };

    let (sub_key, sub_base, mut sub_opts, model_label) = match chosen_model {
        Some(ref id) if Some(id.as_str()) != ctx.parent_opts.model_name.as_deref() => {
            let key = crate::provider::get_api_key_for_model(id).ok_or_else(|| {
                Error::Invalid(format!("no API key configured for subagent model {id}"))
            })?;
            let base = crate::provider::base_url_for(id);
            let opts = ChatOptions {
                model: None,
                model_name: Some(id.clone()),
                base_url: base.clone(),
                effort: ctx.parent_opts.effort,
                ..Default::default()
            };
            (key, base, opts, id.clone())
        }
        Some(id) => (
            ctx.parent_api_key.clone(),
            ctx.parent_opts.base_url.clone(),
            ctx.parent_opts.clone(),
            id,
        ),
        None => (
            ctx.parent_api_key.clone(),
            ctx.parent_opts.base_url.clone(),
            ctx.parent_opts.clone(),
            "parent default".into(),
        ),
    };
    sub_opts.base_url = sub_base;

    let role_name = if !role_arg.is_empty() {
        role_arg
    } else if let Some(p) = matched_profile {
        p.name.as_str()
    } else {
        "Subagent"
    };
    let brief_note = matched_profile
        .map(|p| format!("\nRole guidance: {}", p.brief.trim()))
        .unwrap_or_default();

    let system = format!(
        "You are {role_name}, a focused subagent working in {}. \
         Execute the assigned task directly with read-only / inspection / MCP tools, verify facts against files or outputs you actually read, \
         and return a concise, concrete findings summary.{brief_note}",
        ctx.registry.cwd().display()
    );

    let mut child_reg = ctx.registry.clone();
    child_reg.set_auto_approve(true);
    let (appr_tx, appr_rx) = approval_channel();
    drop(appr_tx);

    let child_input = LoopInput {
        http: ctx.http.clone(),
        api_key: sub_key,
        history: vec![Message::system(system), Message::user(task)],
        opts: sub_opts,
        registry: child_reg,
        mcp: Arc::clone(&ctx.mcp),
        approvals: appr_rx,
        subagent: Some(SubagentConfig {
            depth: depth + 1,
            ..Default::default()
        }),
    };

    let (tx, mut rx) = mpsc::channel(128);
    let handle = tokio::spawn(run_subagent_boxed(child_input, tx));

    let mut out = String::new();
    let mut rounds_used = 0usize;
    let collect = async {
        while let Some(ev) = rx.recv().await {
            match ev {
                LoopEvent::Content(c) => out.push_str(&c),
                LoopEvent::Finished { rounds } => {
                    rounds_used = rounds;
                    break;
                }
                LoopEvent::Failed(e) => {
                    return Err(Error::Invalid(format!("subagent ({model_label}): {e}")));
                }
                _ => {}
            }
        }
        Ok(())
    };
    if tokio::time::timeout(
        std::time::Duration::from_secs(SUBAGENT_TIMEOUT_SECS),
        collect,
    )
    .await
    .is_err()
    {
        handle.abort();
        return Err(Error::Invalid(format!(
            "subagent ({model_label}) timed out after {SUBAGENT_TIMEOUT_SECS}s"
        )));
    }
    let _ = handle.await;

    let body = out.trim();
    if body.is_empty() {
        Ok(format!("[subagent {role_name} on {model_label} finished in {rounds_used} rounds with no text]"))
    } else {
        Ok(format!(
            "[subagent: {role_name} · model: {model_label} · rounds: {rounds_used}]\n{body}"
        ))
    }
}

async fn record_tool_result(
    events: &mpsc::Sender<LoopEvent>,
    history: &mut Vec<Message>,
    call: &ToolCall,
    outcome: Result<String>,
) {
    let (output, is_error) = match outcome {
        Ok(text) => (text, false),
        Err(e) => (e.to_string(), true),
    };
    let _ = events
        .send(LoopEvent::ToolFinished {
            call_id: call.id.clone(),
            name: call.function.name.clone(),
            output: truncate_for_display(&output, 4000),
            is_error,
        })
        .await;
    history.push(Message::tool_result(
        call.id.clone(),
        cap_for_history(&output, HISTORY_TOOL_OUTPUT_CAP),
    ));
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

    #[test]
    fn long_tool_output_keeps_head_and_tail_for_history() {
        let big = format!("START{}END", "x".repeat(100_000));
        let out = cap_for_history(&big, 1_000);
        assert!(out.len() < 1_200, "{}", out.len());
        assert!(out.starts_with("START") && out.ends_with("END"));
        assert!(out.contains("elided"));
        assert_eq!(cap_for_history("short", 1_000), "short");
        let multi = "é".repeat(2_000); // must not split a char
        assert!(cap_for_history(&multi, 999).contains("elided"));
    }

    #[test]
    fn empty_or_valid_json_arguments_pass_and_broken_ones_do_not() {
        assert!(arguments_are_valid(""));
        assert!(arguments_are_valid("  "));
        assert!(arguments_are_valid(r#"{"a":1}"#));
        assert!(!arguments_are_valid(r#"{"path":"x","content":"half"#));
    }

    #[test]
    fn compaction_elides_oldest_tool_output_and_keeps_the_tail() {
        let mut h = vec![Message::user("task")];
        for i in 0..10 {
            h.push(Message::tool_result(format!("c{i}"), "y".repeat(10_000)));
        }
        compact_history(&mut h, 50_000, 4);
        let total: usize = h.iter().map(|m| m.content.len()).sum();
        assert!(total <= 50_000 + 1_000, "{total}");
        assert_eq!(h[0].content, "task");
        assert!(h[1].content.contains("elided"));
        assert_eq!(h[1].tool_call_id.as_deref(), Some("c0")); // pairing survives
        assert_eq!(h[10].content.len(), 10_000); // newest kept intact
        let mut small = vec![Message::user("a"), Message::tool_result("c", "b")];
        compact_history(&mut small, 50_000, 4);
        assert_eq!(small[1].content, "b");
    }

    #[test]
    fn agy_loop_step_parsing_and_parallel_wave_safety() {
        let steps = parse_steps(
            r#"{"steps":[{"title":"Inspect crate","status":"done"},{"title":"Run tests","status":"in_progress"}]}"#,
        );
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].title, "Inspect crate");
        assert_eq!(steps[1].status, "in_progress");

        let ask_reg = ToolRegistry::new(false);
        assert!(is_parallel_safe(&ask_reg, &call("read_file", r#"{"path":"x"}"#)));
        assert!(is_parallel_safe(&ask_reg, &call("update_steps", r#"{"steps":[]}"#)));
        assert!(is_parallel_safe(&ask_reg, &call("spawn_subagent", r#"{"task":"check"}"#)));
        assert!(!is_parallel_safe(&ask_reg, &call("write_file", r#"{"path":"x","content":"y"}"#)));
        assert!(!is_parallel_safe(&ask_reg, &call("read_file", r#"{"path":"cut off"#)));
    }

    #[test]
    fn sanitize_and_trim_history_repairs_stopped_calls_for_local_models() {
        let mut stopped = Message::user("");
        stopped.role = "assistant".into();
        stopped.tool_calls = vec![call("shell", r#"{"command":"ls"}"#)];

        let mut empty_assistant = Message::user("");
        empty_assistant.role = "assistant".into();

        let mut history = vec![
            Message::system("sys"),
            Message::user("first"),
            stopped,
            Message::user("rresume"),
            empty_assistant,
            Message::tool_result("orphan_id", "orphan"),
            Message::user("show your plan"),
        ];

        sanitize_history(&mut history);
        assert_eq!(history.len(), 5); // sys, first, stopped, synthetic tool_result, coalesced(rresume, show your plan)
        assert_eq!(history[3].role, "tool");
        assert!(history[3].content.contains("stopped"));
        assert_eq!(history[4].content, "rresume\n\nshow your plan");

        // Two consecutive user turns coalesce cleanly on wire sanitization.
        let mut two_users = vec![
            Message::user("first ask"),
            Message::user("follow-up retry"),
        ];
        sanitize_history(&mut two_users);
        assert_eq!(two_users.len(), 1);
        assert!(two_users[0].content.contains("first ask"));
        assert!(two_users[0].content.contains("follow-up retry"));

        // Tool selection caps bloated 397-tool MCP lists for small local models.
        let mut defs: Vec<crate::api::ToolDef> = vec![
            crate::api::ToolDef {
                name: "shell".into(),
                description: "run".into(),
                parameters: serde_json::json!({}),
            },
            crate::api::ToolDef {
                name: "mcp_mnemai_recall".into(),
                description: "mem".into(),
                parameters: serde_json::json!({}),
            },
        ];
        for i in 0..80 {
            defs.push(crate::api::ToolDef {
                name: format!("mcp_extra_tool_{i}"),
                description: "extra".into(),
                parameters: serde_json::json!({}),
            });
        }
        let local_tools = select_tools_for_model(defs.clone(), Some("lmstudio/liquid/lfm2.5-1.2b"));
        assert_eq!(local_tools.len(), LOCAL_MAX_TOOLS);
        assert!(local_tools.iter().any(|t| t.name == "shell"));
        assert!(local_tools.iter().any(|t| t.name == "mcp_mnemai_recall"));
        assert_eq!(select_tools_for_model(defs, Some("deepseek-flash")).len(), 82);
    }
}
