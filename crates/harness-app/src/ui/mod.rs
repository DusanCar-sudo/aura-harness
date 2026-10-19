//! Panels: conversation list, chat transcript, composer, settings.

pub mod chat;
pub mod filetree;
pub mod input;
pub mod model_factory;
pub mod plugin_browser;
pub mod preview;
pub mod settings;
pub mod sidebar;

use harness_core::agent::Approval;

use crate::theme;

/// One full-width clickable row: left-aligned label, optional right hint.
///
/// egui's ordinary labels only react on their glyphs, which is why plugin rows
/// felt "missed by one row": the Add button was a tiny target at the far right.
/// This paints the whole row and returns one response for it, so a click
/// anywhere on the line selects/adds that item.
pub fn row_button(
    ui: &mut egui::Ui,
    height: f32,
    selected: bool,
    label: &str,
    right: &str,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());

    if selected || response.hovered() {
        let fill = if selected {
            theme::accent().gamma_multiply(0.18)
        } else {
            theme::bubble()
        };
        ui.painter().rect_filled(rect, egui::Rounding::same(4.0), fill);
    }

    let right_galley = if right.is_empty() {
        None
    } else {
        Some(ui.painter().layout_no_wrap(
            right.to_string(),
            egui::FontId::proportional(11.0),
            theme::dim(),
        ))
    };
    let right_width = right_galley
        .as_ref()
        .map(|g| g.size().x + 10.0)
        .unwrap_or(0.0);

    let text_color = if selected { theme::accent() } else { theme::text() };
    let clip = egui::Rect::from_min_max(
        rect.min + egui::vec2(6.0, 0.0),
        egui::pos2(rect.right() - right_width, rect.bottom()),
    );
    ui.painter().with_clip_rect(clip).text(
        rect.left_center() + egui::vec2(6.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        text_color,
    );

    if let Some(galley) = right_galley {
        let pos = egui::pos2(rect.right() - 6.0 - galley.size().x, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(pos, galley, theme::dim());
    }

    response
}

/// What the right-hand panel is showing. The preview lives in the same box as
/// the file tree, one tab over, so a build can be watched where it is edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RightTab {
    #[default]
    Files,
    Preview,
    Plugins,
}

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
