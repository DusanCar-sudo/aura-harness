//! Left sidebar: brand mark, conversation list, plugin status, settings entry.

use crate::theme;
use egui::{Align, Layout, RichText, Ui};
use harness_core::config::ThemeChoice;
use harness_core::session::SessionHeader;

/// What the user did in the sidebar this frame.
#[derive(Debug, Clone)]
pub enum SidebarAction {
    NewSession,
    Open(String),
    Delete(String),
    RenameRequest(String),
    OpenSettings,
    OpenPlugins,
    /// Switch theme from the sidebar picker.
    Theme(ThemeChoice),
}

pub struct SidebarState<'a> {
    pub sessions: &'a [SessionHeader],
    pub current_id: Option<&'a str>,
    /// Human-readable plugin/tool status, shown as a footer.
    pub plugin_status: &'a str,
    pub plugin_ok: bool,
    /// The Aura mark, loaded from the real artwork.
    pub mark: Option<&'a egui::TextureHandle>,
    pub theme: ThemeChoice,
}

pub fn show(ui: &mut Ui, state: &SidebarState<'_>) -> Option<SidebarAction> {
    let mut action = None;

    // The Aura mark, then the wordmark beside it.
    ui.horizontal(|ui| {
        let size = egui::vec2(40.0, 40.0);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        match state.mark {
            Some(tex) => {
                ui.painter().image(
                    tex.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            None => {
                // Until the texture loads, fall back to the stripe band.
                theme::stripes(ui, size.x);
            }
        }
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.label(
                RichText::new("AURA")
                    .color(theme::accent())
                    .strong()
                    .size(24.0),
            );
            ui.label(RichText::new("harness").color(theme::dim()).size(11.0));
        });
    });
    ui.add_space(2.0);
    theme::stripes(ui, ui.available_width().min(140.0));
    ui.add_space(4.0);

    if ui.button("+  New chat").clicked() {
        action = Some(SidebarAction::NewSession);
    }
    ui.add_space(6.0);
    ui.separator();

    // Pin the footer BEFORE the scroll area so a long session list can never
    // overlap Settings, Theme, or the plugin status strip.
    let mut pick = state.theme;
    let footer_frame = egui::Frame::none()
        .fill(theme::palette_for(state.theme).panel)
        .inner_margin(egui::Margin {
            left: 0.0,
            right: 0.0,
            top: 4.0,
            bottom: 8.0,
        });
    egui::TopBottomPanel::bottom("sidebar-footer")
        .show_separator_line(false)
        .frame(footer_frame)
        .show_inside(ui, |ui| {
            ui.separator();
            let color = if state.plugin_ok {
                theme::dim()
            } else {
                theme::warn()
            };
            if ui
                .add(
                    egui::Button::new(RichText::new(state.plugin_status).color(color).small())
                        .frame(false),
                )
                .on_hover_text("Open Plugins & MCP servers")
                .clicked()
            {
                action = Some(SidebarAction::OpenPlugins);
            }
            ui.add_space(4.0);
            ui.spacing_mut().interact_size.y = 26.0;
            let half = ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).max(76.0);
            ui.horizontal(|ui| {
                let settings_btn = egui::Button::new(RichText::new("Settings").size(12.5))
                    .fill(theme::card())
                    .min_size(egui::vec2(half, 26.0));
                if ui
                    .add(settings_btn)
                    .on_hover_text("Settings · providers, keys, model, agents, plugins")
                    .clicked()
                {
                    action = Some(SidebarAction::OpenSettings);
                }
                ui.style_mut().visuals.widgets.inactive.bg_fill = theme::card();
                ui.style_mut().visuals.widgets.inactive.weak_bg_fill = theme::card();
                let current_label = match state.theme {
                    ThemeChoice::Nature => "Nature",
                    ThemeChoice::ChatGpt => "ChatGPT",
                    _ => state.theme.label(),
                };
                let palette = egui::ComboBox::from_id_salt("sidebar-theme")
                    .selected_text(RichText::new(format!("Theme: {current_label}")).size(12.0))
                    .width(half)
                    .show_ui(ui, |ui| {
                        ui.small(RichText::new("Dark themes").color(theme::dim()));
                        for choice in ThemeChoice::all().into_iter().filter(|t| t.is_dark()) {
                            let pal = theme::palette_for(choice);
                            ui.horizontal(|ui| {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(10.0, 10.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(rect, 2.0, pal.accent);
                                ui.selectable_value(&mut pick, choice, choice.label());
                            });
                        }
                        ui.separator();
                        ui.small(RichText::new("Light themes").color(theme::dim()));
                        for choice in ThemeChoice::all().into_iter().filter(|t| !t.is_dark()) {
                            let pal = theme::palette_for(choice);
                            ui.horizontal(|ui| {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(10.0, 10.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(rect, 2.0, pal.accent);
                                ui.selectable_value(&mut pick, choice, choice.label());
                            });
                        }
                    });
                palette
                    .response
                    .on_hover_text(format!("Theme · {}", state.theme.label()));
            });
        });

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for header in state.sessions {
                let selected = state.current_id == Some(header.id.as_str());
                let label = RichText::new(short_title(&header.title)).color(if selected {
                    theme::accent()
                } else {
                    theme::text()
                });

                ui.horizontal(|ui| {
                    let lbl = ui
                        .selectable_label(selected, label)
                        .on_hover_text("Click to open · double-click or ren to rename");
                    // Double-click wins over the single-click open.
                    if lbl.double_clicked() {
                        action = Some(SidebarAction::RenameRequest(header.id.clone()));
                    } else if lbl.clicked() {
                        action = Some(SidebarAction::Open(header.id.clone()));
                    }
                    if lbl
                        .context_menu(|ui| {
                            if ui.button("Rename").clicked() {
                                action = Some(SidebarAction::RenameRequest(header.id.clone()));
                                ui.close_menu();
                            }
                            if ui.button("Delete").clicked() {
                                action = Some(SidebarAction::Delete(header.id.clone()));
                                ui.close_menu();
                            }
                        })
                        .is_some()
                    {
                        // menu opened; nothing else to do
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .small_button(RichText::new("x").color(theme::dim()))
                            .on_hover_text("Delete conversation")
                            .clicked()
                        {
                            action = Some(SidebarAction::Delete(header.id.clone()));
                        }
                        if ui
                            .small_button(RichText::new("ren").color(theme::dim()))
                            .on_hover_text("Rename conversation")
                            .clicked()
                        {
                            action = Some(SidebarAction::RenameRequest(header.id.clone()));
                        }
                    });
                });
            }
        });

    if pick != state.theme {
        action = Some(SidebarAction::Theme(pick));
    }

    action
}

/// Keep sidebar rows to one readable line without losing the tail.
fn short_title(title: &str) -> String {
    const MAX: usize = 26;
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return "(untitled)".to_string();
    }
    let mut out: String = trimmed.chars().take(MAX).collect();
    if trimmed.chars().count() > MAX {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortens_long_titles() {
        assert_eq!(short_title("hi"), "hi");
        assert_eq!(short_title(""), "(untitled)");
        let long = "a".repeat(40);
        let out = short_title(&long);
        assert!(out.ends_with('…'));
        assert_eq!(out.chars().count(), 27);
    }
}
