//! Chat transcript: messages, streaming answer, reasoning, tool cards.

use crate::markdown;
use crate::theme;
use crate::ui::ToolCard;
use base64::Engine;
use harness_core::agent::Approval;
use harness_core::api::{ImagePart, Message};
use egui::{Align, Layout, RichText, Ui};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Decoded attachment thumbnails, keyed by a hash of the base64 payload.
pub type ImageCache = HashMap<u64, (egui::TextureHandle, egui::Vec2)>;

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
    /// Read an assistant answer aloud through the voice chain.
    Speak(String),
}

pub struct ChatState<'a> {
    pub messages: &'a [Message],
    pub live: &'a LiveTurn,
    pub cards: &'a [ToolCard],
    pub busy: bool,
    pub cache: &'a mut egui_commonmark::CommonMarkCache,
    pub images: &'a mut ImageCache,
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
                if let Some(a) = render_message(ui, state.cache, state.images, msg) {
                    action = Some(a);
                }
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

fn render_message(
    ui: &mut Ui,
    cache: &mut egui_commonmark::CommonMarkCache,
    images: &mut ImageCache,
    msg: &Message,
) -> Option<ChatAction> {
    let mut action = None;
    // Drag-select copies (egui native); these buttons are the one-tap fallbacks.
    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
        if ui.small_button("copy").clicked() {
            ui.ctx().copy_text(msg.content.clone());
        }
        if msg.role == "assistant"
            && !msg.content.is_empty()
            && ui
                .small_button("speak")
                .on_hover_text("Read this answer aloud")
                .clicked()
        {
            action = Some(ChatAction::Speak(msg.content.clone()));
        }
    });
    match msg.role.as_str() {
        "user" => {
            egui::Frame::none()
                .fill(theme::bubble())
                .inner_margin(egui::Margin::same(10.0))
                .rounding(egui::Rounding::same(6.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("You").color(theme::structure()).strong().small());
                    for img in &msg.images {
                        match thumbnail(ui.ctx(), images, img) {
                            Some((tex, size)) => {
                                ui.add(egui::Image::new((tex.id(), size)).rounding(4.0));
                            }
                            None => {
                                ui.label(
                                    RichText::new(format!("[{} image]", img.mime))
                                        .color(theme::dim())
                                        .small(),
                                );
                            }
                        }
                    }
                    if !msg.content.is_empty() {
                        ui.label(RichText::new(&msg.content).color(theme::text()));
                    }
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
    action
}

/// Decode an attached image once and keep the texture for later frames.
/// Thumbnails are capped so a 12 MP photo does not become a 12 MP GPU
/// texture just to be shown in a chat bubble.
fn thumbnail(
    ctx: &egui::Context,
    cache: &mut ImageCache,
    img: &ImagePart,
) -> Option<(egui::TextureHandle, egui::Vec2)> {
    const EDGE: f32 = 260.0;
    let key = {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        img.data.hash(&mut h);
        h.finish()
    };
    if let Some((tex, size)) = cache.get(&key) {
        return Some((tex.clone(), *size));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&img.data)
        .ok()?;
    let decoded = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (w, h) = decoded.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let scale = (EDGE / w.max(h) as f32).min(1.0);
    let rgba = if scale < 1.0 {
        image::imageops::resize(
            &decoded,
            (w as f32 * scale).max(1.0) as u32,
            (h as f32 * scale).max(1.0) as u32,
            image::imageops::FilterType::Triangle,
        )
    } else {
        decoded
    };
    let size = egui::vec2(rgba.width() as f32, rgba.height() as f32);
    let color =
        egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
    let tex = ctx.load_texture(format!("attachment-{key}"), color, egui::TextureOptions::LINEAR);
    cache.insert(key, (tex.clone(), size));
    Some((tex, size))
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
