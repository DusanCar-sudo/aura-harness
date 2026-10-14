//! Chat transcript: messages, streaming answer, reasoning, tool cards.

use crate::markdown;
use crate::theme;
use crate::ui::ToolCard;
use harness_core::agent::Approval;
use harness_core::api::Message;
use egui::{Align, Layout, RichText, Ui};

/// Live streaming buffers for the turn currently in flight.
#[derive(Debug, Default)]
pub struct LiveTurn {
    pub reasoning: String,
    pub content: String,
    /// Seconds spent thinking, frozen once the turn completes.
    pub reasoning_secs: Option<f32>,
    pub reasoning_started: Option<std::time::Instant>,
}

impl LiveTurn {
    pub fn clear(&mut self) {
        self.reasoning.clear();
        self.content.clear();
        self.reasoning_secs = None;
        self.reasoning_started = None;
    }

    pub fn is_empty(&self) -> bool {
        self.reasoning.is_empty() && self.content.is_empty()
    }

    /// Note the first reasoning delta so we can show "Thought for Ns".
    pub fn touch_reasoning(&mut self) {
        if self.reasoning_started.is_none() {
            self.reasoning_started = Some(std::time::Instant::now());
        }
    }

    pub fn freeze_reasoning_clock(&mut self) {
        if let Some(start) = self.reasoning_started.take() {
            self.reasoning_secs = Some(start.elapsed().as_secs_f32());
        }
    }
}

/// What the user asked the transcript to do.
#[derive(Debug, Clone)]
pub enum ChatAction {
    Approve(String),
    Reject(String),
}

pub struct ChatState<'a> {
    pub messages: &'a [Message],
    pub live: &'a LiveTurn,
    pub cards: &'a [ToolCard],
    pub busy: bool,
    pub cache: &'a mut egui_commonmark::CommonMarkCache,
}

pub fn show(ui: &mut Ui, state: &mut ChatState<'_>) -> Option<ChatAction> {
    let mut action = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            if state.messages.is_empty() && state.live.is_empty() {
                welcome(ui);
                return;
            }

            for msg in state.messages {
                render_message(ui, state.cache, msg);
            }

            // Tool activity sits after the turn that requested it.
            for card in state.cards {
                if let Some(a) = render_tool_card(ui, card) {
                    action = Some(a);
                }
            }

            render_live(ui, state);
        });

    action
}

fn welcome(ui: &mut Ui) {
    ui.add_space(ui.available_height() * 0.28);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("AURA HARNESS").color(theme::accent()).strong().size(28.0));
        theme::stripes(ui, 180.0);
        ui.add_space(6.0);
        ui.label(
            RichText::new("DeepSeek harness — streaming reasoning, markdown, your plugins.")
                .color(theme::dim()),
        );
    });
}

fn render_message(ui: &mut Ui, cache: &mut egui_commonmark::CommonMarkCache, msg: &Message) {
    match msg.role.as_str() {
        "user" => {
            egui::Frame::none()
                .fill(theme::bubble())
                .inner_margin(egui::Margin::same(10.0))
                .rounding(egui::Rounding::same(6.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("You").color(theme::structure()).strong().small());
                    ui.label(RichText::new(&msg.content).color(theme::text()));
                });
            ui.add_space(6.0);
        }
        "assistant" => {
            if !msg.reasoning_content.is_empty() {
                reasoning_block(ui, &msg.reasoning_content, None);
            }
            if !msg.content.is_empty() {
                markdown::show(ui, cache, &msg.content);
            }
            ui.add_space(6.0);
        }
        "tool" => {
            egui::Frame::none()
                .fill(theme::field())
                .inner_margin(egui::Margin::same(8.0))
                .rounding(egui::Rounding::same(4.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("tool result").color(theme::dim()).small());
                    ui.label(RichText::new(&msg.content).color(theme::dim()).monospace());
                });
            ui.add_space(4.0);
        }
        _ => {}
    }
}

/// Dimmed, collapsed-by-default chain of thought.
fn reasoning_block(ui: &mut Ui, text: &str, secs: Option<f32>) {
    let header = match secs {
        Some(s) => format!("Thought for {s:.1}s"),
        None => "Reasoning".to_string(),
    };
    egui::CollapsingHeader::new(RichText::new(header).color(theme::dim()).italics())
        .id_salt(("reasoning", text.len()))
        .default_open(false)
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(theme::dim()).italics());
        });
}

fn render_tool_card(ui: &mut Ui, card: &ToolCard) -> Option<ChatAction> {
    let mut action = None;

    let accent = if card.is_error {
        theme::danger()
    } else if card.is_pending() {
        theme::warn()
    } else {
        theme::ok()
    };

    egui::Frame::none()
        .fill(theme::card())
        .stroke(egui::Stroke::new(1.0_f32, accent))
        .inner_margin(egui::Margin::same(8.0))
        .rounding(egui::Rounding::same(6.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("tool").color(accent).small());
                ui.label(RichText::new(&card.name).color(theme::text()).strong().monospace());
                if card.is_pending() {
                    ui.spinner();
                }
            });
            ui.label(RichText::new(&card.summary).color(theme::dim()).monospace().small());

            if card.awaiting_approval() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new("This runs a local command. Approve?")
                        .color(theme::warn())
                        .small(),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Approve").color(theme::on_accent()).strong())
                                .fill(theme::accent()),
                        )
                        .clicked()
                    {
                        action = Some(ChatAction::Approve(card.call_id.clone()));
                    }
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Reject").color(theme::text()).strong())
                                .fill(theme::danger()),
                        )
                        .clicked()
                    {
                        action = Some(ChatAction::Reject(card.call_id.clone()));
                    }
                });
            } else if let Some(output) = &card.output {
                egui::CollapsingHeader::new(
                    RichText::new("output").color(theme::dim()).small().monospace(),
                )
                .id_salt(("tool-out", card.call_id.as_str()))
                .default_open(false)
                .show(ui, |ui| {
                    ui.label(RichText::new(output).color(theme::text()).monospace().small());
                });
            } else if card.decision == Some(Approval::Reject) {
                ui.label(RichText::new("rejected").color(theme::dim()).small());
            }
        });
    ui.add_space(6.0);

    action
}

fn render_live(ui: &mut Ui, state: &mut ChatState<'_>) {
    if !state.live.reasoning.is_empty() {
        reasoning_block(ui, &state.live.reasoning, state.live.reasoning_secs);
    }
    if !state.live.content.is_empty() {
        markdown::show(ui, state.cache, &state.live.content);
    }
    if state.busy && state.live.content.is_empty() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new("thinking…").color(theme::dim()).italics());
        });
    }
    ui.with_layout(Layout::right_to_left(Align::TOP), |_ui| {});
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_turn_tracks_reasoning_clock() {
        let mut live = LiveTurn::default();
        assert!(live.is_empty());
        live.reasoning.push_str("hmm");
        assert!(!live.is_empty());
        live.touch_reasoning();
        assert!(live.reasoning_started.is_some());
        live.freeze_reasoning_clock();
        assert!(live.reasoning_secs.is_some());
        assert!(live.reasoning_started.is_none());
        live.clear();
        assert!(live.is_empty());
        assert!(live.reasoning_secs.is_none());
    }

    #[test]
    fn tool_card_approval_lifecycle() {
        let mut card = ToolCard::new("c1".into(), "cmd_echo".into(), "cmd_echo({})".into(), true);
        assert!(card.is_pending());
        assert!(card.awaiting_approval());
        card.decision = Some(Approval::Approve);
        assert!(!card.awaiting_approval());
        card.output = Some("ok".into());
        assert!(!card.is_pending());
    }

    #[test]
    fn mcp_card_never_awaits_approval() {
        let card = ToolCard::new("c1".into(), "mcp_x_y".into(), "mcp_x_y({})".into(), false);
        assert!(!card.awaiting_approval());
    }
}
