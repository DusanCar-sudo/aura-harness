//! Google Gemini generateContent stream provider.
//! Endpoint: POST https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent?key={api_key}&alt=sse
//! Body: { contents, systemInstruction?, tools?, generationConfig }.

use crate::api::{ChatOptions, Message, StreamEvent, ToolDef, Usage};
use crate::error::{Error, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tokio::sync::mpsc;

pub fn stream_chat(
    http: reqwest::Client,
    api_key: &str,
    base_url: Option<&str>,
    messages: Vec<Message>,
    opts: ChatOptions,
) -> mpsc::Receiver<Result<StreamEvent>> {
    let (tx, rx) = mpsc::channel::<Result<StreamEvent>>(256);
    let raw_model = opts
        .model_name
        .clone()
        .unwrap_or_else(|| "gemini-2.5-flash".to_string());
    let model_name = crate::provider::strip_routing_prefix(&raw_model).to_string();
    let base = base_url
        .unwrap_or("https://generativelanguage.googleapis.com")
        .trim_end_matches('/')
        .trim_end_matches("/v1beta")
        .to_string();
    let api_key = api_key.to_string();
    let body = build_google_body(&messages, &opts);

    tokio::spawn(async move {
        let url = format!(
            "{base}/v1beta/models/{model_name}:streamGenerateContent?key={api_key}&alt=sse"
        );
        let resp = tokio::time::timeout(
            crate::api::STREAM_IDLE_TIMEOUT,
            http.post(&url).json(&body).send(),
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
        let mut state = StreamState::default();
        loop {
            let next =
                tokio::time::timeout(crate::api::STREAM_IDLE_TIMEOUT, stream.next()).await;
            match next {
                Ok(Some(Ok(b))) => {
                    let chunk = String::from_utf8_lossy(&b).replace("\r\n", "\n");
                    buf.push_str(&chunk);
                    while let Some(pos) = buf.find("\n\n") {
                        let frame: String = buf.drain(..pos + 2).collect();
                        if let Some(err) = crate::provider::extract_frame_error(&frame) {
                            let _ = tx.send(Err(err)).await;
                            return;
                        }
                        for ev in state.parse_frame(&frame) {
                            if tx.send(Ok(ev)).await.is_err() {
                                return;
                            }
                        }
                    }
                }
                Ok(Some(Err(e))) => {
                    let _ = tx.send(Err(Error::Http(e))).await;
                    return;
                }
                Ok(None) => {
                    if !buf.trim().is_empty() {
                        if let Some(err) = crate::provider::extract_frame_error(&buf) {
                            let _ = tx.send(Err(err)).await;
                            return;
                        }
                        for ev in state.parse_frame(&buf) {
                            if tx.send(Ok(ev)).await.is_err() {
                                return;
                            }
                        }
                    }
                    break;
                }
                Err(_) => {
                    let _ = tx
                        .send(Err(Error::Api {
                            status: 0,
                            message: "stream idle for 90s — server stalled".into(),
                        }))
                        .await;
                    return;
                }
            }
        }
        let finish = if state.saw_tool_call {
            "tool_calls"
        } else {
            state.finish_reason.as_deref().unwrap_or("stop")
        };
        let _ = tx
            .send(Ok(StreamEvent::Done {
                finish_reason: Some(finish.to_string()),
                usage: state.usage,
            }))
            .await;
    });
    rx
}

/// Build the complete Gemini `v1beta` generateContent JSON payload.
pub fn build_google_body(messages: &[Message], opts: &ChatOptions) -> Value {
    let max_tokens = opts.max_tokens.unwrap_or(8192);
    let mut call_names: BTreeMap<String, String> = BTreeMap::new();
    let mut turns: Vec<(&'static str, Vec<Value>)> = Vec::new();
    let mut system_text = String::new();

    let push_turn = |turns: &mut Vec<(&'static str, Vec<Value>)>,
                     role: &'static str,
                     mut new_parts: Vec<Value>| {
        if new_parts.is_empty() {
            return;
        }
        if let Some((last_role, last_parts)) = turns.last_mut() {
            if *last_role == role {
                last_parts.append(&mut new_parts);
                return;
            }
        }
        turns.push((role, new_parts));
    };

    for m in messages {
        match m.role.as_str() {
            "system" => {
                if !m.content.is_empty() {
                    if !system_text.is_empty() {
                        system_text.push_str("\n\n");
                    }
                    system_text.push_str(&m.content);
                }
            }
            "assistant" => {
                let mut parts = Vec::new();
                if !m.content.is_empty() {
                    parts.push(json!({"text": m.content}));
                }
                for tc in &m.tool_calls {
                    if !tc.id.is_empty() && !tc.function.name.is_empty() {
                        call_names.insert(tc.id.clone(), tc.function.name.clone());
                    }
                    let args = crate::provider::parse_args_object(&tc.function.arguments);
                    let sig = tc
                        .thought_signature
                        .as_deref()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or("skip_thought_signature_validator");
                    parts.push(json!({
                        "functionCall": {
                            "name": tc.function.name,
                            "args": args,
                        },
                        "thoughtSignature": sig,
                    }));
                }
                push_turn(&mut turns, "model", parts);
            }
            "tool" => {
                let fn_name = m
                    .tool_call_id
                    .as_deref()
                    .and_then(|id| call_names.get(id))
                    .map(String::as_str)
                    .unwrap_or("tool");
                let part = json!({
                    "functionResponse": {
                        "name": fn_name,
                        "response": { "result": m.content },
                    }
                });
                push_turn(&mut turns, "user", vec![part]);
            }
            _ => {
                let mut parts = Vec::with_capacity(m.images.len() + 1);
                if !m.content.is_empty() {
                    parts.push(json!({"text": m.content}));
                }
                for img in &m.images {
                    parts.push(json!({
                        "inlineData": { "mimeType": img.mime, "data": img.data }
                    }));
                }
                push_turn(&mut turns, "user", parts);
            }
        }
    }

    let contents: Vec<Value> = turns
        .into_iter()
        .map(|(role, parts)| json!({ "role": role, "parts": parts }))
        .collect();

    let mut body = json!({
        "contents": contents,
        "generationConfig": { "maxOutputTokens": max_tokens },
    });
    if !system_text.is_empty() {
        body["systemInstruction"] = json!({"parts": [{"text": system_text}]});
    }
    if !opts.tools.is_empty() {
        let decls: Vec<Value> = opts.tools.iter().map(to_google_tool).collect();
        body["tools"] = json!([{ "functionDeclarations": decls }]);
    }
    body
}

fn to_google_tool(t: &ToolDef) -> Value {
    json!({
        "name": t.name,
        "description": t.description,
        "parameters": sanitize_google_schema(&t.parameters),
    })
}

/// Clean JSON Schema down to the subset Gemini `v1beta` `Schema` accepts
/// (drops `$schema`, `additionalProperties`, `default`, numeric constraints, and
/// empty `properties: {}` objects that trigger HTTP 400).
fn sanitize_google_schema(v: &Value) -> Value {
    let Some(obj) = v.as_object() else {
        return json!({ "type": "object" });
    };
    const ALLOWED: &[&str] = &[
        "type",
        "format",
        "description",
        "nullable",
        "enum",
        "items",
        "properties",
        "required",
        "minItems",
        "maxItems",
    ];
    let mut out = serde_json::Map::new();
    for (k, val) in obj {
        if !ALLOWED.contains(&k.as_str()) {
            continue;
        }
        if k == "properties" {
            if let Some(props) = val.as_object() {
                if !props.is_empty() {
                    let cleaned: serde_json::Map<String, Value> = props
                        .iter()
                        .map(|(pk, pv)| (pk.clone(), sanitize_google_schema(pv)))
                        .collect();
                    out.insert(k.clone(), Value::Object(cleaned));
                }
            }
        } else if k == "items" {
            out.insert(k.clone(), sanitize_google_schema(val));
        } else {
            out.insert(k.clone(), val.clone());
        }
    }
    out.entry("type")
        .or_insert_with(|| Value::String("object".into()));
    Value::Object(out)
}

#[derive(Default)]
struct StreamState {
    next_call_idx: usize,
    saw_tool_call: bool,
    finish_reason: Option<String>,
    usage: Option<Usage>,
}

impl StreamState {
    fn parse_frame(&mut self, frame: &str) -> Vec<StreamEvent> {
        let mut out = Vec::new();
        for line in frame.lines() {
            let Some(payload) = line.strip_prefix("data:") else {
                continue;
            };
            let payload = payload.trim();
            if payload.is_empty() || payload == "[DONE]" {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(payload) else {
                continue;
            };
            if let Some(meta) = v.get("usageMetadata") {
                let prompt = meta
                    .get("promptTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let cand = meta
                    .get("candidatesTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let total = meta
                    .get("totalTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(prompt + cand);
                self.usage = Some(Usage {
                    prompt_tokens: prompt,
                    completion_tokens: cand,
                    total_tokens: total,
                });
            }
            if let Some(cands) = v.get("candidates").and_then(Value::as_array) {
                for c in cands {
                    if let Some(parts) = c
                        .get("content")
                        .and_then(|c| c.get("parts"))
                        .and_then(Value::as_array)
                    {
                        for p in parts {
                            let is_thought =
                                p.get("thought").and_then(Value::as_bool).unwrap_or(false);
                            if let Some(t) = p.get("text").and_then(Value::as_str) {
                                if !t.is_empty() {
                                    if is_thought {
                                        out.push(StreamEvent::Reasoning(t.to_string()));
                                    } else {
                                        out.push(StreamEvent::Content(t.to_string()));
                                    }
                                }
                            }
                            if let Some(fc) = p.get("functionCall") {
                                let name = fc
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string();
                                let idx = self.next_call_idx;
                                self.next_call_idx += 1;
                                self.saw_tool_call = true;
                                let id = fc
                                    .get("id")
                                    .and_then(Value::as_str)
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| format!("call_{name}_{idx}"));
                                out.push(StreamEvent::ToolCallStart {
                                    index: idx,
                                    id,
                                    name,
                                });
                                if let Some(sig) = p.get("thoughtSignature").and_then(Value::as_str)
                                {
                                    if !sig.is_empty() {
                                        out.push(StreamEvent::ToolCallSignature {
                                            index: idx,
                                            signature: sig.to_string(),
                                        });
                                    }
                                }
                                let args = fc
                                    .get("args")
                                    .filter(|a| a.is_object())
                                    .map(|a| a.to_string())
                                    .unwrap_or_else(|| "{}".to_string());
                                out.push(StreamEvent::ToolCallArgs {
                                    index: idx,
                                    delta: args,
                                });
                            }
                        }
                    }
                    if let Some(fr) = c.get("finishReason").and_then(Value::as_str) {
                        self.finish_reason = Some(match fr {
                            "MAX_TOKENS" => "length".to_string(),
                            _ => "stop".to_string(),
                        });
                    }
                }
            }
        }
        out
    }
}

/// Parse one SSE frame statelessly (unit-test helper).
pub fn parse_gemini_frame(frame: &str) -> Vec<StreamEvent> {
    StreamState::default().parse_frame(frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ToolCall;

    #[test]
    fn google_body_serializes_function_call_args_as_struct_never_string() {
        let history = vec![
            Message::system("Sys instruction"),
            Message::user("Read note.txt"),
            Message {
                role: "assistant".into(),
                content: String::new(),
                images: Vec::new(),
                reasoning_content: String::new(),
                tool_calls: vec![
                    ToolCall::new("c1", 0, "read_file", r#"{"path":"note.txt","limit":50}"#),
                    ToolCall::new("c2", 1, "list_files", "{bad json"),
                ],
                tool_call_id: None,
                ts: None,
            },
            Message::tool_result("c1", "hello"),
            Message::tool_result("c2", "a.rs"),
        ];
        let body = build_google_body(
            &history,
            &ChatOptions {
                tools: vec![ToolDef {
                    name: "read_file".into(),
                    description: "read".into(),
                    parameters: json!({
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "path": {"type": "string"},
                            "limit": {"type": "integer", "minimum": 1}
                        },
                        "required": ["path"]
                    }),
                }],
                ..Default::default()
            },
        );

        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 3);
        assert_eq!(contents[1]["role"], "model");
        let fc1 = &contents[1]["parts"][0]["functionCall"];
        assert!(fc1["args"].is_object(), "args must be a JSON object: {fc1}");
        assert_eq!(fc1["args"]["path"], "note.txt");
        assert_eq!(
            contents[1]["parts"][0]["thoughtSignature"],
            "skip_thought_signature_validator"
        );
        // Invalid/truncated JSON args safely degrade to `{}` instead of string.
        assert!(contents[1]["parts"][1]["functionCall"]["args"].is_object());
        // Consecutive tool results merge cleanly into one user turn with functionResponse.
        assert_eq!(contents[2]["role"], "user");
        assert_eq!(contents[2]["parts"][0]["functionResponse"]["name"], "read_file");
        assert_eq!(contents[2]["parts"][1]["functionResponse"]["name"], "list_files");
        // Schema sanitization strips unsupported keywords.
        let decl = &body["tools"][0]["functionDeclarations"][0];
        assert!(decl["parameters"].get("additionalProperties").is_none());
        assert!(decl["parameters"]["properties"]["limit"].get("minimum").is_none());
    }

    #[test]
    fn parse_frame_extracts_reasoning_parallel_calls_and_thought_signature() {
        let frame = r#"data: {"candidates":[{"content":{"parts":[{"thought":true,"text":"plan..."},{"functionCall":{"name":"read_file","args":{"path":"a.rs"}},"thoughtSignature":"sig-xyz"},{"functionCall":{"name":"read_file","args":{"path":"b.rs"}}}]}}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5,"totalTokenCount":15}}"#;
        let mut st = StreamState::default();
        let events = st.parse_frame(frame);
        assert!(st.saw_tool_call);
        assert!(matches!(&events[0], StreamEvent::Reasoning(s) if s == "plan..."));
        assert!(matches!(&events[1], StreamEvent::ToolCallStart { index: 0, name, .. } if name == "read_file"));
        assert!(matches!(&events[2], StreamEvent::ToolCallSignature { index: 0, signature } if signature == "sig-xyz"));
        assert!(matches!(&events[4], StreamEvent::ToolCallStart { index: 1, id, .. } if id == "call_read_file_1"));
    }
}
