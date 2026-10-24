//! Chat transcript: messages, streaming answer, reasoning, tool activity.
//!
//! The transcript is derived from the persisted message list so every item
//! keeps its place in time. A tool call renders directly under the assistant
//! turn that asked for it, and the final answer therefore sits at the bottom
//! where it can be read. The previous version rendered every message first and
//! then every tool card, which buried the answer above a wall of boxes.
//!
//! Tool rows are one line by default — status, readable command, first line of
//! result — and expand to the full output on click. A finished turn ends with
//! a "Task finished" line so there is never a doubt about whether the agent is
//! still working.

use crate::markdown;
use crate::theme;
use crate::ui::ToolCard;
use base64::Engine;
use harness_core::api::{ImagePart, Message, ToolCall};
use egui::{Align, Layout, RichText, Ui};
use std::collections::{HashMap, HashSet};
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

            {
                let rows = transcript_rows(state.messages, state.cards);
                for row in &rows {
                    let result = match row {
                        TranscriptRow::Message(msg) => {
                            render_message(ui, state.cache, state.images, msg)
                        }
                        TranscriptRow::Call {
                            call,
                            result,
                            card,
                        } => render_call(ui, call, *result, *card, state.busy),
                        TranscriptRow::Orphan(card) => {
                            render_orphan_card(ui, card, state.busy)
                        }
                    };
                    if result.is_some() {
                        action = result;
                    }
                }
            }

            render_live(ui, state);
            turn_footer(ui, state.messages, state.busy);
        });

    show_message_menu(ui, &mut action);

    action
}

/// One line of the transcript, in the order it happened.
#[derive(Debug)]
enum TranscriptRow<'a> {
    Message(&'a Message),
    Call {
        call: &'a ToolCall,
        result: Option<&'a Message>,
        card: Option<&'a ToolCard>,
    },
    Orphan(&'a ToolCard),
}

/// Interleave assistant turns, their tool calls, and the final answer by
/// walking the saved messages in order. Raw `role = "tool"` messages are
/// skipped: their text is shown inside the call row instead of duplicating it
/// as a second block. Cards whose call is not in the message list yet are
/// appended so a request that is still in flight never disappears.
fn transcript_rows<'a>(
    messages: &'a [Message],
    cards: &'a [ToolCard],
) -> Vec<TranscriptRow<'a>> {
    let results: HashMap<&str, &Message> = messages
        .iter()
        .filter(|m| m.role == "tool")
        .filter_map(|m| m.tool_call_id.as_deref().map(|id| (id, m)))
        .collect();
    let cards_by_id: HashMap<&str, &ToolCard> =
        cards.iter().map(|c| (c.call_id.as_str(), c)).collect();

    let mut rows = Vec::new();
    let mut shown_calls: HashSet<&str> = HashSet::new();
    let known_calls: HashSet<&str> = messages
        .iter()
        .flat_map(|m| m.tool_calls.iter().map(|c| c.id.as_str()))
        .collect();
    for msg in messages {
        if msg.role == "tool" {
            let paired = msg
                .tool_call_id
                .as_deref()
                .is_some_and(|id| known_calls.contains(id));
            if paired {
                continue;
            }
        }
        rows.push(TranscriptRow::Message(msg));
        if msg.role == "assistant" {
            for call in &msg.tool_calls {
                shown_calls.insert(call.id.as_str());
                rows.push(TranscriptRow::Call {
                    call,
                    result: results.get(call.id.as_str()).copied(),
                    card: cards_by_id.get(call.id.as_str()).copied(),
                });
            }
        }
    }
    for card in cards {
        if !shown_calls.contains(card.call_id.as_str()) {
            rows.push(TranscriptRow::Orphan(card));
        }
    }
    rows
}


fn welcome(ui: &mut Ui) {
    ui.add_space(ui.available_height() * 0.28);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("AURA HARNESS").color(theme::accent()).strong().size(28.0));
        theme::stripes(ui, 180.0);
        ui.add_space(6.0);
        ui.label(
            RichText::new("leanproiq.com  ·  leanroboiq.online").color(theme::dim()),
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
    match msg.role.as_str() {
        "user" => {
            // Drag-select copies (egui native); these buttons are the one-tap
            // fallbacks.
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                if ui.small_button("copy").clicked() {
                    ui.ctx().copy_text(msg.content.clone());
                }
            });
            let bubble = egui::Frame::none()
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
            note_right_click(ui, bubble.response.rect, msg);
            ui.add_space(10.0);
        }
        "assistant" => {
            let visible = display_text(&msg.content);
            let block = ui.scope(|ui| {
                // Reasoning from pure tool-call rounds is process noise; keep
                // the chain of thought only on the turn that produced an
                // actual answer.
                if !visible.is_empty() && !msg.reasoning_content.is_empty() {
                    reasoning_block(ui, &msg.reasoning_content, None);
                }
                if !visible.is_empty() {
                    // The answer must never read like another tool row: an
                    // explicit speaker label plus breathing room separates it.
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("AURA")
                                .color(theme::accent())
                                .strong()
                                .small(),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("copy").clicked() {
                                ui.ctx().copy_text(visible.to_string());
                            }
                            if ui
                                .small_button("speak")
                                .on_hover_text("Read this answer aloud")
                                .clicked()
                            {
                                action = Some(ChatAction::Speak(visible.to_string()));
                            }
                        });
                    });
                    markdown::show(ui, cache, &visible);
                    ui.add_space(4.0);
                }
            });
            note_right_click(ui, block.response.rect, msg);
            ui.add_space(8.0);
        }
        // Only reached for a result whose call was not saved (very old
        // sessions); normal results render inside their call row.
        "tool" => {
            let card = egui::Frame::none()
                .fill(theme::field())
                .inner_margin(egui::Margin::same(8.0))
                .rounding(egui::Rounding::same(4.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("tool result").color(theme::dim()).small());
                    ui.label(RichText::new(&msg.content).color(theme::dim()).monospace().small());
                });
            note_right_click(ui, card.response.rect, msg);
            ui.add_space(4.0);
        }
        _ => {}
    }
    action
}

/// One open right-click menu for the transcript.
#[derive(Clone, Default)]
struct ChatMenu {
    pos: egui::Pos2,
    content: String,
    speak: bool,
}

fn menu_id() -> egui::Id {
    egui::Id::new("chat-context-menu")
}

/// Remember that a message was right-clicked. The menu is painted by
/// [`show_message_menu`] after the scroll area, so it cannot be clipped.
fn note_right_click(ui: &Ui, rect: egui::Rect, msg: &Message) {
    let (pos, clicked) = ui.input(|i| (i.pointer.interact_pos(), i.pointer.secondary_clicked()));
    if let (true, Some(pos)) = (clicked, pos) {
        if rect.contains(pos) {
            let visible = display_text(&msg.content);
            let content = if msg.role == "assistant" {
                visible.to_string()
            } else {
                msg.content.clone()
            };
            let speak = msg.role == "assistant" && !visible.is_empty();
            ui.data_mut(|data| {
                data.insert_temp(
                    menu_id(),
                    ChatMenu {
                        pos,
                        content,
                        speak,
                    },
                )
            });
        }
    }
}

/// Draw the transcript's right-click menu: copy the whole message, and read
/// an assistant answer aloud. Text selection inside a bubble still copies
/// with Ctrl+C; this is the one-click path.
fn show_message_menu(ui: &mut Ui, action: &mut Option<ChatAction>) {
    let Some(menu) = ui.data(|data| data.get_temp::<ChatMenu>(menu_id())) else {
        return;
    };
    let mut close = false;
    let area = egui::Area::new(egui::Id::new("chat-context-popup"))
        .order(egui::Order::Foreground)
        .fixed_pos(menu.pos)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(130.0);
                if ui.button("Copy message").clicked() {
                    ui.ctx().copy_text(menu.content.clone());
                    close = true;
                }
                if menu.speak
                    && ui
                        .button("Speak")
                        .on_hover_text("Read this answer aloud")
                        .clicked()
                {
                    *action = Some(ChatAction::Speak(menu.content.clone()));
                    close = true;
                }
            });
        });
    let clicked_outside = ui.input(|i| {
        i.pointer.primary_clicked()
            && i.pointer
                .interact_pos()
                .is_some_and(|pos| !area.response.rect.contains(pos))
    });
    if close || clicked_outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui.data_mut(|data| data.remove_temp::<ChatMenu>(menu_id()));
    }
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

// ---- tool activity -------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolStatus {
    Running,
    Waiting,
    Ok,
    Warn,
    Failed,
    Stopped,
}

impl ToolStatus {
    fn glyph(self) -> &'static str {
        match self {
            ToolStatus::Running => "…",
            ToolStatus::Waiting => "?",
            ToolStatus::Ok => "✓",
            ToolStatus::Warn => "!",
            ToolStatus::Failed => "✕",
            ToolStatus::Stopped => "–",
        }
    }

    fn color(self) -> egui::Color32 {
        match self {
            ToolStatus::Running | ToolStatus::Stopped => theme::dim(),
            ToolStatus::Waiting | ToolStatus::Warn => theme::warn(),
            ToolStatus::Ok => theme::ok(),
            ToolStatus::Failed => theme::danger(),
        }
    }
}

/// A saved tool result and the live card (approval verdict + error flag) may
/// disagree about liveness; the card wins while the turn is in flight.
fn classify<'a>(
    result: Option<&'a Message>,
    card: Option<&'a ToolCard>,
    busy: bool,
) -> (ToolStatus, Option<&'a str>) {
    let output = card
        .and_then(|c| c.output.as_deref())
        .or_else(|| result.map(|m| m.content.as_str()));
    if card.is_some_and(|c| c.awaiting_approval()) {
        return (ToolStatus::Waiting, output);
    }
    let Some(text) = output else {
        return (
            if busy { ToolStatus::Running } else { ToolStatus::Stopped },
            None,
        );
    };
    let failed = card.is_some_and(|c| c.is_error) || output_looks_failed(text);
    if failed {
        // A non-zero shell exit is a warning, an explicit tool error is a
        // failure. Neither should wear the green "ok" tick.
        let hard = text.starts_with("[tool error]")
            || text.starts_with("[user rejected")
            || card.is_some_and(|c| c.is_error && !text.contains("[exit code:"));
        return (
            if hard { ToolStatus::Failed } else { ToolStatus::Warn },
            output,
        );
    }
    (ToolStatus::Ok, output)
}

fn output_looks_failed(s: &str) -> bool {
    if s.starts_with("[tool error]") {
        return true;
    }
    if let Some(pos) = s.rfind("[exit code: ") {
        let code = s[pos + "[exit code: ".len()..]
            .split(']')
            .next()
            .unwrap_or("")
            .trim();
        return code != "0";
    }
    false
}

/// Turn `shell({"command":"…"})` into `$ …`, and other calls into a one-line
/// human label. The raw JSON was the reason the transcript could not be read.
fn friendly_call(name: &str, arguments: &str) -> String {
    let parsed: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let get = |key: &str| parsed.get(key).and_then(|v| v.as_str()).unwrap_or("");
    let one = |s: &str| first_line(s, 160);
    match name {
        "shell" => format!("$ {}", one(get("command"))),
        "read_file" => format!("read {}", one(get("path"))),
        "write_file" => format!("write {}", one(get("path"))),
        "edit_file" => format!("edit {}", one(get("path"))),
        "list_files" => format!("ls {}", one(get("path"))),
        "grep" => format!("grep {} · {}", one(get("pattern")), one(get("path"))),
        "glob" => format!("glob {} · {}", one(get("pattern")), one(get("path"))),
        _ if name.starts_with("mcp_") => {
            let args = parsed.to_string();
            if args == "null" || args == "{}" {
                name.to_string()
            } else {
                format!("{name} {}", one(&args))
            }
        }
        _ => {
            let args = parsed.to_string();
            if args == "null" || args == "{}" {
                name.to_string()
            } else {
                format!("{name} · {}", one(&args))
            }
        }
    }
}

/// First line of `s`, trimmed to `max` characters on a char boundary.
fn first_line(s: &str, max: usize) -> String {
    let line = s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if line.chars().count() <= max {
        return line.to_string();
    }
    let cut: String = line.chars().take(max).collect();
    format!("{cut}…")
}

/// Some providers leak their tool-call sentinel into the text stream. DeepSeek
/// behind NVIDIA NIM, for example, writes `<｜DSML｜ calls>` as the whole
/// answer even though the calls themselves arrived structured. That marker is
/// transport noise, not prose; hide it from the rendered answer without
/// touching what is saved or sent back to the model.
fn display_text(raw: &str) -> std::borrow::Cow<'_, str> {
    if !raw.contains("DSML") {
        return std::borrow::Cow::Borrowed(raw);
    }
    let mut out = String::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.contains("DSML") || trimmed == "calls>" {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    std::borrow::Cow::Owned(out.trim().to_string())
}

/// The line under the command: what actually came back.
fn result_preview(output: &str) -> Option<String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("[stderr]"))?;
    Some(first_line(line, 150))
}

fn render_call(
    ui: &mut Ui,
    call: &ToolCall,
    result: Option<&Message>,
    card: Option<&ToolCard>,
    busy: bool,
) -> Option<ChatAction> {
    let mut action = None;
    let (status, output) = classify(result, card, busy);
    let detail = friendly_call(&call.function.name, &call.function.arguments);
    let full_detail = detail.clone();
    let default_open = matches!(status, ToolStatus::Failed | ToolStatus::Warn);

    let header_id = ui.make_persistent_id(("tool-call", call.id.as_str()));
    let state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        header_id,
        default_open,
    );

    let mut header_rect = egui::Rect::NOTHING;
    let header = state.show_header(ui, |ui| {
        let color = status.color();
        if status == ToolStatus::Running {
            ui.add(egui::Spinner::new().size(11.0).color(color));
        } else {
            ui.label(RichText::new(status.glyph()).color(color).monospace().small());
        }
        ui.vertical(|ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(&detail)
                        .color(theme::text())
                        .monospace()
                        .small(),
                )
                .truncate(),
            );
            if status == ToolStatus::Running {
                ui.label(RichText::new("running…").color(theme::dim()).small().italics());
            } else if let Some(text) = output {
                if let Some(preview) = result_preview(text) {
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("↳ {preview}")).color(theme::dim()).small(),
                        )
                        .truncate(),
                    );
                }
            }
        });
        header_rect = ui.min_rect();
    });

    // A command that needs a verdict can never be collapsed away: the buttons
    // are the only way the turn continues.
    if status == ToolStatus::Waiting {
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            ui.label(
                RichText::new("This runs a local command. Approve?")
                    .color(theme::warn())
                    .small(),
            );
            if ui
                .add(
                    egui::Button::new(RichText::new("Approve").color(theme::on_accent()).strong())
                        .fill(theme::accent()),
                )
                .clicked()
            {
                action = Some(ChatAction::Approve(call.id.clone()));
            }
            if ui
                .add(
                    egui::Button::new(RichText::new("Reject").color(theme::text()).strong())
                        .fill(theme::danger()),
                )
                .clicked()
            {
                action = Some(ChatAction::Reject(call.id.clone()));
            }
        });
        ui.add_space(4.0);
        return action;
    }

    header.body(|ui| {
        // Displaying a 50 MB command output would stall the UI; the model
        // still has the full text in history.
        let text = match output {
            Some(t) if !t.trim().is_empty() => t,
            Some(_) => "[no output]",
            None => "[no result recorded]",
        };
        let text = harness_core::agent::truncate_for_display(text, 12_000);
        ui.label(
            RichText::new(format!(
                "{} line{}",
                text.lines().count(),
                if text.lines().count() == 1 { "" } else { "s" }
            ))
            .color(theme::dim())
            .small(),
        );
        egui::Frame::none()
            .fill(theme::field())
            .inner_margin(egui::Margin::same(8.0))
            .rounding(egui::Rounding::same(4.0))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("tool-output", call.id.as_str()))
                    .max_height(220.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&text).color(theme::text()).monospace().small(),
                            )
                            .wrap(),
                        );
                    });
            });
    });

    // The triangle alone is a tiny target. Make the whole row toggle.
    let row_click = ui
        .interact(header_rect, header_id.with("row-click"), egui::Sense::click())
        .on_hover_text(full_detail.clone())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if row_click.clicked() {
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            header_id,
            default_open,
        );
        state.toggle(ui);
        state.store(ui.ctx());
    }
    ui.add_space(2.0);

    action
}

/// Fallback for a card whose assistant message is not in the transcript yet.
fn render_orphan_card(ui: &mut Ui, card: &ToolCard, busy: bool) -> Option<ChatAction> {
    let mut action = None;
    let (status, output) = classify(None, Some(card), busy);
    let label = if card.summary.trim().is_empty() {
        card.name.as_str()
    } else {
        card.summary.as_str()
    };
    if status == ToolStatus::Waiting {
        ui.horizontal(|ui| {
            ui.label(RichText::new(status.glyph()).color(status.color()).monospace().small());
            ui.label(RichText::new(label).monospace().small().color(theme::text()));
        });
        ui.horizontal(|ui| {
            ui.add_space(18.0);
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
        return action;
    }
    ui.horizontal(|ui| {
        if status == ToolStatus::Running {
            ui.add(egui::Spinner::new().size(11.0).color(status.color()));
        } else {
            ui.label(RichText::new(status.glyph()).color(status.color()).monospace().small());
        }
        ui.add(
            egui::Label::new(
                RichText::new(label)
                    .color(theme::text())
                    .monospace()
                    .small(),
            )
            .truncate(),
        );
    });
    if let Some(text) = output {
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            ui.add(
                egui::Label::new(
                    RichText::new(first_line(text, 150)).color(theme::dim()).small(),
                )
                .truncate(),
            );
        });
    }
    ui.add_space(2.0);
    action
}

fn render_live(ui: &mut Ui, state: &mut ChatState<'_>) {
    if !state.live.reasoning.is_empty() {
        reasoning_block(ui, &state.live.reasoning, state.live.reasoning_secs);
    }
    let visible = display_text(&state.live.content);
    if !visible.is_empty() {
        ui.label(RichText::new("AURA").color(theme::accent()).strong().small());
        markdown::show(ui, state.cache, &visible);
    }
    if state.busy && visible.is_empty() {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(12.0));
            ui.label(RichText::new("thinking…").color(theme::dim()).italics());
        });
    }
    ui.with_layout(Layout::right_to_left(Align::TOP), |_ui| {});
}

/// "Task finished · 12 tool calls · 1m 24s" under a completed answer.
///
/// Tool activity above this line is history; nothing is still running and the
/// answer directly above is the final one. Reloading an old chat shows the
/// same marker because it is computed from the saved messages.
fn turn_footer(ui: &mut Ui, messages: &[Message], busy: bool) {
    if busy {
        return;
    }
    let Some(stats) = last_turn_stats(messages) else {
        return;
    };
    ui.add_space(10.0);
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(RichText::new("✓").color(theme::ok()).strong());
        ui.label(RichText::new("Task finished").color(theme::ok()).strong().small());
        let mut meta = format!(
            "· {} tool call{}",
            stats.calls,
            if stats.calls == 1 { "" } else { "s" }
        );
        if let Some(secs) = stats.secs {
            meta.push_str(&format!(" · {}", human_duration(secs)));
        }
        ui.label(RichText::new(meta).color(theme::dim()).small());
    });
}

#[derive(Debug, PartialEq)]
struct TurnStats {
    calls: usize,
    secs: Option<f64>,
}

/// Stats for the last user turn, but only when it ended on an assistant turn
/// (with or without text — a model that stops silently still gets a marker).
fn last_turn_stats(messages: &[Message]) -> Option<TurnStats> {
    let last = messages.last()?;
    if last.role != "assistant" {
        return None;
    }
    let user_idx = messages.iter().rposition(|m| m.role == "user")?;
    let calls = messages[user_idx + 1..]
        .iter()
        .map(|m| m.tool_calls.len())
        .sum();
    let secs = match (messages[user_idx].ts, last.ts) {
        (Some(start), Some(end)) => Some(
            (end - start).num_milliseconds().max(0) as f64 / 1000.0,
        ),
        _ => None,
    };
    Some(TurnStats { calls, secs })
}

fn human_duration(secs: f64) -> String {
    let total = secs.round() as u64;
    if total < 60 {
        format!("{total}s")
    } else if total < 3600 {
        format!("{}m {:02}s", total / 60, total % 60)
    } else {
        format!("{}h {:02}m", total / 3600, (total % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_core::agent::Approval;

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

    #[test]
    fn shell_calls_read_as_commands() {
        assert_eq!(
            friendly_call("shell", r#"{"command":"ls -la"}"#),
            "$ ls -la"
        );
        assert_eq!(
            friendly_call("edit_file", r#"{"path":"src/main.rs","old_string":"a"}"#),
            "edit src/main.rs"
        );
        // Multi-line heredocs keep only their first line in the collapsed row.
        let heredoc = friendly_call("shell", r#"{"command":"cd /tmp\npython3 - <<'PY'\nprint(1)\nPY"}"#);
        assert!(heredoc.starts_with("$ cd /tmp"), "{heredoc}");
        assert!(!heredoc.contains("print(1)"), "{heredoc}");
    }

    #[test]
    fn provider_markers_never_reach_the_answer() {
        assert_eq!(display_text("\n\n<｜DSML｜ calls>\n").as_ref(), "");
        assert_eq!(
            display_text("Done.\n\n<｜DSML｜ calls>\n").as_ref(),
            "Done."
        );
        assert_eq!(display_text("plain answer").as_ref(), "plain answer");
    }

    #[test]
    fn non_zero_exit_is_not_green() {
        assert!(output_looks_failed("ok\n[exit code: -1]"));
        assert!(output_looks_failed("[tool error]\nboom"));
        assert!(!output_looks_failed("fine\n[exit code: 0]"));
        assert!(!output_looks_failed("plain output"));
    }

    #[test]
    fn turn_stats_count_only_the_last_turn() {
        let mut user = Message::user("build it");
        user.ts = Some(chrono::Utc::now());
        let mut call = ToolCall::new("c1", 0, "shell", r#"{"command":"ls"}"#);
        call.id = "c1".into();
        let mut assistant = Message::user("");
        assistant.role = "assistant".into();
        assistant.tool_calls = vec![call];
        let mut answer = Message::user("done");
        answer.role = "assistant".into();
        answer.ts = Some(user.ts.unwrap() + chrono::Duration::seconds(65));
        let messages = vec![user, assistant, answer];
        let stats = last_turn_stats(&messages).unwrap();
        assert_eq!(stats.calls, 1);
        assert_eq!(stats.secs, Some(65.0));
        assert_eq!(human_duration(65.0), "1m 05s");
    }

    #[test]
    fn an_unanswered_turn_has_no_footer() {
        let user = Message::user("hi");
        assert!(last_turn_stats(&[user]).is_none());
    }

    #[test]
    fn the_answer_renders_last_and_tool_results_belong_to_their_call() {
        let user = Message::user("build it");
        let mut assistant = Message::user("");
        assistant.role = "assistant".into();
        assistant.tool_calls = vec![ToolCall::new(
            "c1",
            0,
            "shell",
            r#"{"command":"ls"}"#,
        )];
        let tool = Message::tool_result("c1", "file.txt");
        let mut answer = Message::user("done");
        answer.role = "assistant".into();
        let messages = vec![user, assistant, tool, answer];

        let rows = transcript_rows(&messages, &[]);
        // user, assistant, the call, the answer. The raw role="tool" message
        // does not become a row of its own.
        assert_eq!(rows.len(), 4);
        match &rows[2] {
            TranscriptRow::Call {
                call,
                result,
                card,
            } => {
                assert_eq!(call.id, "c1");
                assert_eq!(result.map(|m| m.content.as_str()), Some("file.txt"));
                assert!(card.is_none());
            }
            other => panic!("expected a tool call row, got {other:?}"),
        }
        assert!(
            matches!(&rows[3], TranscriptRow::Message(m) if m.content == "done"),
            "the final answer must be the last row"
        );
    }

    #[test]
    fn a_live_card_without_a_message_still_renders() {
        let card = ToolCard::new("c9".into(), "shell".into(), "$ ls".into(), false);
        let rows = transcript_rows(&[], std::slice::from_ref(&card));
        assert!(matches!(&rows[0], TranscriptRow::Orphan(c) if c.call_id == "c9"));
    }

    /// Render a real transcript through egui's layout pass (no GPU) and look
    /// at where the text was actually painted. The final answer and the
    /// finished marker must sit below every tool row.
    #[test]
    fn the_answer_is_painted_below_the_tool_rows() {
        fn call(id: &str, command: &str) -> Message {
            let mut m = Message::user("");
            m.role = "assistant".into();
            m.tool_calls = vec![ToolCall::new(
                id,
                0,
                "shell",
                format!(r#"{{"command":"{command}"}}"#),
            )];
            m
        }

        let messages = vec![
            Message::user("build it"),
            call("c1", "echo one"),
            Message::tool_result("c1", "one.txt"),
            call("c2", "echo two"),
            Message::tool_result("c2", "two.txt"),
            {
                let mut m = Message::user("FINAL ANSWER: all done");
                m.role = "assistant".into();
                m.ts = Some(chrono::Utc::now());
                m
            },
        ];

        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 2400.0),
            )),
            ..Default::default()
        };
        let mut cache = egui_commonmark::CommonMarkCache::default();
        let mut images = ImageCache::new();
        let live = LiveTurn::default();
        // First pass primes sizing; the second pass has settled layout.
        let _ = ctx.run(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut state = ChatState {
                    messages: &messages,
                    live: &live,
                    cards: &[],
                    busy: false,
                    cache: &mut cache,
                    images: &mut images,
                };
                show(ui, &mut state);
            });
        });
        let output = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut state = ChatState {
                    messages: &messages,
                    live: &live,
                    cards: &[],
                    busy: false,
                    cache: &mut cache,
                    images: &mut images,
                };
                show(ui, &mut state);
            });
        });

        let mut texts: Vec<(String, f32)> = Vec::new();
        fn collect(shape: &egui::Shape, out: &mut Vec<(String, f32)>) {
            match shape {
                egui::Shape::Text(t) => out.push((t.galley.text().to_string(), t.pos.y)),
                egui::Shape::Vec(v) => {
                    for s in v {
                        collect(s, out);
                    }
                }
                _ => {}
            }
        }
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut texts);
        }

        let y_of = |needle: &str| {
            texts
                .iter()
                .find(|(t, _)| t.contains(needle))
                .map(|(_, y)| *y)
                .unwrap_or_else(|| panic!("{needle:?} was not painted: {texts:?}"))
        };
        let answer_y = y_of("FINAL ANSWER");
        let footer_y = y_of("Task finished");
        assert!(
            y_of("$ echo one") < answer_y,
            "first tool row must precede the answer"
        );
        assert!(
            y_of("$ echo two") < answer_y,
            "second tool row must precede the answer"
        );
        assert!(
            footer_y > answer_y,
            "the finished marker must come after the answer, not before it"
        );
    }
}
