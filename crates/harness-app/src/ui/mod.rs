//! Panels: conversation list, chat transcript, composer, settings.

pub mod chat;
pub mod filetree;
pub mod input;
pub mod members;
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
        ui.painter()
            .rect_filled(rect, egui::Rounding::same(4.0), fill);
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

    let text_color = if selected {
        theme::accent()
    } else {
        theme::text()
    };
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
        let pos = egui::pos2(
            rect.right() - 6.0 - galley.size().x,
            rect.center().y - galley.size().y / 2.0,
        );
        ui.painter().galley(pos, galley, theme::dim());
    }

    response
}

fn combo_trigger_button(
    ui: &mut egui::Ui,
    label: &str,
    width: f32,
    is_open: bool,
) -> egui::Response {
    let height = ui.spacing().interact_size.y;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let vis = if is_open {
            &ui.visuals().widgets.open
        } else {
            ui.style().interact(&resp)
        };
        ui.painter()
            .rect(rect, vis.rounding, vis.weak_bg_fill, vis.bg_stroke);
        let arrow_w = 16.0;
        let text_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 6.0, rect.top() + 2.0),
            egui::pos2((rect.right() - arrow_w).max(rect.left() + 8.0), rect.bottom() - 2.0),
        );
        let font = egui::TextStyle::Small.resolve(ui.style());
        let galley = ui.painter().layout_no_wrap(label.to_string(), font, vis.text_color());
        let text_y = text_rect.center().y - galley.size().y * 0.5;
        ui.painter()
            .with_clip_rect(text_rect)
            .galley(egui::pos2(text_rect.left(), text_y), galley, vis.text_color());

        let cx = rect.right() - 9.0;
        let cy = rect.center().y;
        let pts = if is_open {
            vec![
                egui::pos2(cx - 3.5, cy + 2.0),
                egui::pos2(cx + 3.5, cy + 2.0),
                egui::pos2(cx, cy - 2.5),
            ]
        } else {
            vec![
                egui::pos2(cx - 3.5, cy - 2.0),
                egui::pos2(cx + 3.5, cy - 2.0),
                egui::pos2(cx, cy + 2.5),
            ]
        };
        ui.painter()
            .add(egui::Shape::convex_polygon(pts, vis.fg_stroke.color, egui::Stroke::NONE));
    }
    resp
}

fn pick_model_row(
    ui: &mut egui::Ui,
    selected_model: &mut String,
    value: &str,
    label: &str,
) -> bool {
    if ui
        .selectable_label(selected_model == value, label)
        .clicked()
    {
        *selected_model = value.to_string();
        ui.memory_mut(|m| m.close_popup());
        return true;
    }
    false
}

/// Reusable searchable dropdown chooser for selecting any model id (e.g. for
/// Lead Orchestrator, Subagent Default, or per-subagent overrides). Returns
/// `true` whenever `selected_model` changed.
pub fn model_dropdown(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash,
    selected_model: &mut String,
    empty_label: &str,
    models: &[harness_core::models::ModelInfo],
    width: f32,
) -> bool {
    let before = selected_model.clone();
    let shown = if selected_model.trim().is_empty() {
        empty_label.to_string()
    } else {
        let name = harness_core::models::display_name(selected_model);
        if name == selected_model.as_str() {
            name
        } else {
            format!("{name} ({})", selected_model.trim())
        }
    };

    let button_id = ui.make_persistent_id(&id_salt);
    let popup_id = button_id.with("popup");
    let q_id = button_id.with("filter");
    let mut query: String = ui.memory(|m| m.data.get_temp(q_id)).unwrap_or_default();

    let is_open = ui.memory(|m| m.is_popup_open(popup_id));
    let button_resp = combo_trigger_button(ui, &shown, width, is_open);
    let mut just_opened = false;
    if button_resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
        just_opened = ui.memory(|m| m.is_popup_open(popup_id));
    }

    let screen = ui.ctx().screen_rect();
    let space_below = screen.bottom() - button_resp.rect.bottom();
    let space_above = button_resp.rect.top() - screen.top();
    let placement = if space_below < 320.0 && space_above > space_below {
        egui::AboveOrBelow::Above
    } else {
        egui::AboveOrBelow::Below
    };

    // Use `CloseOnClickOutside` instead of `ComboBox` (`CloseOnClick`), because
    // `CloseOnClick` closes the popup as soon as the user clicks inside the
    // filter TextEdit search box.
    let mut picked = false;
    egui::popup_above_or_below_widget(
        ui,
        popup_id,
        &button_resp,
        placement,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(width.max(350.0));
            let filter_resp = ui
                .horizontal(|ui| {
                    ui.label(egui::RichText::new("Filter").color(theme::dim()).small());
                    let r = text_edit(
                        ui,
                        &mut query,
                        TextEditSpec::singleline()
                            .hint("provider, model, local, free…")
                            .width(220.0),
                    );
                    if !query.is_empty()
                        && ui.small_button("x").on_hover_text("Clear filter").clicked()
                    {
                        query.clear();
                        r.request_focus();
                    }
                    r
                })
                .inner;
            if just_opened {
                filter_resp.request_focus();
            }
            ui.separator();

            egui::ScrollArea::vertical()
                .id_salt(popup_id.with("scroll"))
                .max_height(310.0)
                .show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    if pick_model_row(ui, selected_model, "", empty_label) {
                        picked = true;
                    }
                    ui.separator();

                    let q = query.trim().to_lowercase();
                    let matched: Vec<&harness_core::models::ModelInfo> =
                        models.iter().filter(|m| model_matches(m, &q)).collect();

                    if matched.is_empty() {
                        ui.small(
                            egui::RichText::new("No models match filter").color(theme::dim()),
                        );
                        return;
                    }

                    let groups = group_models_by_provider(&matched);
                    let selected_family = if selected_model.trim().is_empty() {
                        ""
                    } else {
                        harness_core::models::provider_for_model(selected_model)
                    };

                    for (idx, g) in groups.iter().enumerate() {
                        if idx > 0 && !q.is_empty() {
                            ui.add_space(4.0);
                        }
                        let header = if g.tag.is_empty() {
                            format!("{}  ({})", g.display_name, g.models.len())
                        } else {
                            format!("{}  ({} · {})", g.display_name, g.models.len(), g.tag)
                        };
                        let header_color = match g.tier {
                            0 | 1 => theme::accent(),
                            2 => theme::ok(),
                            _ => theme::dim(),
                        };

                        if q.is_empty() {
                            let default_open = g.family == selected_family
                                || (!selected_family.is_empty()
                                    && g.models.iter().any(|m| m.id == *selected_model));
                            egui::CollapsingHeader::new(
                                egui::RichText::new(header).color(header_color).small().strong(),
                            )
                            .id_salt(popup_id.with(("prov", &g.family)))
                            .default_open(default_open)
                            .show(ui, |ui| {
                                for m in &g.models {
                                    let label = format_model_option_label(m);
                                    if pick_model_row(ui, selected_model, &m.id, &label) {
                                        picked = true;
                                    }
                                }
                            });
                        } else {
                            ui.small(
                                egui::RichText::new(header).color(header_color).strong(),
                            );
                            for m in &g.models {
                                let label = format_model_option_label(m);
                                if pick_model_row(ui, selected_model, &m.id, &label) {
                                    picked = true;
                                }
                            }
                        }
                    }
                });
        },
    );

    if picked || !ui.memory(|m| m.is_popup_open(popup_id)) {
        query.clear();
    }
    ui.memory_mut(|m| m.data.insert_temp(q_id, query));
    *selected_model != before
}

struct ProviderModelGroup<'a> {
    tier: u8,
    family: String,
    display_name: String,
    tag: &'static str,
    models: Vec<&'a harness_core::models::ModelInfo>,
}

fn provider_tier_and_tag(family: &str) -> (u8, &'static str) {
    if harness_core::provider::custom_providers()
        .iter()
        .any(|c| c.id == family)
    {
        return (0, "custom");
    }
    if let Some(spec) = harness_core::models::provider(family) {
        if spec.local {
            return (1, "local");
        }
        if harness_core::models::is_configured(spec) {
            return (2, "ready");
        }
        return (3, "");
    }
    (3, "")
}

fn group_models_by_provider<'a>(
    models: &[&'a harness_core::models::ModelInfo],
) -> Vec<ProviderModelGroup<'a>> {
    let mut by_family: std::collections::BTreeMap<String, Vec<&'a harness_core::models::ModelInfo>> =
        std::collections::BTreeMap::new();
    for &m in models {
        by_family.entry(m.family.clone()).or_default().push(m);
    }
    let mut groups: Vec<ProviderModelGroup<'a>> = by_family
        .into_iter()
        .map(|(family, items)| {
            let (tier, tag) = provider_tier_and_tag(&family);
            let display_name = harness_core::models::provider_display_name(&family);
            ProviderModelGroup {
                tier,
                family,
                display_name,
                tag,
                models: items,
            }
        })
        .collect();
    groups.sort_by(|a, b| {
        a.tier
            .cmp(&b.tier)
            .then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()))
    });
    groups
}

fn format_model_option_label(m: &harness_core::models::ModelInfo) -> String {
    if m.free && m.speed.to_lowercase().contains("local") {
        format!("{} · {} [local]", m.name, m.id)
    } else if m.free {
        format!("{} · {} [free]", m.name, m.id)
    } else {
        format!("{} · {}", m.name, m.id)
    }
}

fn model_matches(m: &harness_core::models::ModelInfo, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    let prov = harness_core::models::provider_display_name(&m.family).to_lowercase();
    m.name.to_lowercase().contains(q)
        || m.id.to_lowercase().contains(q)
        || m.family.to_lowercase().contains(q)
        || prov.contains(q)
        || (q == "free" && m.free)
        || (q == "local" && m.speed.to_lowercase().contains("local"))
}

/// What the right-hand panel is showing. The preview lives in the same box as
/// the file tree, one tab over, so a build can be watched where it is edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RightTab {
    #[default]
    Files,
    Preview,
    Plugins,
    Members,
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
        Self {
            multiline: false,
            hint: "",
            width: None,
            rows: 3,
            password: false,
        }
    }

    pub fn multiline() -> Self {
        Self {
            multiline: true,
            ..Self::singleline()
        }
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
pub fn text_edit(ui: &mut egui::Ui, text: &mut String, spec: TextEditSpec<'_>) -> egui::Response {
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
        if ui
            .add_enabled(has_selection, egui::Button::new("Cut"))
            .clicked()
        {
            action = Some(MenuAction::Cut);
            ui.close_menu();
        }
        if ui
            .add_enabled(has_selection, egui::Button::new("Copy"))
            .clicked()
        {
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
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(index),
        )));
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

fn insert_text(
    text: &mut String,
    state: &mut egui::widgets::text_edit::TextEditState,
    insert: &str,
) {
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

#[cfg(test)]
mod model_dropdown_tests {
    use super::*;
    use harness_core::models::ModelInfo;

    fn sample_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "lmstudio/liquid/lfm2.5-1.2b".into(),
                name: "LFM2.5 1.2B Instruct".into(),
                family: "lmstudio".into(),
                speed: "Local".into(),
                free: true,
            },
            ModelInfo {
                id: "aura-local/lfm3-1.7b-aura-grounded".into(),
                name: "LFM3 1.7B Aura Grounded".into(),
                family: "aura-local".into(),
                speed: "Local".into(),
                free: true,
            },
            ModelInfo {
                id: "deepseek/deepseek-chat".into(),
                name: "DeepSeek Chat".into(),
                family: "deepseek".into(),
                speed: "fast".into(),
                free: false,
            },
        ]
    }

    #[test]
    fn groups_models_by_provider_with_local_tier_first() {
        let models = sample_models();
        let refs: Vec<&ModelInfo> = models.iter().collect();
        let groups = group_models_by_provider(&refs);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].tag, "local");
        assert_eq!(groups[1].tag, "local");
        assert!(matches!(groups[0].family.as_str(), "aura-local" | "lmstudio"));
    }

    #[test]
    fn clicking_search_box_inside_dropdown_keeps_popup_open() {
        let ctx = egui::Context::default();
        let models = sample_models();
        let mut selected = String::new();
        let mut button_rect = egui::Rect::NOTHING;
        let mut popup_id = egui::Id::NULL;

        let screen = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        ));

        // Frame 1: render closed button to measure its rect.
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: screen,
                time: Some(1.0),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let top_left = ui.next_widget_position();
                    button_rect = egui::Rect::from_min_size(
                        top_left,
                        egui::vec2(180.0, ui.spacing().interact_size.y),
                    );
                    model_dropdown(
                        ui,
                        "subagent-test",
                        &mut selected,
                        "Follow main",
                        &models,
                        180.0,
                    );
                    popup_id = ui.make_persistent_id("subagent-test").with("popup");
                });
            },
        );

        // Frame 2-4: hover, press & release on button to open popup.
        let click_pos = button_rect.center();
        let steps: [(f64, Vec<egui::Event>); 3] = [
            (1.1, vec![egui::Event::PointerMoved(click_pos)]),
            (
                1.2,
                vec![egui::Event::PointerButton {
                    pos: click_pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
            ),
            (
                1.3,
                vec![egui::Event::PointerButton {
                    pos: click_pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            ),
        ];
        for (t, events) in steps {
            let input = egui::RawInput {
                screen_rect: screen,
                time: Some(t),
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    model_dropdown(
                        ui,
                        "subagent-test",
                        &mut selected,
                        "Follow main",
                        &models,
                        180.0,
                    );
                });
            });
        }
        assert!(
            ctx.memory(|m| m.is_popup_open(popup_id)),
            "expected popup {popup_id:?} to open after clicking {click_pos:?} (button_rect={button_rect:?})"
        );

        // Frame 5-7: hover and click directly inside the filter TextEdit box.
        let search_pos = egui::pos2(button_rect.left() + 80.0, button_rect.bottom() + 18.0);
        let search_steps: [(f64, Vec<egui::Event>); 3] = [
            (2.0, vec![egui::Event::PointerMoved(search_pos)]),
            (
                2.1,
                vec![egui::Event::PointerButton {
                    pos: search_pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
            ),
            (
                2.2,
                vec![egui::Event::PointerButton {
                    pos: search_pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            ),
        ];
        for (t, events) in search_steps {
            let input = egui::RawInput {
                screen_rect: screen,
                time: Some(t),
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    model_dropdown(
                        ui,
                        "subagent-test",
                        &mut selected,
                        "Follow main",
                        &models,
                        180.0,
                    );
                });
            });
        }
        assert!(
            ctx.memory(|m| m.is_popup_open(popup_id)),
            "clicking inside the search filter box must not close the dropdown"
        );
    }
}
