//! MCP client over stdio.
//!
//! Each configured server is spawned as a child process speaking JSON-RPC 2.0
//! on stdin/stdout. We hand-roll the small subset we need (`initialize`,
//! `tools/list`, `tools/call`) rather than pulling the full `rmcp` SDK: it
//! keeps the dependency surface tiny, the framing is a dozen lines, and every
//! step stays unit-testable against recorded line fixtures.

use crate::api::ToolDef;
use crate::config::{validate_mcp_command, McpServer};
use crate::error::{Error, Result};
use base64::Engine;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

const MCP_CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
const MCP_CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// Tool names are namespaced so a chat can mix several servers safely.
pub fn name_prefix(server: &str) -> String {
    format!("mcp_{server}_")
}

pub fn namespaced(server: &str, tool: &str) -> String {
    format!("{}{}", name_prefix(server), tool)
}

/// Split a namespaced tool name back into (server, tool).
pub fn split_namespaced(name: &str) -> Option<(String, String)> {
    let rest = name.strip_prefix("mcp_")?;
    let (server, tool) = rest.split_once('_')?;
    if server.is_empty() || tool.is_empty() {
        return None;
    }
    Some((server.to_string(), tool.to_string()))
}

/// A live connection to one MCP server.
pub struct McpConnection {
    server: String,
    child: Child,
    reader: BufReader<tokio::process::ChildStdout>,
    stderr_tail: Arc<Mutex<String>>,
    next_id: u64,
}

/// Result of one `tools/call`: the text blocks the server returned.
#[derive(Debug, Clone, Default)]
pub struct CallResult {
    pub text: String,
    pub is_error: bool,
}

impl McpConnection {
    /// Spawn the server and complete the MCP handshake with timeout & stderr capture.
    ///
    /// The child gets a minimal environment plus whatever `cfg.env` declares —
    /// never this app's own secrets (the API key lives in our process only).
    pub async fn connect(server_name: &str, cfg: &McpServer) -> Result<Self> {
        validate_mcp_command(&cfg.command, &cfg.args).map_err(Error::Mcp)?;

        let mut cmd = Command::new(&cfg.command);
        cmd.args(&cfg.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .env_clear()
            .envs(crate::child_env().iter().map(|(k, v)| (k.clone(), v.clone())));
        for (k, v) in &cfg.env {
            cmd.env(k, v);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| Error::Mcp(format!("spawn {}: {e}", cfg.command)))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Mcp("no stdout on child".into()))?;
        let stderr_tail = Arc::new(Mutex::new(String::new()));
        if let Some(stderr) = child.stderr.take() {
            let tail = Arc::clone(&stderr_tail);
            tokio::spawn(async move {
                let mut r = BufReader::new(stderr);
                let mut line = String::new();
                while let Ok(n) = r.read_line(&mut line).await {
                    if n == 0 {
                        break;
                    }
                    let mut guard = tail.lock().await;
                    guard.push_str(&line);
                    if guard.len() > 2000 {
                        let cut = guard.len() - 1500;
                        let safe = guard
                            .char_indices()
                            .find(|(idx, _)| *idx >= cut)
                            .map(|(idx, _)| idx)
                            .unwrap_or(0);
                        guard.drain(..safe);
                    }
                    line.clear();
                }
            });
        }
        let mut conn = Self {
            server: server_name.to_string(),
            child,
            reader: BufReader::new(stdout),
            stderr_tail: Arc::clone(&stderr_tail),
            next_id: 1,
        };
        match tokio::time::timeout(MCP_CONNECT_TIMEOUT, conn.initialize()).await {
            Ok(Ok(())) => Ok(conn),
            Ok(Err(e)) => {
                tokio::time::sleep(Duration::from_millis(40)).await;
                let stderr = stderr_tail.lock().await.trim().to_string();
                if stderr.is_empty() {
                    Err(e)
                } else {
                    let tail = stderr.lines().last().unwrap_or(&stderr);
                    Err(Error::Mcp(format!("{e} ({tail})")))
                }
            }
            Err(_) => {
                let stderr = stderr_tail.lock().await.trim().to_string();
                if stderr.is_empty() {
                    Err(Error::Mcp(format!(
                        "handshake timed out after {}s",
                        MCP_CONNECT_TIMEOUT.as_secs()
                    )))
                } else {
                    let tail = stderr.lines().last().unwrap_or(&stderr);
                    Err(Error::Mcp(format!(
                        "handshake timed out after {}s ({tail})",
                        MCP_CONNECT_TIMEOUT.as_secs()
                    )))
                }
            }
        }
    }

    pub fn server_name(&self) -> &str {
        &self.server
    }

    async fn write_message(&mut self, msg: &Value) -> Result<()> {
        let stdin = self
            .child
            .stdin
            .as_mut()
            .ok_or_else(|| Error::Mcp("no stdin on child".into()))?;
        let mut line = serde_json::to_string(msg)?;
        line.push('\n');
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| Error::Mcp(format!("write: {e}")))?;
        stdin.flush().await.map_err(|e| Error::Mcp(format!("flush: {e}")))?;
        Ok(())
    }

    /// Read lines until we get the JSON-RPC response with the given id.
    async fn read_response(&mut self, id: u64) -> Result<Value> {
        loop {
            let mut line = String::new();
            let n = self
                .reader
                .read_line(&mut line)
                .await
                .map_err(|e| Error::Mcp(format!("read: {e}")))?;
            if n == 0 {
                return Err(Error::Mcp("server closed stdout".into()));
            }
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue; // servers sometimes log junk to stdout
            };
            if v.get("id").and_then(Value::as_u64) != Some(id) {
                continue; // notification or unrelated response
            }
            if let Some(err) = v.get("error") {
                return Err(Error::Mcp(err.to_string()));
            }
            return Ok(v.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.write_message(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .await?;
        self.read_response(id).await
    }

    async fn notify(&mut self, method: &str) -> Result<()> {
        self.write_message(&json!({"jsonrpc": "2.0", "method": method})).await
    }

    async fn initialize(&mut self) -> Result<()> {
        self.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "aura-harness", "version": env!("CARGO_PKG_VERSION")},
            }),
        )
        .await?;
        self.notify("notifications/initialized").await
    }

    /// Fetch this server's tools as namespaced `ToolDef`s.
    pub async fn list_tools(&mut self) -> Result<Vec<ToolDef>> {
        let result = self.request("tools/list", json!({})).await?;
        Ok(parse_tool_list(&self.server, &result))
    }

    /// Fetch this server's tools as `(original_name, namespaced_def)` pairs.
    ///
    /// Callers that need to *invoke* a tool want the original name, which the
    /// namespaced `ToolDef` alone cannot give back reliably (server names may
    /// themselves contain `_`).
    pub async fn list_tools_raw(&mut self) -> Result<Vec<(String, ToolDef)>> {
        let result = self.request("tools/list", json!({})).await?;
        Ok(parse_tool_list_raw(&self.server, &result))
    }

    /// Invoke one tool by its *original* name on this server, with timeout.
    pub async fn call(&mut self, tool: &str, arguments: &str) -> Result<CallResult> {
        let parsed: Value = serde_json::from_str(arguments).unwrap_or(json!({}));
        let fut = self.request("tools/call", json!({"name": tool, "arguments": parsed}));
        let result = match tokio::time::timeout(MCP_CALL_TIMEOUT, fut).await {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                let stderr = self.stderr_tail.lock().await.trim().to_string();
                if stderr.is_empty() {
                    return Err(e);
                }
                let tail = stderr.lines().last().unwrap_or(&stderr);
                return Err(Error::Mcp(format!("{e} ({tail})")));
            }
            Err(_) => {
                return Err(Error::Mcp(format!(
                    "{}/{} timed out after {}s",
                    self.server,
                    tool,
                    MCP_CALL_TIMEOUT.as_secs()
                )));
            }
        };
        Ok(parse_call_result(&result))
    }
}

/// Turn a `tools/list` result into namespaced tool definitions.
pub fn parse_tool_list(server: &str, result: &Value) -> Vec<ToolDef> {
    parse_tool_list_raw(server, result)
        .into_iter()
        .map(|(_, def)| def)
        .collect()
}

/// Like [`parse_tool_list`], but keeps each tool's original (un-namespaced) name.
pub fn parse_tool_list_raw(server: &str, result: &Value) -> Vec<(String, ToolDef)> {
    result
        .get("tools")
        .and_then(Value::as_array)
        .map(|tools| {
            tools
                .iter()
                .filter_map(|t| {
                    let name = t.get("name").and_then(Value::as_str)?;
                    Some((
                        name.to_string(),
                        ToolDef {
                            name: namespaced(server, name),
                            description: t
                                .get("description")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            // Servers are not obliged to emit a complete JSON
                            // Schema; normalize so the API accepts it.
                            parameters: crate::api::normalize_parameters(
                                t.get("inputSchema").cloned().unwrap_or(Value::Null),
                            ),
                        },
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Flatten a `tools/call` result's content blocks into plain text, saving
/// decoded base64 `image` blocks into temporary files so the UI can show them.
pub fn parse_call_result(result: &Value) -> CallResult {
    let is_error = result.get("isError").and_then(Value::as_bool).unwrap_or(false);
    let mut parts = Vec::new();
    if let Some(blocks) = result.get("content").and_then(Value::as_array) {
        for (idx, block) in blocks.iter().enumerate() {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(t) = block.get("text").and_then(Value::as_str) {
                        parts.push(t.to_string());
                    }
                }
                Some("image") => {
                    let mime = block
                        .get("mimeType")
                        .and_then(Value::as_str)
                        .unwrap_or("image/png");
                    let saved = block
                        .get("data")
                        .and_then(Value::as_str)
                        .filter(|b64| b64.len() > 64)
                        .and_then(|b64| {
                            let clean = b64.split(',').next_back().unwrap_or(b64);
                            base64::engine::general_purpose::STANDARD.decode(clean.trim()).ok()
                        })
                        .and_then(|bytes| save_mcp_image(&bytes, mime, idx).ok().map(|p| (p, bytes.len())));
                    match saved {
                        Some((path, len)) => parts.push(format!(
                            "[image saved: {} ({mime}, {len} bytes)]",
                            path.display()
                        )),
                        None => parts.push(format!("[image content ({mime})]")),
                    }
                }
                Some(other) => parts.push(format!("[{other} content]")),
                None => {}
            }
        }
    }
    if parts.is_empty() {
        // Some servers return structured content instead of text blocks.
        if let Some(sc) = result.get("structuredContent") {
            parts.push(sc.to_string());
        }
    }
    CallResult {
        text: parts.join("\n"),
        is_error,
    }
}

fn save_mcp_image(bytes: &[u8], mime: &str, idx: usize) -> std::io::Result<std::path::PathBuf> {
    let ext = if mime.contains("jpeg") || mime.contains("jpg") {
        "jpg"
    } else if mime.contains("webp") {
        "webp"
    } else {
        "png"
    };
    let dir = std::env::temp_dir().join("aura-harness-mcp-images");
    std::fs::create_dir_all(&dir)?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = dir.join(format!("shot-{ts}-{idx}.{ext}"));
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// Event emitted for one MCP server as soon as its concurrent boot settles.
pub struct McpServerBoot {
    pub name: String,
    pub conn: Option<McpConnection>,
    pub tools: Vec<crate::tools::RegisteredTool>,
    pub error: Option<String>,
}

/// Boot all enabled servers concurrently and stream each result as soon as it
/// completes (fast local servers like `mnemai` and `aura_chrome` land first).
pub async fn connect_streaming(
    servers: BTreeMap<String, McpServer>,
    tx: tokio::sync::mpsc::UnboundedSender<McpServerBoot>,
) {
    let mut set = tokio::task::JoinSet::new();
    for (name, cfg) in servers {
        if !cfg.enabled {
            continue;
        }
        set.spawn(async move {
            match McpConnection::connect(&name, &cfg).await {
                Ok(mut conn) => match conn.list_tools_raw().await {
                    Ok(raw) => {
                        let tools = raw
                            .into_iter()
                            .map(|(tool, def)| crate::tools::RegisteredTool {
                                def,
                                executor: crate::tools::ToolExecutor::Mcp {
                                    server: name.clone(),
                                    tool,
                                },
                            })
                            .collect();
                        McpServerBoot {
                            name,
                            conn: Some(conn),
                            tools,
                            error: None,
                        }
                    }
                    Err(e) => McpServerBoot {
                        name,
                        conn: Some(conn),
                        tools: Vec::new(),
                        error: Some(format!("tools/list: {e}")),
                    },
                },
                Err(e) => McpServerBoot {
                    name,
                    conn: None,
                    tools: Vec::new(),
                    error: Some(e.to_string()),
                },
            }
        });
    }
    while let Some(res) = set.join_next().await {
        if let Ok(boot) = res {
            let _ = tx.send(boot);
        }
    }
}

/// Spawn every enabled server concurrently. Servers that fail to start are
/// reported, not fatal — one broken plugin must not take the chat down.
pub async fn connect_all(
    servers: &BTreeMap<String, McpServer>,
) -> (BTreeMap<String, McpConnection>, Vec<String>) {
    let mut set = tokio::task::JoinSet::new();
    for (name, cfg) in servers {
        if !cfg.enabled {
            continue;
        }
        let (name, cfg) = (name.clone(), cfg.clone());
        set.spawn(async move { (name.clone(), McpConnection::connect(&name, &cfg).await) });
    }
    let mut conns = BTreeMap::new();
    let mut failures = Vec::new();
    while let Some(res) = set.join_next().await {
        match res {
            Ok((name, Ok(c))) => {
                conns.insert(name, c);
            }
            Ok((name, Err(e))) => failures.push(format!("{name}: {e}")),
            Err(e) => failures.push(format!("mcp task: {e}")),
        }
    }
    (conns, failures)
}

/// Collect tool definitions from already-connected servers.
pub async fn collect_all_tools(conns: &mut BTreeMap<String, McpConnection>) -> Vec<ToolDef> {
    let mut out = Vec::new();
    for conn in conns.values_mut() {
        if let Ok(defs) = conn.list_tools().await {
            out.extend(defs);
        }
    }
    out
}

/// Collect registry entries for every tool on every connected server.
///
/// The server name comes from the map key, so it survives server names that
/// contain underscores.
pub async fn collect_registered(
    conns: &mut BTreeMap<String, McpConnection>,
) -> Vec<crate::tools::RegisteredTool> {
    let mut out = Vec::new();
    for (server, conn) in conns.iter_mut() {
        if let Ok(tools) = conn.list_tools_raw().await {
            for (tool, def) in tools {
                out.push(crate::tools::RegisteredTool {
                    def,
                    executor: crate::tools::ToolExecutor::Mcp {
                        server: server.clone(),
                        tool,
                    },
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespacing_round_trips() {
        assert_eq!(namespaced("mnemai", "memory_search"), "mcp_mnemai_memory_search");
        assert_eq!(
            split_namespaced("mcp_mnemai_memory_search"),
            Some(("mnemai".into(), "memory_search".into()))
        );
        assert_eq!(split_namespaced("cmd_echo"), None);
        assert_eq!(split_namespaced("mcp_noseparator"), None);
    }

    #[test]
    fn parses_tool_list_with_schema() {
        let result = json!({
            "tools": [{
                "name": "memory_search",
                "description": "search the shared memory",
                "inputSchema": {"type": "object", "properties": {"q": {"type": "string"}}}
            }]
        });
        let defs = parse_tool_list("mnemai", &result);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "mcp_mnemai_memory_search");
        assert_eq!(defs[0].parameters["properties"]["q"]["type"], "string");
    }

    #[test]
    fn tool_list_defaults_missing_schema() {
        let result = json!({"tools": [{"name": "ping"}]});
        let defs = parse_tool_list("s", &result);
        assert_eq!(defs[0].parameters["type"], "object");
        assert_eq!(defs[0].description, "");
    }

    #[test]
    fn tool_list_tolerates_garbage() {
        assert!(parse_tool_list("s", &json!({})).is_empty());
        assert!(parse_tool_list("s", &json!({"tools": "nope"})).is_empty());
    }

    #[test]
    fn flattens_text_content_blocks() {
        let result = json!({
            "content": [
                {"type": "text", "text": "first"},
                {"type": "text", "text": "second"}
            ]
        });
        let r = parse_call_result(&result);
        assert_eq!(r.text, "first\nsecond");
        assert!(!r.is_error);
    }

    #[test]
    fn marks_error_results() {
        let result = json!({"content": [{"type": "text", "text": "boom"}], "isError": true});
        assert!(parse_call_result(&result).is_error);
    }

    #[test]
    fn falls_back_to_structured_content() {
        let result = json!({"structuredContent": {"hits": 3}});
        assert!(parse_call_result(&result).text.contains("hits"));
    }

    #[test]
    fn notes_non_text_blocks() {
        let result = json!({"content": [{"type": "image", "data": "..."}]});
        assert!(parse_call_result(&result).text.contains("image"));
    }

    #[test]
    fn raw_tool_list_preserves_original_names() {
        let result = json!({"tools": [{"name": "memory_search", "description": "d"}]});
        let raw = parse_tool_list_raw("my_server", &result);
        assert_eq!(raw.len(), 1);
        assert_eq!(raw[0].0, "memory_search");
        assert_eq!(raw[0].1.name, "mcp_my_server_memory_search");
    }
}
