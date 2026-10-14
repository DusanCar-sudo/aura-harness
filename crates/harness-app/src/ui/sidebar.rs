//! Left sidebar: brand mark, conversation list, plugin status, settings entry.

use crate::theme;
use harness_core::config::ThemeChoice;
use harness_core::session::SessionHeader;
use egui::{Align, Layout, RichText, Ui};

/// What the user did in the sidebar this frame.
#[derive(Debug, Clone)]
pub enum SidebarAction {
    NewSession,
    Open(String),
    Delete(String),
    OpenSettings,
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
            ui.label(RichText::new("AURA").color(theme::accent()).strong().size(18.0));
            ui.label(RichText::new("HARNESS").color(theme::text()).strong().size(18.0));
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
                    if ui.selectable_label(selected, label).clicked() {
                        action = Some(SidebarAction::Open(header.id.clone()));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .small_button(RichText::new("x").color(theme::dim()))
                            .on_hover_text("Delete conversation")
                            .clicked()
                        {
                            action = Some(SidebarAction::Delete(header.id.clone()));
                        }
                    });
                });
            }
        });

    let mut pick = state.theme;
    ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
        ui.add_space(6.0);
        // Theme picker — the full set, one click from anywhere.
        ui.horizontal(|ui| {
            ui.label(RichText::new("Theme").color(theme::dim()).small());
            egui::ComboBox::from_id_salt("sidebar-theme")
                .selected_text(state.theme.label())
                .width(120.0)
                .show_ui(ui, |ui| {
                    for choice in ThemeChoice::all() {
                        ui.selectable_value(&mut pick, choice, choice.label());
                    }
                });
        });
        if ui.button("Settings").clicked() {
            action = Some(SidebarAction::OpenSettings);
        }
        ui.separator();
        let color = if state.plugin_ok {
            theme::dim()
        } else {
            theme::warn()
        };
        ui.label(RichText::new(state.plugin_status).color(color).small());
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
