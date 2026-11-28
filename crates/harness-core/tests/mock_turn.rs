//! Offline chat→tool→answer turn against a local mock OpenAI-compatible
//! server: proves the loop executes a built-in tool and feeds the result back,
//! with no network, key or UI.

use std::sync::Arc;

use harness_core::agent::{self, LoopEvent, LoopInput};
use harness_core::api::{ChatOptions, Message};
use harness_core::tools::ToolRegistry;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

fn sse(chunks: &[&str]) -> String {
    let mut out = String::new();
    for c in chunks {
        out.push_str(&format!("data: {c}\n\n"));
    }
    out.push_str("data: [DONE]\n\n");
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{out}",
        out.len()
    )
}

/// Serve one canned response per connection, in order; returns the base URL.
async fn mock_server(responses: Vec<String>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        for resp in responses {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 65536];
            let _ = sock.read(&mut buf).await;
            sock.write_all(resp.as_bytes()).await.unwrap();
            let _ = sock.shutdown().await;
        }
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn a_turn_calls_a_tool_and_answers_from_its_output() {
    let dir = std::env::temp_dir().join(format!("aura-mock-turn-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("note.txt"), "mock-secret-42").unwrap();

    let call = r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"note.txt\"}"}}]},"finish_reason":null}]}"#;
    let call_done = r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#;
    let text = r#"{"choices":[{"delta":{"content":"It says mock-secret-42."},"finish_reason":null}]}"#;
    let text_done = r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
    let base = mock_server(vec![sse(&[call, call_done]), sse(&[text, text_done])]).await;

    let mut registry = ToolRegistry::new(true);
    registry.set_cwd(&dir);
    let (_appr_tx, appr_rx) = agent::approval_channel();
    let input = LoopInput {
        http: reqwest::Client::new(),
        api_key: "test".into(),
        history: vec![Message::user("what does note.txt say?")],
        opts: ChatOptions { base_url: Some(base), ..Default::default() },
        registry,
        mcp: Arc::new(Mutex::new(Default::default())),
        approvals: appr_rx,
        subagent: None,
    };
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    tokio::spawn(async move {
        let _ = agent::run(input, tx).await;
    });

    let (mut tool_out, mut answer, mut approvals) = (String::new(), String::new(), 0);
    while let Some(ev) = rx.recv().await {
        match ev {
            LoopEvent::ToolRequested { needs_approval, .. } => approvals += needs_approval as u32,
            LoopEvent::ToolFinished { output, .. } => tool_out = output,
            LoopEvent::Content(c) => answer.push_str(&c),
            LoopEvent::Finished { rounds } => {
                assert_eq!(rounds, 2);
                break;
            }
            LoopEvent::Failed(e) => panic!("loop failed: {e}"),
            _ => {}
        }
    }
    assert_eq!(approvals, 0);
    assert!(tool_out.contains("mock-secret-42"), "{tool_out}");
    assert_eq!(answer, "It says mock-secret-42.");
    let _ = std::fs::remove_dir_all(&dir);
}
