pub mod agent;
pub mod api;
pub mod attach;
pub mod catalog;
pub mod cmdtool;
pub mod config;
pub mod error;
pub mod mcp;
pub mod online;
pub mod session;
pub mod tools;

pub use error::{Error, Result};

/// The environment a spawned child (command tool, MCP server) is given:
/// nothing inherited, only what running a program needs, plus whatever the
/// plugin itself declares. Stops this app's own secrets — the API key,
/// session tokens in the parent shell — from leaking into every tool.
pub fn child_env() -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = vec![
        (
            "PATH".into(),
            std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into()),
        ),
        (
            "HOME".into(),
            std::env::var("HOME").unwrap_or_else(|_| ".".into()),
        ),
    ];
    for key in ["LANG", "LC_ALL", "TMPDIR"] {
        if let Ok(v) = std::env::var(key) {
            env.push((key.into(), v));
        }
    }
    env
}

/// Serializes tests that mutate shared env vars (`AURA_HARNESS_HOME`,
/// `DEEPSEEK_API_KEY`) — cargo runs tests on parallel threads that would
/// otherwise race each other's paths.
#[cfg(test)]
pub(crate) mod env_lock {
    use std::sync::{Mutex, MutexGuard};

    static LOCK: Mutex<()> = Mutex::new(());

    pub fn guard() -> MutexGuard<'static, ()> {
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
