//! Live check against a real `aura sidecar`.
//!
//! Ignored by default: it needs aura-code installed and takes a few seconds
//! to boot Node. Run it with
//!
//! ```text
//! cargo test -p harness-core --test engine_live -- --ignored
//! ```

use harness_core::engine::EngineClient;

fn aura_bin() -> String {
    std::env::var("AURA_BIN").unwrap_or_else(|_| "aura".to_string())
}

#[test]
#[ignore = "spawns the real Aura engine"]
fn runs_a_core_command_and_reports_terminal_only() {
    let mut engine = EngineClient::spawn(&aura_bin(), ".", "", None, None)
        .expect("aura sidecar should start");

    let skills = engine.run_command("/skills").expect("command.run answers");
    assert!(skills.handled, "/skills should be handled by the core");
    assert!(!skills.terminal_only);
    let text = skills.lines.join("\n");
    assert!(
        text.to_lowercase().contains("skill"),
        "unexpected /skills output: {text}"
    );

    // The model picker drives the TUI's stdin; the harness handles it in its
    // own UI, and the engine must say so instead of silently doing nothing.
    let model = engine.run_command("/model").expect("command.run answers");
    assert!(model.terminal_only, "/model should be terminal-only");
    assert!(model.reason.is_some(), "terminal-only needs a reason");
}

#[test]
#[ignore = "spawns the real Aura engine"]
fn a_command_session_is_created_once_and_reused() {
    let mut engine = EngineClient::spawn(&aura_bin(), ".", "", None, None).expect("start");
    let first = engine.session().expect("session");
    engine.run_command("/id").expect("command");
    let second = engine.session().expect("session");
    assert_eq!(first, second, "one engine session per client");
}
