//! Worker thread in front of [`harness_core::engine::EngineClient`].
//!
//! `aura sidecar` is a child process: starting it and waiting for a command
//! must never happen on the UI thread. This module owns the thread, sends
//! jobs to it, and hands finished answers back to the frame that asks.

use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};

use harness_core::engine::{CommandOutput, EngineClient};

pub struct EngineJob {
    pub command: String,
}

#[derive(Debug)]
pub enum EngineResult {
    Done(CommandOutput),
    Failed(String),
}

pub struct EngineBridge {
    jobs: Sender<EngineJob>,
    results: Receiver<EngineResult>,
    /// Kept for the "engine: …" status line.
    bin: String,
}

impl EngineBridge {
    pub fn start(
        project_root: String,
        model: String,
        api_key: Option<String>,
        base_url: Option<String>,
    ) -> Self {
        let bin = find_aura_binary().unwrap_or_else(|| "aura".to_string());
        let (job_tx, job_rx) = mpsc::channel::<EngineJob>();
        let (result_tx, result_rx) = mpsc::channel::<EngineResult>();

        {
            let bin = bin.clone();
            std::thread::Builder::new()
                .name("aura-engine".into())
                .spawn(move || {
                    let mut client: Option<EngineClient> = None;
                    while let Ok(job) = job_rx.recv() {
                        if client.as_mut().is_some_and(|c| !c.is_alive()) {
                            client = None; // crashed or exited; start fresh
                        }
                        if client.is_none() {
                            match EngineClient::spawn(
                                &bin,
                                &project_root,
                                &model,
                                api_key.clone(),
                                base_url.clone(),
                            ) {
                                Ok(c) => client = Some(c),
                                Err(e) => {
                                    let _ = result_tx.send(EngineResult::Failed(e.to_string()));
                                    continue;
                                }
                            }
                        }
                        let answer = client
                            .as_mut()
                            .expect("client was just created")
                            .run_command(&job.command);
                        match answer {
                            Ok(out) => {
                                let _ = result_tx.send(EngineResult::Done(out));
                            }
                            Err(e) => {
                                // A dead child cannot answer again: drop it so
                                // the next command starts a new one.
                                if client.as_mut().is_some_and(|c| !c.is_alive()) {
                                    client = None;
                                }
                                let _ = result_tx.send(EngineResult::Failed(e.to_string()));
                            }
                        }
                    }
                })
                .ok();
        }

        Self {
            jobs: job_tx,
            results: result_rx,
            bin,
        }
    }

    /// Queue a command. Answers arrive through [`Self::try_recv`].
    pub fn run(&self, command: impl Into<String>) {
        let _ = self.jobs.send(EngineJob {
            command: command.into(),
        });
    }

    pub fn try_recv(&self) -> Option<EngineResult> {
        self.results.try_recv().ok()
    }

    pub fn binary(&self) -> &str {
        &self.bin
    }
}

/// `AURA_BIN` wins, then `aura` on PATH. `None` means no engine is installed:
/// the GUI still works, engine-only commands say so.
pub fn find_aura_binary() -> Option<String> {
    if let Ok(path) = std::env::var("AURA_BIN") {
        let path = path.trim();
        if !path.is_empty() && Path::new(path).is_file() {
            return Some(path.to_string());
        }
    }
    let path = std::env::var("PATH").ok()?;
    for dir in path.split(':') {
        let candidate = Path::new(dir).join("aura");
        if candidate.is_file() {
            return Some(candidate.display().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_binary_is_reported_not_guessed() {
        // Not asserting None (CI images may have aura installed); only that a
        // found path really exists.
        if let Some(bin) = find_aura_binary() {
            assert!(Path::new(&bin).is_file(), "{bin}");
        }
    }
}
