//! Composer: text entry, attachments, model/effort pickers, send/stop.

use crate::theme;
use harness_core::attach::Attachment;
use harness_core::config::{EffortChoice, ModelChoice};
use egui::{Key, RichText, Ui};

/// What the user did in the composer this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposerAction {
    Send,
    Stop,
    Attach,
    RemoveAttachment(usize),
}

pub struct ComposerState<'a> {
    pub draft: &'a mut String,
    pub attachments: &'a [Attachment],
    pub model: &'a mut ModelChoice,
    pub effort: &'a mut EffortChoice,
    pub busy: bool,
    pub tool_count: usize,
}

pub fn show(ui: &mut Ui, state: &mut ComposerState<'_>) -> Option<ComposerAction> {
    let mut action = None;

    for (i, att) in state.attachments.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(short_path(&att.path))
                    .color(theme::dim())
                    .small(),
            );
            if ui.small_button("x").clicked() {
                action = Some(ComposerAction::RemoveAttachment(i));
            }
        });
    }

    let hint = if state.busy {
        "Generating…  (Stop to abort)"
    } else {
        "Ask anything…  (Enter to send, Shift+Enter for a new line)"
    };

    let response = ui.add(
        egui::TextEdit::multiline(state.draft)
            .hint_text(hint)
            .desired_rows(3)
            .desired_width(f32::INFINITY),
    );

    // Enter sends; Shift+Enter inserts a newline.
    if response.has_focus() && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter)) {
        action = Some(ComposerAction::Send);
    }

    ui.horizontal(|ui| {
        if ui
            .button("Attach")
            .on_hover_text("Attach text files as context")
            .clicked()
        {
            action = Some(ComposerAction::Attach);
        }

        egui::ComboBox::from_id_salt("model-picker")
            .selected_text(model_label(*state.model))
            .show_ui(ui, |ui| {
                ui.selectable_value(state.model, ModelChoice::Flash, "deepseek-flash");
                ui.selectable_value(state.model, ModelChoice::V4Pro, "deepseek-v4-pro");
            });

        egui::ComboBox::from_id_salt("effort-picker")
            .selected_text(effort_label(*state.effort))
            .show_ui(ui, |ui| {
                ui.selectable_value(state.effort, EffortChoice::None, "thinking off");
                ui.selectable_value(state.effort, EffortChoice::Low, "effort: low");
                ui.selectable_value(state.effort, EffortChoice::High, "effort: high");
                ui.selectable_value(state.effort, EffortChoice::Max, "effort: max");
            });

        if state.tool_count > 0 {
            ui.label(
                RichText::new(format!("{} tools", state.tool_count))
                    .color(theme::dim())
                    .small(),
            );
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if state.busy {
                if ui
                    .add(
                        egui::Button::new(RichText::new("Stop").color(theme::text()).strong())
                            .fill(theme::danger()),
                    )
                    .clicked()
                {
                    action = Some(ComposerAction::Stop);
                }
            } else {
                let send = egui::Button::new(RichText::new("Send").color(theme::on_accent()).strong())
                    .fill(theme::accent());
                if ui.add(send).clicked() {
                    action = Some(ComposerAction::Send);
                }
            }
        });
    });

    action
}

fn model_label(m: ModelChoice) -> &'static str {
    match m {
        ModelChoice::Flash => "deepseek-flash",
        ModelChoice::V4Pro => "deepseek-v4-pro",
    }
}

fn effort_label(e: EffortChoice) -> &'static str {
    match e {
        EffortChoice::None => "thinking off",
        EffortChoice::Low => "effort: low",
        EffortChoice::High => "effort: high",
        EffortChoice::Max => "effort: max",
    }
}

/// Middle-truncate a path so the filename stays visible.
fn short_path(path: &str) -> String {
    const MAX: usize = 42;
    if path.chars().count() <= MAX {
        return path.to_string();
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    format!("…/{name}")
}

/// True when the draft has something worth sending.
pub fn is_sendable(draft: &str, attachments: usize) -> bool {
    !draft.trim().is_empty() || attachments > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sendable_needs_text_or_attachment() {
        assert!(!is_sendable("", 0));
        assert!(!is_sendable("   \n ", 0));
        assert!(is_sendable("hi", 0));
        assert!(is_sendable("", 1));
    }

    #[test]
    fn shortens_deep_paths() {
        let p = format!("/home/dusan/{}/notes.md", "a".repeat(60));
        let out = short_path(&p);
        assert!(out.starts_with('…'));
        assert!(out.ends_with("notes.md"));
        assert_eq!(short_path("/tmp/x.txt"), "/tmp/x.txt");
    }

    #[test]
    fn labels_match_api_model_ids() {
        assert_eq!(model_label(ModelChoice::Flash), "deepseek-flash");
        assert_eq!(model_label(ModelChoice::V4Pro), "deepseek-v4-pro");
        assert_eq!(effort_label(EffortChoice::None), "thinking off");
    }
}
