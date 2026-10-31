//! Panels: conversation list, chat transcript, composer, settings.

pub mod chat;
pub mod filetree;
pub mod input;
pub mod model_factory;
pub mod palette;
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

    #[cfg(test)]
    pub fn is_pending(&self) -> bool {
        self.output.is_none()
    }

    /// True while a command tool waits for the user's verdict.
    pub fn awaiting_approval(&self) -> bool {
        self.needs_approval && self.decision.is_none()
    }
}

/// How a text field should look. The menu behaviour comes from [`text_edit`].
pub struct TextEditSpec<'a> {
    multiline: bool,
    hint: &'a str,
    width: Option<f32>,
    rows: usize,
    password: bool,
}

impl<'a> TextEditSpec<'a> {
    pub fn singleline() -> Self {
        Self { multiline: false, hint: "", width: None, rows: 3, password: false }
    }

    pub fn multiline() -> Self {
        Self { multiline: true, ..Self::singleline() }
    }

    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = hint;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = rows;
        self
    }

    pub fn password(mut self, password: bool) -> Self {
        self.password = password;
        self
    }
}

enum MenuAction {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

/// A text field that behaves like a desktop one: right-click gives Cut, Copy,
/// Paste and Select all, and Ctrl+A works. egui 0.29 ships neither, although
/// it does wire Ctrl+C/X/V and Ctrl+Z/Y in `TextEdit`.
pub fn text_edit(
    ui: &mut egui::Ui,
    text: &mut String,
    spec: TextEditSpec<'_>,
) -> egui::Response {
    let mut edit = if spec.multiline {
        egui::TextEdit::multiline(text)
    } else {
        egui::TextEdit::singleline(text)
    };
    if !spec.hint.is_empty() {
        edit = edit.hint_text(spec.hint);
    }
    if let Some(width) = spec.width {
        edit = edit.desired_width(width);
    }
    if spec.multiline {
        edit = edit.desired_rows(spec.rows);
    }
    if spec.password {
        edit = edit.password(true);
    }

    let output = edit.show(ui);
    let response = output.response;
    let id = response.id;
    let mut state = output.state;

    // Right-click should put the caret in the field it is acting on.
    if response.secondary_clicked() {
        response.request_focus();
    }

    // egui 0.29 ignores Ctrl+A in TextEdit; do it here while we know both the
    // state and the text.
    if response.has_focus() && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::A)) {
        select_all(text, &mut state);
        egui::TextEdit::store_state(ui.ctx(), id, state.clone());
        ui.ctx().request_repaint();
    }

    let mut action = None;
    response.context_menu(|ui| {
        let has_selection = selection_range(&state).is_some();
        if ui.add_enabled(has_selection, egui::Button::new("Cut")).clicked() {
            action = Some(MenuAction::Cut);
            ui.close_menu();
        }
        if ui.add_enabled(has_selection, egui::Button::new("Copy")).clicked() {
            action = Some(MenuAction::Copy);
            ui.close_menu();
        }
        if ui.button("Paste").clicked() {
            action = Some(MenuAction::Paste);
            ui.close_menu();
        }
        ui.separator();
        if ui.button("Select all").clicked() {
            action = Some(MenuAction::SelectAll);
            ui.close_menu();
        }
    });

    if let Some(action) = action {
        match action {
            MenuAction::Copy => {
                if let Some((a, b)) = selection_range(&state) {
                    ui.ctx().copy_text(slice_chars(text, a, b));
                }
            }
            MenuAction::Cut => {
                if let Some((a, b)) = selection_range(&state) {
                    ui.ctx().copy_text(slice_chars(text, a, b));
                    let (ba, bb) = (byte_index(text, a), byte_index(text, b));
                    text.replace_range(ba..bb, "");
                    set_cursor(&mut state, a);
                }
            }
            MenuAction::Paste => {
                if let Some(paste) = clipboard_text() {
                    insert_text(text, &mut state, &paste);
                }
            }
            MenuAction::SelectAll => select_all(text, &mut state),
        }
        egui::TextEdit::store_state(ui.ctx(), id, state.clone());
        response.request_focus();
        ui.ctx().request_repaint();
    }

    response
}

fn selection_range(state: &egui::widgets::text_edit::TextEditState) -> Option<(usize, usize)> {
    let range = state.cursor.char_range()?;
    let a = range.primary.index.min(range.secondary.index);
    let b = range.primary.index.max(range.secondary.index);
    (a != b).then_some((a, b))
}

fn set_cursor(state: &mut egui::widgets::text_edit::TextEditState, index: usize) {
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(index))));
}

fn select_all(text: &str, state: &mut egui::widgets::text_edit::TextEditState) {
    let len = text.chars().count();
    state.cursor.set_char_range(Some(egui::text::CCursorRange {
        primary: egui::text::CCursor::new(len),
        secondary: egui::text::CCursor::new(0),
    }));
}

fn slice_chars(text: &str, a: usize, b: usize) -> String {
    text.chars().skip(a).take(b.saturating_sub(a)).collect()
}

fn byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

fn insert_text(text: &mut String, state: &mut egui::widgets::text_edit::TextEditState, insert: &str) {
    let (a, b) = selection_range(state).unwrap_or_else(|| {
        let caret = state
            .cursor
            .char_range()
            .map(|range| range.primary.index)
            .unwrap_or_else(|| text.chars().count());
        (caret, caret)
    });
    let (ba, bb) = (byte_index(text, a), byte_index(text, b));
    text.replace_range(ba..bb, insert);
    set_cursor(state, a + insert.chars().count());
}

/// Read the system clipboard. arboard is the cross-platform path; the Linux
/// commands are a fallback for compositors where arboard cannot attach.
fn clipboard_text() -> Option<String> {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        if let Ok(text) = clipboard.get_text() {
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        for (cmd, args) in [
            ("wl-paste", &["--no-newline"][..]),
            ("xclip", &["-selection", "clipboard", "-o"][..]),
        ] {
            if let Ok(out) = std::process::Command::new(cmd).args(args).output() {
                if out.status.success() {
                    if let Ok(text) = String::from_utf8(out.stdout) {
                        if !text.is_empty() {
                            return Some(text);
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod text_edit_tests {
    use super::*;

    fn state_with_range(a: usize, b: usize) -> egui::widgets::text_edit::TextEditState {
        let mut state = egui::widgets::text_edit::TextEditState::default();
        state.cursor.set_char_range(Some(egui::text::CCursorRange {
            primary: egui::text::CCursor::new(a),
            secondary: egui::text::CCursor::new(b),
        }));
        state
    }

    #[test]
    fn paste_replaces_the_selection() {
        let mut text = "hello world".to_owned();
        let mut state = state_with_range(11, 6);
        insert_text(&mut text, &mut state, "there");
        assert_eq!(text, "hello there");
        assert_eq!(state.cursor.char_range().unwrap().primary.index, 11);
    }

    #[test]
    fn paste_uses_character_offsets_not_bytes() {
        let mut text = "héllo".to_owned();
        let mut state = state_with_range(1, 2);
        insert_text(&mut text, &mut state, "e");
        assert_eq!(text, "hello");
    }

    #[test]
    fn select_all_covers_the_whole_text() {
        let text = "abc".to_owned();
        let mut state = egui::widgets::text_edit::TextEditState::default();
        select_all(&text, &mut state);
        assert_eq!(selection_range(&state), Some((0, 3)));
        // And a cut through the helper's range math is lossless.
        let (a, b) = selection_range(&state).unwrap();
        assert_eq!(slice_chars(&text, a, b), text);
    }

    #[test]
    fn byte_index_clamps_past_the_end() {
        let text = "héllo";
        assert_eq!(byte_index(text, 0), 0);
        assert_eq!(byte_index(text, 1), 1);
        assert_eq!(byte_index(text, 2), 3);
        assert_eq!(byte_index(text, 99), text.len());
    }
}
