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

use crate::api::{ChatOptions, Message, StreamEvent};
use crate::error::{Error, Result};
use serde_json::{json, Value};
use tokio::sync::mpsc;

pub fn stream_chat(
    http: reqwest::Client,
    api_key: &str,
    base_url: Option<&str>,
    messages: Vec<Message>,
    opts: ChatOptions,
) -> mpsc::Receiver<Result<StreamEvent>> {
    let (tx, rx) = mpsc::channel::<Result<StreamEvent>>(256);
    let base_url = base_url.unwrap_or("https://api.anthropic.com").to_string();
    let http = http.clone();
    let api_key = api_key.to_string();
    let body = build_anthropic_body(&messages, &opts);

    tokio::spawn(async move {
        let resp = tokio::time::timeout(
            crate::api::STREAM_IDLE_TIMEOUT,
            http.post(format!("{}/v1/messages", base_url.trim_end_matches('/')))
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
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
            Err(_) => {
                let _ = tx
                    .send(Err(Error::Api {
                        status: 0,
                        message: format!(
                            "no response after {}s — server stalled, request dropped",
                            crate::api::STREAM_IDLE_TIMEOUT.as_secs()
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
        use futures::StreamExt;
        let mut buf = String::new();
        let mut finish_reason: Option<String> = None;
        let mut sent_done = false;
        loop {
            let next =
                match tokio::time::timeout(crate::api::STREAM_IDLE_TIMEOUT, stream.next()).await {
                    Ok(n) => n,
                    Err(_) => {
                        let _ = tx
                            .send(Err(Error::Api {
                                status: 0,
                                message: "stream idle for 90s — server stalled".into(),
                            }))
                            .await;
                        return;
                    }
                };
            match next {
                Some(Ok(b)) => {
                    buf.push_str(&String::from_utf8_lossy(&b));
                    while let Some(pos) = buf.find("\n\n") {
                        let frame: String = buf.drain(..pos + 2).collect();
                        if !dispatch_frame(&frame, &tx, &mut finish_reason, &mut sent_done).await {
                            return;
                        }
                    }
                }
                Some(Err(e)) => {
                    let _ = tx.send(Err(Error::Http(e))).await;
                    return;
                }
                None => {
                    if !buf.trim().is_empty()
                        && !dispatch_frame(&buf, &tx, &mut finish_reason, &mut sent_done).await
                    {
                        return;
                    }
                    break;
                }
            }
        }
        if !sent_done {
            let _ = tx
                .send(Ok(StreamEvent::Done {
                    finish_reason: Some(finish_reason.unwrap_or_else(|| "stop".into())),
                    usage: None,
                }))
                .await;
        }
    });
    rx
}

async fn dispatch_frame(
    frame: &str,
    tx: &mpsc::Sender<Result<StreamEvent>>,
    finish_reason: &mut Option<String>,
    sent_done: &mut bool,
) -> bool {
    if let Some(err) = crate::provider::extract_frame_error(frame) {
        let _ = tx.send(Err(err)).await;
        return false;
    }
    for ev in parse_anthropic_frame(frame) {
        match ev {
            StreamEvent::Done {
                finish_reason: Some(fr),
                usage: None,
            } => {
                // `message_delta` sets "tool_calls" or "length"; `message_stop`
                // emits "stop" without knowing earlier stop_reason, so preserve
                // any non-"stop" reason recorded earlier.
                if finish_reason.is_none() || fr != "stop" {
                    *finish_reason = Some(fr);
                }
                if frame_has_type(frame, "message_stop") {
                    *sent_done = true;
                    if tx
                        .send(Ok(StreamEvent::Done {
                            finish_reason: finish_reason.clone(),
                            usage: None,
                        }))
                        .await
                        .is_err()
                    {
                        return false;
                    }
                }
            }
            other => {
                if tx.send(Ok(other)).await.is_err() {
                    return false;
                }
            }
        }
    }
    true
}

fn frame_has_type(frame: &str, expected: &str) -> bool {
    frame.lines().any(|line| {
        line.strip_prefix("data:")
            .and_then(|p| serde_json::from_str::<Value>(p.trim()).ok())
            .and_then(|v| v.get("type").and_then(Value::as_str).map(|s| s == expected))
            .unwrap_or(false)
    })
}

/// Build an Anthropic `/v1/messages` request body from internal messages & options.
pub fn build_anthropic_body(messages: &[Message], opts: &ChatOptions) -> Value {
    let raw_model = opts
        .model_name
        .as_deref()
        .unwrap_or("claude-opus-4-5-20251001");
    let model_name = raw_model.strip_prefix("anthropic/").unwrap_or(raw_model);
    let max_tokens = opts.max_tokens.unwrap_or(8192);

    let mut system = String::new();
    let mut anthropic_messages: Vec<Value> = Vec::new();
    for m in messages {
        if m.role == "system" {
            if !m.content.is_empty() {
                if !system.is_empty() {
                    system.push_str("\n\n");
                }
                system.push_str(&m.content);
            }
            continue;
        }

        let mut content = Vec::new();
        if m.role == "tool" {
            content.push(json!({
                "type": "tool_result",
                "tool_use_id": m.tool_call_id,
                "content": m.content,
            }));
        } else {
            if !m.content.is_empty() {
                content.push(json!({"type": "text", "text": m.content}));
            }
            if m.role == "user" {
                for img in &m.images {
                    content.push(json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": img.mime,
                            "data": img.data,
                        }
                    }));
                }
            }
            if m.role == "assistant" {
                for tc in &m.tool_calls {
                    content.push(json!({
                        "type": "tool_use",
                        "id": tc.id,
                        "name": tc.function.name,
                        "input": crate::provider::parse_args_object(&tc.function.arguments),
                    }));
                }
            }
        }

        if content.is_empty() {
            continue;
        }
        let mapped = role_map(&m.role);
        if let Some(last) = anthropic_messages.last_mut() {
            if last.get("role").and_then(Value::as_str) == Some(mapped) {
                if let Some(arr) = last.get_mut("content").and_then(Value::as_array_mut) {
                    arr.extend(content);
                    continue;
                }
            }
        }
        anthropic_messages.push(json!({"role": mapped, "content": content}));
    }

    let mut body = json!({
        "model": model_name,
        "messages": anthropic_messages,
        "max_tokens": max_tokens,
        "stream": true,
    });
    if !system.is_empty() {
        body["system"] = json!(system);
    }
    if !opts.tools.is_empty() {
        body["tools"] = json!(opts
            .tools
            .iter()
            .map(|t| json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters,
            }))
            .collect::<Vec<_>>());
    }
    body
}

fn role_map(role: &str) -> &'static str {
    match role {
        "assistant" => "assistant",
        _ => "user", // user and tool_result blocks both go under "user"
    }
}

/// Parse one Anthropic SSE frame into zero or more `StreamEvent`s.
pub fn parse_anthropic_frame(frame: &str) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    for line in frame.lines() {
        let payload = match line.strip_prefix("data:") {
            Some(p) => p.trim(),
            None => continue,
        };
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(payload) else {
            continue;
        };
        let etype = v.get("type").and_then(Value::as_str).unwrap_or_default();
        match etype {
            "content_block_delta" => {
                let index = v.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                if let Some(delta) = v.get("delta") {
                    if let Some(text) = delta.get("text").and_then(Value::as_str) {
                        if !text.is_empty() {
                            events.push(StreamEvent::Content(text.to_string()));
                        }
                    }
                    if let Some(t) = delta.get("thinking").and_then(Value::as_str) {
                        if !t.is_empty() {
                            events.push(StreamEvent::Reasoning(t.to_string()));
                        }
                    }
                    if let Some(t) = delta.get("partial_json").and_then(Value::as_str) {
                        if !t.is_empty() {
                            events.push(StreamEvent::ToolCallArgs {
                                index,
                                delta: t.to_string(),
                            });
                        }
                    }
                }
            }
            "content_block_start" => {
                let index = v.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                if let Some(cb) = v.get("content_block") {
                    if cb.get("type").and_then(Value::as_str) == Some("tool_use") {
                        events.push(StreamEvent::ToolCallStart {
                            index,
                            id: cb
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            name: cb
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                        });
                    }
                }
            }
            "message_delta" => {
                if let Some(sr) = v
                    .get("delta")
                    .and_then(|d| d.get("stop_reason"))
                    .and_then(Value::as_str)
                {
                    let mapped = match sr {
                        "tool_use" => "tool_calls",
                        "max_tokens" => "length",
                        _ => "stop",
                    };
                    events.push(StreamEvent::Done {
                        finish_reason: Some(mapped.to_string()),
                        usage: None,
                    });
                }
            }
            "message_stop" => {
                events.push(StreamEvent::Done {
                    finish_reason: Some("stop".into()),
                    usage: None,
                });
            }
            _ => {}
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ToolCall;

    #[test]
    fn anthropic_body_uses_object_input_and_merges_tool_results() {
        let mut assistant = Message::user("");
        assistant.role = "assistant".into();
        assistant.tool_calls = vec![
            ToolCall::new("tu_1", 0, "read_file", r#"{"path":"a.txt"}"#),
            ToolCall::new("tu_2", 1, "read_file", ""),
        ];
        let messages = vec![
            Message::system("sys"),
            Message::user("check files"),
            assistant,
            Message::tool_result("tu_1", "alpha"),
            Message::tool_result("tu_2", "beta"),
        ];
        let body = build_anthropic_body(&messages, &ChatOptions::default());
        assert_eq!(body["system"], json!("sys"));
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3); // user, assistant, merged user(tool_results)
        let tu_blocks = msgs[1]["content"].as_array().unwrap();
        assert_eq!(tu_blocks[0]["input"], json!({"path": "a.txt"}));
        assert_eq!(tu_blocks[1]["input"], json!({}));
        let tr_blocks = msgs[2]["content"].as_array().unwrap();
        assert_eq!(tr_blocks.len(), 2);
        assert_eq!(tr_blocks[0]["type"], json!("tool_result"));
        assert_eq!(tr_blocks[0]["tool_use_id"], json!("tu_1"));
        assert_eq!(tr_blocks[1]["tool_use_id"], json!("tu_2"));
    }

    #[test]
    fn parse_anthropic_frame_routes_partial_json_and_tool_stop_reason() {
        let start = "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_9\",\"name\":\"shell\"}}\n\n";
        let delta = "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"cmd\\\":\\\"ls\\\"}\"}}\n\n";
        let done = "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"}}\n\n";

        let ev1 = parse_anthropic_frame(start);
        assert!(matches!(&ev1[0], StreamEvent::ToolCallStart { index: 1, id, name } if id == "tu_9" && name == "shell"));

        let ev2 = parse_anthropic_frame(delta);
        assert!(matches!(&ev2[0], StreamEvent::ToolCallArgs { index: 1, delta } if delta.contains("ls")));

        let ev3 = parse_anthropic_frame(done);
        assert!(matches!(&ev3[0], StreamEvent::Done { finish_reason: Some(r), .. } if r == "tool_calls"));
    }
}
