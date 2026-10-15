//! Panels: conversation list, chat transcript, composer, settings.

pub mod chat;
pub mod filetree;
pub mod input;
pub mod plugin_browser;
pub mod settings;
pub mod sidebar;

use harness_core::agent::Approval;

/// A tool call rendered in the transcript, with its result once it lands.
#[derive(Debug, Clone)]
pub struct ToolCard {
    pub call_id: String,
    pub name: String,
    pub summary: String,
    pub needs_approval: bool,
    pub decision: Option<Approval>,
    pub output: Option<String>,
    pub is_error: bool,
}

impl ToolCard {
    pub fn new(call_id: String, name: String, summary: String, needs_approval: bool) -> Self {
        Self {
            call_id,
            name,
            summary,
            needs_approval,
            decision: None,
            output: None,
            is_error: false,
        }
    }

    pub fn is_pending(&self) -> bool {
        self.output.is_none()
    }

    /// True while a command tool waits for the user's verdict.
    pub fn awaiting_approval(&self) -> bool {
        self.needs_approval && self.decision.is_none()
    }
}
