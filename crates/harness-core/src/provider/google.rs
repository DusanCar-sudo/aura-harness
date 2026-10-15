//! Google Gemini generateContent stream provider.
//! Endpoint: POST https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent?key={api_key}&alt=sse
//! Body: { contents, systemInstruction?, generationConfig }. Parse candidate parts.

use crate::api::StreamEvent;
use crate::error::{Error, Result};
use serde_json::{json, Value};
use tokio::sync::mpsc;

pub fn stream_chat(
    http: reqwest::Client,
    api_key: &str,
    base_url: Option<&str>,
    mut _messages: Vec<crate::api::Message>,
    opts: crate::api::ChatOptions,
) -> mpsc::Receiver<Result<StreamEvent>> {
    let (tx, rx) = mpsc::channel::<Result<StreamEvent>>(256);
    let model_name = opts
        .model_name
        .clone()
        .unwrap_or_else(|| "gemini-2.5-flash".to_string());
    let model_name = model_name.strip_prefix("gemini/").unwrap_or(&model_name).to_string();
    let max_tokens = opts.max_tokens.unwrap_or(8192);
    let base = base_url.unwrap_or("https://generativelanguage.googleapis.com").to_string();
    let api_key = api_key.to_string();
    let http = http.clone();

    // Translate messages into Gemini `contents`.
    let mut contents = Vec::new();
    let mut system_text = String::new();
    for m in &_messages {
        match m.role.as_str() {
            "system" => {
                if !m.content.is_empty() {
                    if !system_text.is_empty() { system_text.push_str("\n\n"); }
                    system_text.push_str(&m.content);
                }
            }
            "assistant" => {
                let mut parts = Vec::new();
                if !m.content.is_empty() {
                    parts.push(json!({"text": m.content}));
                }
                for tc in &m.tool_calls {
                    parts.push(json!({"functionCall": {"name": tc.function.name, "args": tc.function.arguments}}));
                }
                if !parts.is_empty() {
                    contents.push(json!({"role": "model", "parts": parts}));
                }
            }
            "tool" => {
                contents.push(json!({"role": "user", "parts": [{"text": format!("Tool result: {}", m.content)}]}));
            }
            _ => {
                if !m.content.is_empty() {
                    contents.push(json!({"role": "user", "parts": [{"text": m.content}]}));
                }
            }
        }
    }

    let mut body = json!({
        "contents": contents,
        "generationConfig": { "maxOutputTokens": max_tokens },
    });
    if !system_text.is_empty() {
        body["systemInstruction"] = json!({"parts": [{"text": system_text}]});
    }

    tokio::spawn(async move {
        let url = format!("{}/v1beta/models/{}:streamGenerateContent?key={}&alt=sse", base.trim_end_matches('/'), model_name, api_key);
        let resp = http.post(&url).json(&body).send().await;
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

async fn parse_frame(frame: &str, tx: &mpsc::Sender<Result<StreamEvent>>) {
    for line in frame.lines() {
        let payload = match line.strip_prefix("data:") {
            Some(p) => p.trim(),
            None => continue,
        };
        if payload.is_empty() || payload == "[DONE]" { continue; }
        let Ok(v) = serde_json::from_str::<Value>(payload) else { continue; };
        if let Some(cands) = v.get("candidates").and_then(Value::as_array) {
            for c in cands {
                if let Some(parts) = c.get("content").and_then(|c| c.get("parts")).and_then(Value::as_array) {
                    for p in parts {
                        if let Some(t) = p.get("text").and_then(Value::as_str) {
                            if !t.is_empty() {
                                let _ = tx.send(Ok(StreamEvent::Content(t.to_string()))).await;
                            }
                        }
                        if let Some(fc) = p.get("functionCall") {
                            let name = fc.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
                            let id = fc.get("id").and_then(Value::as_str).map(|s| s.to_string()).unwrap_or_else(|| format!("call_{}", &name));
                            let _ = tx.send(Ok(StreamEvent::ToolCallStart { index: 0, id, name })).await;
                            let args = fc.get("args").map(|a| a.to_string()).unwrap_or_default();
                            if !args.is_empty() {
                                let _ = tx.send(Ok(StreamEvent::ToolCallArgs { index: 0, delta: args })).await;
                            }
                        }
                    }
                }
                if let Some(fr) = c.get("finishReason").and_then(Value::as_str) {
                    if fr == "STOP" || fr == "MAX_TOKENS" {
                        // signal will be handled by Done at end
                    }
                }
            }
        }
    }
}

// public helper for tests
pub fn parse_gemini_frame(frame: &str) -> Vec<StreamEvent> {
    let mut out = Vec::new();
    for line in frame.lines() {
        let Some(payload) = line.strip_prefix("data:") else { continue; };
        let payload = payload.trim();
        if payload.is_empty() || payload == "[DONE]" { continue; }
        let Ok(v) = serde_json::from_str::<Value>(payload) else { continue; };
        if let Some(cands) = v.get("candidates").and_then(Value::as_array) {
            for c in cands {
                if let Some(parts) = c.get("content").and_then(|c| c.get("parts")).and_then(Value::as_array) {
                    for p in parts {
                        if let Some(t) = p.get("text").and_then(Value::as_str) {
                            if !t.is_empty() { out.push(StreamEvent::Content(t.to_string())); }
                        }
                        if let Some(fc) = p.get("functionCall") {
                            let name = fc.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
                            let id = fc.get("id").and_then(Value::as_str).map(|s| s.to_string()).unwrap_or_else(|| format!("call_{}", &name));
                            out.push(StreamEvent::ToolCallStart { index: 0, id, name });
                        }
                    }
                }
            }
        }
    }
    out
}
