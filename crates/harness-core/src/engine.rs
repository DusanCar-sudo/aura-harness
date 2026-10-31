//! Client for the Aura Code engine over stdio (`aura sidecar`).
//!
//! The harness has its own agent loop, but Aura Code owns a much larger
//! command surface — memory (`:dream`, `:rem`, `:mine`), workflows,
//! `:machina`, `:council`, `:designx`, lessons, the cost ledger and more.
//! Re-implementing them in Rust would fork the behaviour and drift. Instead
//! the harness starts `aura sidecar` (newline-delimited JSON, see
//! `docs/PROTOCOL.md` in aura-code) and asks it to run the command through
//! `command.run`, which is the same implementation the TUI and web client use.
//!
//! One process, one session, one command at a time. Commands that need a
//! terminal (`:model`, `:approve`, …) come back with `terminal_only: true`
//! and a reason; the GUI handles those natively instead.

use crate::error::{Error, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// One command's answer, as the engine reports it.
#[derive(Debug, Clone, Default)]
pub struct CommandOutput {
    /// False when the engine did not recognise the command at all.
    pub handled: bool,
    /// True for commands that drive a terminal's stdin (model picker, …).
    pub terminal_only: bool,
    /// Why the command needs a terminal, when `terminal_only`.
    pub reason: Option<String>,
    /// The command's stdout, split into lines. May contain ANSI escapes.
    pub lines: Vec<String>,
    /// True when the command replaced the engine-side conversation.
    pub session_replaced: bool,
}

pub struct EngineClient {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    frames: Receiver<Value>,
    session: Option<String>,
    next_id: u64,
    project_root: String,
    model: String,
    api_key: Option<String>,
    base_url: Option<String>,
}

impl EngineClient {
    /// Start `aura sidecar` in `project_root`. The process is lazy about
    /// models: the session is created on first use.
    pub fn spawn(
        bin: &str,
        project_root: &str,
        model: &str,
        api_key: Option<String>,
        base_url: Option<String>,
    ) -> Result<Self> {
        let mut child = Command::new(bin)
            .arg("sidecar")
            .current_dir(project_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Diagnostics go to stderr; stdout is the frame stream. Keep the
            // noise out of a GUI with no terminal to show it in.
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| Error::Invalid(format!("could not start `{bin} sidecar`: {e}")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Invalid("aura sidecar has no stdout".into()))?;
        let stdin = Arc::new(Mutex::new(
            child
                .stdin
                .take()
                .ok_or_else(|| Error::Invalid("aura sidecar has no stdin".into()))?,
        ));

        let (tx, frames) = mpsc::channel();
        {
            let stdin = Arc::clone(&stdin);
            std::thread::Builder::new()
                .name("aura-engine-reader".into())
                .spawn(move || {
                    for line in BufReader::new(stdout).lines() {
                        let Ok(line) = line else { break };
                        let Ok(frame) = serde_json::from_str::<Value>(&line) else {
                            continue;
                        };
                        match frame.get("kind").and_then(|k| k.as_str()) {
                            // The engine asks the client to approve a tool
                            // mid-command. With no human in the loop it must
                            // be a refusal: silence is never consent.
                            Some("req") => {
                                let id = frame.get("id").cloned().unwrap_or(Value::Null);
                                let reply = json!({
                                    "kind": "res",
                                    "id": id,
                                    "ok": true,
                                    "result": { "decision": "deny" }
                                });
                                if let Ok(mut w) = stdin.lock() {
                                    let _ = writeln!(w, "{reply}");
                                    let _ = w.flush();
                                }
                            }
                            Some("res") => {
                                if tx.send(frame).is_err() {
                                    break;
                                }
                            }
                            // engine.ready, turn events, logs: nothing to do.
                            _ => {}
                        }
                    }
                })
                .map_err(|e| Error::Invalid(format!("could not start the engine reader: {e}")))?;
        }

        Ok(Self {
            child,
            stdin,
            frames,
            session: None,
            next_id: 0,
            project_root: project_root.to_string(),
            model: model.to_string(),
            api_key,
            base_url,
        })
    }

    /// The engine-side session, created on first use.
    pub fn session(&mut self) -> Result<String> {
        if let Some(id) = &self.session {
            return Ok(id.clone());
        }
        let mut params = json!({
            "projectRoot": self.project_root,
            "name": "Aura Harness",
        });
        if !self.model.is_empty() {
            params["model"] = json!(self.model);
        }
        if let Some(key) = &self.api_key {
            params["apiKey"] = json!(key);
        }
        if let Some(base) = &self.base_url {
            params["baseUrl"] = json!(base);
        }

        let mut result = self.request("session.create", params.clone(), Duration::from_secs(60));

        // A model id the harness knows but the engine's registry does not must
        // not make every command fail: retry with the engine's saved default.
        if result.is_err() && params.get("model").is_some() {
            params.as_object_mut().map(|o| o.remove("model"));
            result = self.request("session.create", params, Duration::from_secs(60));
        }

        let result = result?;
        let id = result
            .get("sessionId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Invalid("session.create returned no sessionId".into()))?
            .to_string();
        self.session = Some(id.clone());
        Ok(id)
    }

    /// Run one `:command` on the engine and collect its answer.
    pub fn run_command(&mut self, command: &str) -> Result<CommandOutput> {
        let session_id = self.session()?;
        let result = self.request(
            "command.run",
            json!({ "sessionId": session_id, "command": command }),
            Duration::from_secs(900),
        )?;

        let lines = result
            .get("output")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        Ok(CommandOutput {
            handled: result
                .get("handled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            terminal_only: result
                .get("terminalOnly")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            reason: result
                .get("reason")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            lines,
            session_replaced: result
                .get("sessionReplaced")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        })
    }

    fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id.to_string();
        let frame = json!({
            "kind": "req",
            "id": id,
            "method": method,
            "params": params,
        });

        {
            let mut writer = self
                .stdin
                .lock()
                .map_err(|_| Error::Invalid("the engine's stdin is poisoned".into()))?;
            writeln!(writer, "{frame}")?;
            writer.flush()?;
        }

        let started = Instant::now();
        loop {
            let left = timeout.checked_sub(started.elapsed()).unwrap_or_default();
            if left.is_zero() {
                return Err(Error::Invalid(format!("{method} timed out")));
            }
            match self.frames.recv_timeout(left) {
                Ok(frame) => {
                    if frame.get("id").and_then(|v| v.as_str()) != Some(id.as_str()) {
                        continue; // a late answer to an earlier request
                    }
                    if frame.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                        return Ok(frame.get("result").cloned().unwrap_or(Value::Null));
                    }
                    let message = frame
                        .pointer("/error/message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("the engine refused the request");
                    return Err(Error::Invalid(message.to_string()));
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(Error::Invalid(format!("{method} timed out")));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(Error::Invalid(
                        "the Aura engine exited before answering".into(),
                    ));
                }
            }
        }
    }

    /// True while the child process is still running.
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn project_root(&self) -> &str {
        &self.project_root
    }
}

impl Drop for EngineClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_binary_is_an_error_not_a_panic() {
        let err = match EngineClient::spawn(
            "/nonexistent/definitely-not-aura",
            ".",
            "",
            None,
            None,
        ) {
            Ok(_) => panic!("a nonexistent binary must not spawn"),
            Err(e) => e.to_string(),
        };
        assert!(err.contains("could not start"), "{err}");
    }
}
