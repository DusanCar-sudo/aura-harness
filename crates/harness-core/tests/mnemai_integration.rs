//! End-to-end check of the MCP client against the real Aura Mnemai server.
//!
//! Skips (rather than fails) when the `mnemai` launcher is not installed, so
//! the suite stays green on machines without the memory layer.

use harness_core::config::McpServer;
use harness_core::mcp::McpConnection;
use std::path::PathBuf;

fn launcher() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".local/bin/mnemai");
    path.exists().then_some(path)
}

#[tokio::test]
async fn connects_to_mnemai_and_lists_its_memory_tools() {
    let Some(launcher) = launcher() else {
        eprintln!("skipping: ~/.local/bin/mnemai not installed");
        return;
    };

    let server = McpServer {
        command: launcher.display().to_string(),
        args: vec!["serve".into()],
        env: Default::default(),
        enabled: true,
    };

    let mut conn = McpConnection::connect("mnemai", &server)
        .await
        .expect("mnemai handshake");

    let tools = conn.list_tools_raw().await.expect("tools/list");
    let names: Vec<&str> = tools.iter().map(|(name, _)| name.as_str()).collect();

    assert!(!names.is_empty(), "mnemai exposed no tools");
    assert!(
        names.iter().any(|n| n.contains("search")),
        "expected a search tool, got {names:?}"
    );

    // Namespacing must be intact for the model to call them.
    let (_, def) = &tools[0];
    assert!(
        def.name.starts_with("mcp_mnemai_"),
        "unexpected namespaced name {}",
        def.name
    );
}

#[tokio::test]
async fn a_missing_server_fails_instead_of_hanging() {
    let server = McpServer {
        command: "/nonexistent/definitely-not-a-server".into(),
        args: vec![],
        env: Default::default(),
        enabled: true,
    };
    assert!(McpConnection::connect("ghost", &server).await.is_err());
}
