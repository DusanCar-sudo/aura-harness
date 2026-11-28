//! Universal multi-provider layer, ported from aura-code (TypeScript → Rust).
//!
//! Each provider has a descriptor: family, routing prefixes, API key env var,
//! transport class, and base URL. Model names route to the matching provider
//! through that one table.

pub mod anthropic;
pub mod descriptors;
pub mod google;

pub use anthropic::stream_chat as stream_anthropic;
pub use google::stream_chat as stream_google;

pub use descriptors::{
    api_env_for, base_url_for, custom_providers, detect_provider_kind, get_api_key_for_model,
    provider_family, set_custom_providers, strip_routing_prefix, ProviderDescriptor,
    TransportClass, KNOWN_PROVIDERS,
};

/// Coerce serialized tool arguments into a JSON object (`Struct` / dict).
///
/// Gemini (`functionCall.args`) and Anthropic (`tool_use.input`) both require a
/// JSON object rather than a string; empty or truncated arguments safely fall
/// back to `{}`.
pub(crate) fn parse_args_object(raw: &str) -> serde_json::Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return serde_json::json!({});
    }
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(serde_json::Value::Object(map)) => serde_json::Value::Object(map),
        _ => serde_json::json!({}),
    }
}

/// Extract an error `(status, message)` from a streamed SSE JSON object when a
/// server signals an error after returning HTTP 200 (e.g. LM Studio `event: error`).
pub(crate) fn extract_sse_error(v: &serde_json::Value) -> Option<(u16, String)> {
    if let Some(err) = v.get("error") {
        let msg = err
            .get("message")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| err.to_string());
        let code = err
            .get("code")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u16;
        return Some((code, msg));
    }
    if v.get("type").and_then(serde_json::Value::as_str) == Some("error") {
        let msg = v
            .get("message")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| v.to_string());
        return Some((0, msg));
    }
    None
}

/// Check every `data:` JSON payload in an SSE frame for a streamed server error.
pub(crate) fn extract_frame_error(frame: &str) -> Option<crate::error::Error> {
    for line in frame.lines() {
        let Some(payload) = line.strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) {
            if let Some((status, message)) = extract_sse_error(&v) {
                return Some(crate::error::Error::Api { status, message });
            }
        }
    }
    None
}
