//! DeepSeek chat-completions client (OpenAI-compatible) with SSE streaming.

use std::time::Duration;

use crate::error::{Error, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::time::timeout;

pub const API_BASE: &str = "https://api.deepseek.com";

/// Max silence between SSE chunks before the stream is declared dead.
/// Generous: reasoning models pause between tokens, but servers send
/// keepalives well inside this window.
pub(crate) const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Model {
    #[serde(rename = "deepseek-flash")]
    Flash,
    #[serde(rename = "deepseek-v4-pro")]
    V4Pro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReasoningEffort {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "high")]
    High,
    #[serde(rename = "max")]
    Max,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    /// Position while streaming deltas arrive. Local bookkeeping only — the
    /// API rejects unknown members on tool calls.
    #[serde(default, skip_serializing)]
    pub index: usize,
    /// Always `"function"`. Required by the API on every assistant tool call.
    #[serde(rename = "type", default = "tool_kind")]
    pub kind: String,
    pub function: FunctionCall,
}

/// The only tool-call kind the API accepts today.
pub fn tool_kind() -> String {
    "function".to_string()
}

impl ToolCall {
    pub fn new(
        id: impl Into<String>,
        index: usize,
        name: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            index,
            kind: tool_kind(),
            function: FunctionCall {
                name: name.into(),
                arguments: arguments.into(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub arguments: String,
}

/// One image attached to a user turn, already base64-encoded.
///
/// The wire shape differs per provider (OpenAI `image_url`, Anthropic `image`
/// source, Gemini `inlineData`), so the transports translate it themselves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImagePart {
    /// e.g. `image/png`
    pub mime: String,
    /// Standard base64, no data-URI prefix.
    pub data: String,
}

impl ImagePart {
    /// `data:image/png;base64,…` — the OpenAI-compatible shape.
    pub fn data_uri(&self) -> String {
        format!("data:{};base64,{}", self.mime, self.data)
    }
}

/// One message in a conversation, on our side and on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String, // system | user | assistant | tool
    #[serde(default)]
    pub content: String,
    /// Images attached to a user turn. Kept out of the OpenAI-compatible
    /// `messages` serialization — [`wire_message`] expands them into a
    /// content-part array instead.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImagePart>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reasoning_content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Session-local timestamp (not sent to the API).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts: Option<chrono::DateTime<chrono::Utc>>,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
            images: Vec::new(),
            reasoning_content: String::new(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            ts: Some(chrono::Utc::now()),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        let mut m = Self::user(content);
        m.role = "system".into();
        m
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        let mut m = Self::user(content);
        m.role = "tool".into();
        m.tool_call_id = Some(tool_call_id.into());
        m
    }

    /// Copy with display-only fields stripped for the wire: the API rejects
    /// echoed reasoning and has no use for our session-local timestamp.
    pub fn to_api(&self) -> Self {
        Self {
            reasoning_content: String::new(),
            ts: None,
            ..self.clone()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// JSON Schema object for `parameters`.
    pub parameters: Value,
}

/// Coerce a provider-supplied schema into what the API demands.
///
/// The API rejects anything that is not a JSON Schema object of
/// `type: "object"` — an empty `[cmd.x] schema` table or an MCP server that
/// omits `type` both produce a 400 otherwise.
pub fn normalize_parameters(value: Value) -> Value {
    match value {
        Value::Object(mut map) => {
            map.entry("type")
                .or_insert_with(|| Value::String("object".into()));
            map.entry("properties").or_insert_with(|| json!({}));
            Value::Object(map)
        }
        _ => json!({"type": "object", "properties": {}}),
    }
}

#[derive(Debug, Default, Clone)]
pub struct ChatOptions {
    pub model: Option<Model>,
    /// Override model ID string (for multi-provider routing). When set,
    /// takes precedence over `model`.
    pub model_name: Option<String>,
    pub effort: Option<ReasoningEffort>,
    pub max_tokens: Option<u32>,
    pub tools: Vec<ToolDef>,
    /// Base URL for the chat completions endpoint (only for OpenAI-compatible
    /// providers). Default uses DeepSeek endpoint if None.
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingSwitch {
    #[serde(rename = "type")]
    pub kind: String, // "enabled" | "disabled"
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
}

/// Events emitted while a streamed completion is in flight.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    Reasoning(String),
    Content(String),
    ToolCallStart { index: usize, id: String, name: String },
    ToolCallArgs { index: usize, delta: String },
    Done { finish_reason: Option<String>, usage: Option<Usage> },
}

/// Parse one SSE `data:` payload into stream events. Empty result = not a data
/// chunk we care about (e.g. `[DONE]`, comments, keepalives).
pub fn parse_stream_chunk(payload: &str) -> Result<Vec<StreamEvent>> {
    let payload = payload.trim();
    if payload.is_empty() || payload == "[DONE]" {
        return Ok(Vec::new());
    }
    let v: Value = serde_json::from_str(payload)?;
    let mut events = Vec::new();
    let Some(choices) = v.get("choices").and_then(Value::as_array) else {
        return Ok(events);
    };
    for choice in choices {
        let Some(delta) = choice.get("delta") else { continue };
        if let Some(r) = delta.get("reasoning_content").and_then(Value::as_str) {
            if !r.is_empty() {
                events.push(StreamEvent::Reasoning(r.to_string()));
            }
        }
        if let Some(c) = delta.get("content").and_then(Value::as_str) {
            if !c.is_empty() {
                events.push(StreamEvent::Content(c.to_string()));
            }
        }
        if let Some(tcs) = delta.get("tool_calls").and_then(Value::as_array) {
            for tc in tcs {
                let index = tc.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                if let Some(id) = tc.get("id").and_then(Value::as_str) {
                    let name = tc
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    events.push(StreamEvent::ToolCallStart {
                        index,
                        id: id.to_string(),
                        name: name.to_string(),
                    });
                }
                if let Some(args) = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(Value::as_str)
                {
                    if !args.is_empty() {
                        events.push(StreamEvent::ToolCallArgs {
                            index,
                            delta: args.to_string(),
                        });
                    }
                }
            }
        }
        if let Some(fr) = choice.get("finish_reason").and_then(Value::as_str) {
            let usage = v.get("usage").and_then(|u| {
                if u.is_null() {
                    None
                } else {
                    serde_json::from_value::<Usage>(u.clone()).ok()
                }
            });
            events.push(StreamEvent::Done {
                finish_reason: Some(fr.to_string()),
                usage,
            });
        }
    }
    Ok(events)
}

/// One message in the OpenAI-compatible wire shape.
///
/// Messages without images keep `content` as a plain string (the format every
/// compatible server understands). With images, `content` becomes the
/// multimodal part array. `images` itself is never sent as a field.
fn wire_message(m: &Message) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("role".into(), json!(m.role));
    if m.images.is_empty() {
        out.insert("content".into(), json!(m.content));
    } else {
        let mut parts = Vec::with_capacity(m.images.len() + 1);
        if !m.content.is_empty() {
            parts.push(json!({"type": "text", "text": m.content}));
        }
        for img in &m.images {
            parts.push(json!({
                "type": "image_url",
                "image_url": { "url": img.data_uri() }
            }));
        }
        out.insert("content".into(), Value::Array(parts));
    }
    if !m.tool_calls.is_empty() {
        out.insert(
            "tool_calls".into(),
            serde_json::to_value(&m.tool_calls).unwrap_or(Value::Null),
        );
    }
    if let Some(id) = &m.tool_call_id {
        out.insert("tool_call_id".into(), json!(id));
    }
    Value::Object(out)
}

fn build_body(messages: &[Message], opts: &ChatOptions) -> Value {
    // Use explicit model_name string if provided (multi-provider routing),
    // else fall back to the Model enum (legacy DeepSeek).
    // model string: explicit name overrides the enum.
    let model_str = opts
        .model_name
        .clone()
        .unwrap_or_else(|| match opts.model.unwrap_or(Model::Flash) {
            Model::Flash => "deepseek-flash".to_string(),
            Model::V4Pro => "deepseek-v4-pro".to_string(),
        });
    // A routing prefix (`openrouter/`, `fpt/`, `deepseek/`, …) belongs to the
    // factory, not the wire. Providers expect the bare model id.
    let model = crate::provider::strip_routing_prefix(&model_str).to_string();
    let effort = opts.effort.unwrap_or(ReasoningEffort::None);
    let thinking = match effort {
        ReasoningEffort::None => ThinkingSwitch { kind: "disabled".into() },
        _ => ThinkingSwitch { kind: "enabled".into() },
    };
    let tools = if opts.tools.is_empty() {
        None
    } else {
        Some(
            opts.tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect::<Vec<_>>(),
        )
    };
    let effort_value = match effort {
        ReasoningEffort::None => serde_json::Value::Null,
        e => serde_json::to_value(e).unwrap_or(serde_json::Value::Null),
    };
    serde_json::json!({
        "model": model,
        "messages": messages.iter().map(wire_message).collect::<Vec<_>>(),
        "stream": true,
        "stream_options": { "include_usage": true },
        "thinking": thinking,
        "reasoning_effort": effort_value,
        "max_tokens": opts.max_tokens,
        "tools": tools,
    })
}

/// Kick off a streamed completion. Returns a receiver of events; errors are
/// delivered as `Err` over the channel. Dropping the receiver cancels the task.
pub fn stream_chat(
    http: reqwest::Client,
    api_key: &str,
    messages: Vec<Message>,
    opts: ChatOptions,
) -> tokio::sync::mpsc::Receiver<Result<StreamEvent>> {
    // Defer to vendor-specific transports where needed so the OpenAI-compat
    // path stays the default for the openapi-compatible 90%.
    if let Some(model) = opts.model_name.as_deref() {
        let base_url = opts.base_url.clone();
        let family = crate::provider::provider_family(model);
        if family == "anthropic" || model.starts_with("claude/") {
            return crate::provider::stream_anthropic(
                http, api_key, base_url.as_deref(), messages, opts,
            );
        }
        if family == "google" || model.starts_with("gemini/") {
            return crate::provider::stream_google(
                http, api_key, base_url.as_deref(), messages, opts,
            );
        }
    }
    // re-bind opts if not moved (it wasn't moved in the non-delegating path) 
    let _ = &opts;
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<StreamEvent>>(256);
    let api_key = api_key.to_string();
    tokio::spawn(async move {
        let body = build_body(&messages, &opts);
        let base = opts.base_url.clone().unwrap_or_else(|| API_BASE.to_string());
        // A server may accept the connection and then never send response
        // headers. The body-stream timeout below cannot see that, so the
        // request itself needs the same deadline or the turn hangs forever
        // with the UI stuck on "Working…".
        let resp = timeout(
            STREAM_IDLE_TIMEOUT,
            http.post(format!("{}/chat/completions", base.trim_end_matches('/')))
                .bearer_auth(api_key)
                .json(&body)
                .send(),
        )
        .await;
        let resp = match resp {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                let _ = tx.send(Err(Error::Http(e))).await;
                return;
            }
            Err(_elapsed) => {
                let _ = tx
                    .send(Err(Error::Api {
                        status: 0,
                        message: format!(
                            "no response after {}s — server stalled, request dropped",
                            STREAM_IDLE_TIMEOUT.as_secs()
                        ),
                    }))
                    .await;
                return;
            }
        };
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            let _ = tx.send(Err(Error::Api { status, message })).await;
            return;
        }
        let mut stream = resp.bytes_stream();
        let mut buf = String::new();
        loop {
            let chunk = match timeout(STREAM_IDLE_TIMEOUT, stream.next()).await {
                Ok(Some(c)) => c,
                Ok(None) => break, // stream ended cleanly
                Err(_elapsed) => {
                    let _ = tx
                        .send(Err(Error::Api {
                            status: 0,
                            message: format!(
                                "stream idle for {}s — server stalled, connection dropped",
                                STREAM_IDLE_TIMEOUT.as_secs()
                            ),
                        }))
                        .await;
                    return;
                }
            };
            let bytes = match chunk {
                Ok(b) => b,
                Err(e) => {
                    let _ = tx.send(Err(Error::Http(e))).await;
                    return;
                }
            };
            buf.push_str(&String::from_utf8_lossy(&bytes));
            // SSE frames are separated by blank lines.
            while let Some(pos) = buf.find("\n\n") {
                let frame: String = buf.drain(..pos + 2).collect();
                for line in frame.lines() {
                    let Some(payload) = line.strip_prefix("data:") else { continue };
                    match parse_stream_chunk(payload) {
                        Ok(events) => {
                            for ev in events {
                                if tx.send(Ok(ev)).await.is_err() {
                                    return; // receiver dropped: cancelled
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(e)).await;
                            return;
                        }
                    }
                }
            }
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_and_reasoning_deltas() {
        let payload = r#"{"id":"1","choices":[{"index":0,"delta":{"content":"Hi","reasoning_content":"think"},"finish_reason":null}]}"#;
        let events = parse_stream_chunk(payload).unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], StreamEvent::Reasoning(r) if r == "think"));
        assert!(matches!(&events[1], StreamEvent::Content(c) if c == "Hi"));
    }

    #[test]
    fn parses_tool_call_start_and_args() {
        let start = r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"echo","arguments":""}}]},"finish_reason":null}]}"#;
        let args = r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"text\":"}}]},"finish_reason":null}]}"#;
        let events = parse_stream_chunk(start).unwrap();
        assert!(matches!(&events[0], StreamEvent::ToolCallStart { name, .. } if name == "echo"));
        let events = parse_stream_chunk(args).unwrap();
        assert!(matches!(&events[0], StreamEvent::ToolCallArgs { delta, .. } if delta.contains("text")));
    }

    #[test]
    fn parses_done_with_usage() {
        let payload = r#"{"choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":17,"completion_tokens":9,"total_tokens":26}}"#;
        let events = parse_stream_chunk(payload).unwrap();
        match &events[0] {
            StreamEvent::Done { finish_reason, usage } => {
                assert_eq!(finish_reason.as_deref(), Some("stop"));
                assert_eq!(usage.as_ref().unwrap().total_tokens, 26);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn ignores_done_sentinel_and_blank() {
        assert!(parse_stream_chunk("[DONE]").unwrap().is_empty());
        assert!(parse_stream_chunk("  ").unwrap().is_empty());
    }

    #[test]
    fn body_disables_thinking_on_none_effort() {
        let opts = ChatOptions { effort: Some(ReasoningEffort::None), ..Default::default() };
        let body = build_body(&[], &opts);
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body["reasoning_effort"].is_null());
    }

    #[test]
    fn body_sends_the_wire_model_without_the_factory_prefix() {
        let opts = ChatOptions {
            model_name: Some("openrouter/anthropic/claude-3.5-sonnet".into()),
            ..Default::default()
        };
        let body = build_body(&[], &opts);
        assert_eq!(body["model"], "anthropic/claude-3.5-sonnet");
    }

    #[test]
    fn body_enables_thinking_and_tools() {
        let opts = ChatOptions {
            effort: Some(ReasoningEffort::High),
            tools: vec![ToolDef {
                name: "echo".into(),
                description: "echo".into(),
                parameters: serde_json::json!({"type":"object","properties":{}}),
            }],
            ..Default::default()
        };
        let body = build_body(&[], &opts);
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["reasoning_effort"], "high");
        assert_eq!(body["tools"][0]["function"]["name"], "echo");
    }

    #[test]
    fn tool_result_message_serializes_with_call_id() {
        let m = Message::tool_result("call_1", "output");
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["role"], "tool");
        assert_eq!(v["tool_call_id"], "call_1");
    }

    #[test]
    fn to_api_strips_display_fields() {
        let mut m = Message::user("hi");
        m.reasoning_content = "secret thought".into();
        let wire = m.to_api();
        assert!(wire.reasoning_content.is_empty());
        assert!(wire.ts.is_none());
        assert_eq!(wire.content, "hi");
    }

    #[test]
    fn text_message_stays_a_plain_string_on_the_wire() {
        let m = Message::user("hello");
        let body = build_body(&[m], &ChatOptions::default());
        assert_eq!(body["messages"][0]["content"], "hello");
        assert!(body["messages"][0].get("images").is_none());
    }

    #[test]
    fn image_message_becomes_content_parts() {
        let mut m = Message::user("what is this?");
        m.images = vec![ImagePart {
            mime: "image/png".into(),
            data: "aGVsbG8=".into(),
        }];
        let body = build_body(&[m], &ChatOptions::default());
        let content = &body["messages"][0]["content"];
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "what is this?");
        assert_eq!(content[1]["type"], "image_url");
        assert_eq!(
            content[1]["image_url"]["url"],
            "data:image/png;base64,aGVsbG8="
        );
        assert!(
            body["messages"][0].get("images").is_none(),
            "images are expanded into content, never sent as a field"
        );
    }

    #[test]
    fn image_only_message_has_no_empty_text_part() {
        let mut m = Message::user("");
        m.images = vec![ImagePart {
            mime: "image/jpeg".into(),
            data: "eA==".into(),
        }];
        let body = build_body(&[m], &ChatOptions::default());
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "image_url");
    }

    /// The API rejected a request with `missing field type` — every assistant
    /// tool call must carry `"type": "function"`, and nothing else.
    #[test]
    fn assistant_tool_calls_serialize_for_the_wire() {
        let mut m = Message::user("");
        m.role = "assistant".into();
        m.tool_calls = vec![ToolCall::new("call_1", 0, "cmd_echo", "{\"text\":\"hi\"}")];

        let v = serde_json::to_value(&m).unwrap();
        let call = &v["tool_calls"][0];

        assert_eq!(call["id"], "call_1");
        assert_eq!(call["type"], "function");
        assert_eq!(call["function"]["name"], "cmd_echo");
        assert_eq!(call["function"]["arguments"], "{\"text\":\"hi\"}");
        assert!(
            call.get("index").is_none(),
            "index is local bookkeeping and must not reach the API"
        );
    }

    #[test]
    fn empty_parameters_become_an_object_schema() {
        let out = normalize_parameters(json!({}));
        assert_eq!(out["type"], "object");
        assert_eq!(out["properties"], json!({}));
    }

    #[test]
    fn partial_schemas_get_type_injected() {
        let out = normalize_parameters(json!({
            "properties": {"text": {"type": "string"}},
            "required": ["text"]
        }));
        assert_eq!(out["type"], "object");
        assert_eq!(out["properties"]["text"]["type"], "string");
        assert_eq!(out["required"][0], "text");
    }

    #[test]
    fn complete_schemas_pass_through_and_non_objects_fall_back() {
        let full = json!({"type": "object", "properties": {"a": {"type": "number"}}});
        assert_eq!(normalize_parameters(full.clone()), full);
        let out = normalize_parameters(json!("not-a-schema"));
        assert_eq!(out["type"], "object");
    }

    #[test]
    fn tool_call_round_trips_without_index() {
        let call = ToolCall::new("call_9", 3, "ping", "{}");
        let json = serde_json::to_string(&call).unwrap();
        assert!(!json.contains("index"));
        let back: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(back.index, 0); // defaulted, never needed after streaming
        assert_eq!(back.kind, "function");
        assert_eq!(back.function.name, "ping");
    }
}
