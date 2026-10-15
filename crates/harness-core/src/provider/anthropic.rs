//! Anthropic Messages API provider with SSE streaming.
//!
//! Wire format ref:
//!   POST https://api.anthropic.com/v1/messages
//!   headers: x-api-key, anthropic-version: 2023-06-01
//!   body: { model, messages, system?, stream, max_tokens, tools? }
//! Anthropic SSE event frames carry a `data:` JSON with `type` and payload.
//!
//! We translate back and forth between our internal `api::Message` JSON
//! (OpenAI-style, "system"/"user"/"assistant"/"tool") and Anthropic's block
//! messages.

use crate::api::StreamEvent;
use crate::error::{Error, Result};
use serde_json::{json, Value};
use tokio::sync::mpsc;

pub fn stream_chat(
    http: reqwest::Client,
    api_key: &str,
    base_url: Option<&str>,
    messages: Vec<crate::api::Message>,
    opts: crate::api::ChatOptions,
) -> mpsc::Receiver<Result<StreamEvent>> {
    let (tx, rx) = mpsc::channel::<Result<StreamEvent>>(256);
    let model_name = opts
        .model_name
        .clone()
        .unwrap_or_else(|| "claude-opus-4-5-20251001".to_string());
    let max_tokens = opts.max_tokens.unwrap_or(8192);
    let base_url = base_url.unwrap_or("https://api.anthropic.com").to_string();
    let http = http.clone();
    let api_key = api_key.to_string();
    let tools = opts.tools.clone();

    // Split system messages and conversation blocks.
    let mut system = String::new();
    let mut anthropic_messages: Vec<Value> = Vec::new();
    for m in &messages {
        if m.role == "system" && !m.content.is_empty() {
            if !system.is_empty() { system.push_str("\n\n"); }
            system.push_str(&m.content);
            continue;
        }
        let mut content = Vec::new();
        if !m.content.is_empty() {
            content.push(json!({"type": "text", "text": m.content}));
        }
        if m.role == "assistant" {
            for tc in &m.tool_calls {
                content.push(json!({"type": "tool_use", "id": tc.id, "name": tc.function.name, "input": tc.function.arguments}));
            }
        }
        if m.role == "tool" {
            content.push(json!({"type": "tool_result", "tool_use_id": m.tool_call_id, "content": m.content}));
        }
        anthropic_messages.push(json!({"role": role_map(&m.role), "content": content}));
    }

    tokio::spawn(async move {
        let mut body = json!({
            "model": model_name,
            "messages": anthropic_messages,
            "max_tokens": max_tokens,
            "stream": true,
        });
        if !system.is_empty() { body["system"] = json!(system); }
        if !tools.is_empty() {
            body["tools"] = json!(tools.iter().map(|t| json!({
                "name": t.name, "description": t.description, "input_schema": t.parameters
            })).collect::<Vec<_>>());
        }

        let resp = http
            .post(format!("{}/v1/messages", base_url.trim_end_matches('/')))
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await;
        let resp = match resp {
            Ok(r) => r,
            Err(e) => { let _ = tx.send(Err(Error::Http(e))).await; return; }
        };
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            let _ = tx.send(Err(Error::Api { status, message })).await;
            return;
        }

        let mut stream = resp.bytes_stream();
        use futures::StreamExt;
        let mut buf = String::new();
        loop {
            match stream.next().await {
                Some(Ok(b)) => {
                    buf.push_str(&String::from_utf8_lossy(&b));
                    // SSE frames separated by blank lines.
                    while let Some(pos) = buf.find("\n\n") {
                        let frame: String = buf.drain(..pos + 2).collect();
                        parse_frame(&frame, &tx).await;
                    }
                }
                Some(Err(e)) => {
                    let _ = tx.send(Err(Error::Http(e))).await;
                    return;
                }
                None => break,
            }
        }
        let _ = tx.send(Ok(StreamEvent::Done { finish_reason: Some("stop".into()), usage: None })).await;
    });
    rx
}

fn role_map(role: &str) -> &'static str {
    match role {
        "assistant" => "assistant",
        _ => "user",  // user, tool-result (wrapped as user blocks above)
    }
}

async fn parse_frame(frame: &str, tx: &mpsc::Sender<Result<StreamEvent>>) {
    for line in frame.lines() {
        let payload = match line.strip_prefix("data:") {
            Some(p) => p.trim(),
            None => continue,
        };
        if payload.is_empty() || payload == "[DONE]" { continue; }
        let v: Value = match serde_json::from_str(payload) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let etype = v.get("type").and_then(Value::as_str).unwrap_or_default();
        match etype {
            "content_block_delta" => {
                if let Some(delta) = v.get("delta") {
                    if let Some(text) = delta.get("text").and_then(Value::as_str) {
                        if !text.is_empty() {
                            let _ = tx.send(Ok(StreamEvent::Content(text.to_string()))).await;
                        }
                    }
                    if let Some(t) = delta.get("thinking").and_then(Value::as_str) {
                        if !t.is_empty() {
                            let _ = tx.send(Ok(StreamEvent::Reasoning(t.to_string()))).await;
                        }
                    }
                    if let Some(t) = delta.get("partial_json").and_then(Value::as_str) {
                        if !t.is_empty() {
                            // Keep accumulating caller-side; emit content delta
                            let _ = tx.send(Ok(StreamEvent::Content(t.to_string()))).await;
                        }
                    }
                }
            }
            "content_block_start" => {
                let index = v.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                if let Some(cb) = v.get("content_block") {
                    let ctype = cb.get("type").and_then(Value::as_str).unwrap_or_default();
                    if ctype == "tool_use" {
                        let id = cb.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
                        let name = cb.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
                        let _ = tx.send(Ok(StreamEvent::ToolCallStart { index, id, name })).await;
                    }
                }
            }
            "message_stop" => {
                let _ = tx.send(Ok(StreamEvent::Done { finish_reason: Some("stop".into()), usage: None })).await;
            }
            _ => {}
        }
    }
}

// expose parse logic for tests via a thin pub fn (see parse_anthropic_frame)
pub fn parse_anthropic_frame(frame: &str) -> Vec<StreamEvent> {
    // Not exact because parse_frame uses tx; replicate minimal.
    let mut events = Vec::new();
    for line in frame.lines() {
        let payload = match line.strip_prefix("data:") { Some(p)=>p.trim(), None=>continue };
        if payload.is_empty() || payload == "[DONE]" { continue; }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else { continue };
        let etype = v.get("type").and_then(serde_json::Value::as_str).unwrap_or_default();
        match etype {
            "content_block_delta" => {
                let delta = v.get("delta").cloned().unwrap_or_else(|| serde_json::json!({}));
                if let Some(text) = delta.get("text").and_then(serde_json::Value::as_str) {
                    if !text.is_empty() { events.push(StreamEvent::Content(text.to_string())); }
                }
                if let Some(t) = delta.get("thinking").and_then(serde_json::Value::as_str) {
                    if !t.is_empty() { events.push(StreamEvent::Reasoning(t.to_string())); }
                }
            }
            "content_block_start" => {
                let index = v.get("index").and_then(serde_json::Value::as_u64).unwrap_or(0) as usize;
                if let Some(cb) = v.get("content_block") {
                    if cb.get("type").and_then(serde_json::Value::as_str) == Some("tool_use") {
                        events.push(StreamEvent::ToolCallStart {
                            index,
                            id: cb.get("id").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
                            name: cb.get("name").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    events
}
