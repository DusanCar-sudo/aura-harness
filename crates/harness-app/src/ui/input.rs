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
    OpenModelFactory,
    ToggleMic,
}

/// Microphone state shown in the composer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoiceStatus {
    #[default]
    Idle,
    Recording,
    Transcribing,
}

pub struct ComposerState<'a> {
    pub draft: &'a mut String,
    pub attachments: &'a [Attachment],
    pub model: &'a mut ModelChoice,
    pub effort: &'a mut EffortChoice,
    pub selected_model: Option<String>,
    pub busy: bool,
    pub tool_count: usize,
    pub voice: VoiceStatus,
    pub recording_secs: Option<f32>,
    /// Put the caret in the text box this frame (launch, after Send/Stop, new chat).
    pub request_focus: bool,
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
    if state.request_focus {
        response.request_focus();
    }

    // Enter sends; Shift+Enter inserts a newline.
    if response.has_focus() && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter)) {
        action = Some(ComposerAction::Send);
    }

    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Attach")
            .on_hover_text("Attach text files as context")
            .clicked()
        {
            action = Some(ComposerAction::Attach);
        }

        let mic_label = match state.voice {
            VoiceStatus::Idle => "Mic".to_string(),
            VoiceStatus::Recording => match state.recording_secs {
                Some(secs) => format!("Stop mic {secs:.0}s"),
                None => "Stop mic".to_string(),
            },
            VoiceStatus::Transcribing => "Transcribing…".to_string(),
        };
        if ui
            .button(mic_label)
            .on_hover_text("Record speech and put the transcription in the composer")
            .clicked()
            && state.voice != VoiceStatus::Transcribing
        {
            action = Some(ComposerAction::ToggleMic);
        }

        let current = state.selected_model.as_deref().unwrap_or_else(|| model_label(*state.model));
        let current_name = harness_core::models::display_name(current);
        if ui
            .button(format!("Model: {current_name}"))
            .on_hover_text(format!("{current}\nOpen the full provider → model factory"))
            .clicked()
        {
            action = Some(ComposerAction::OpenModelFactory);
        }

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

    /// Run one egui frame with the composer and report whether a widget holds focus.
    fn composer_holds_focus_after_frame(request_focus: bool) -> bool {
        let ctx = egui::Context::default();
        let mut draft = String::new();
        let mut model = ModelChoice::V4Pro;
        let mut effort = EffortChoice::Max;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                show(
                    ui,
                    &mut ComposerState {
                        draft: &mut draft,
                        attachments: &[],
                        model: &mut model,
                        effort: &mut effort,
                        selected_model: None,
                        busy: false,
                        tool_count: 0,
                        voice: VoiceStatus::Idle,
                        recording_secs: None,
                        request_focus,
                    },
                );
            });
        });
        ctx.memory(|m| m.focused().is_some())
    }

    #[test]
    fn composer_takes_focus_only_when_asked() {
        assert!(composer_holds_focus_after_frame(true));
        assert!(!composer_holds_focus_after_frame(false));
    }

    #[test]
    fn labels_match_api_model_ids() {
        assert_eq!(model_label(ModelChoice::Flash), "deepseek-flash");
        assert_eq!(model_label(ModelChoice::V4Pro), "deepseek-v4-pro");
        assert_eq!(effort_label(EffortChoice::None), "thinking off");
    }
}
