//! Markdown rendering via `egui_commonmark`.
//!
//! Code blocks render syntax-highlighted (syntect) with a built-in copy
//! button, which is what the "rich markdown" requirement asks for.

use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

/// Render `text` as CommonMark into `ui`, reusing `cache` across frames.
pub fn show(ui: &mut egui::Ui, cache: &mut CommonMarkCache, text: &str) {
    CommonMarkViewer::new().show(ui, cache, text);
}
