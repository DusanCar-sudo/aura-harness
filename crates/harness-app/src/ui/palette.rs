//! The `/` command palette.
//!
//! The composer's own text is the filter: type `/` and the list appears, keep
//! typing to narrow it, ↑/↓ to move, Enter to run, Tab to complete, Esc to
//! dismiss. Every entry comes from [`crate::commands::COMMANDS`], which is the
//! palette Aura Code serves to its own clients — so the harness cannot fall
//! behind by maintaining a second list.

use egui::{Key, RichText, Ui};

use crate::commands;
use crate::theme;

#[derive(Debug, Default)]
pub struct PaletteState {
    pub open: bool,
    pub selected: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteAction {
    /// Run this command line now.
    Run(String),
    /// Put this text in the composer and let the user finish it.
    Fill(String),
}

/// Draw the palette above the composer. Returns the chosen action, if any.
pub fn show(
    ui: &mut Ui,
    state: &mut PaletteState,
    draft: &mut String,
) -> Option<PaletteAction> {
    if !state.open {
        return None;
    }

    let query = draft.trim_start_matches(['/', ':']).to_string();
    let hits = commands::filter(&query);
    if state.selected >= hits.len() {
        state.selected = hits.len().saturating_sub(1);
    }

    let mut action = None;

    // Keys are consumed before the text edit sees them, so ↑/↓ move the
    // selection instead of the caret and Enter runs instead of sending.
    let (down, up, enter, tab, esc) = ui.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
            i.consume_key(egui::Modifiers::NONE, Key::Enter),
            i.consume_key(egui::Modifiers::NONE, Key::Tab),
            i.consume_key(egui::Modifiers::NONE, Key::Escape),
        )
    });

    if down {
        state.selected = (state.selected + 1).min(hits.len().saturating_sub(1));
    }
    if up {
        state.selected = state.selected.saturating_sub(1);
    }
    if esc {
        state.open = false;
        return None;
    }
    if enter || tab {
        if let Some(spec) = hits.get(state.selected) {
            if enter && !spec.id.contains(' ') {
                action = Some(PaletteAction::Run(spec.id.to_string()));
            } else {
                action = Some(PaletteAction::Fill(format!("{} ", spec.id)));
            }
        } else if enter {
            // No match: the typed line may still be a command (or a path).
            let typed = draft.trim().to_string();
            if !typed.is_empty() {
                action = Some(PaletteAction::Run(typed));
            }
        }
    }

    egui::Frame::none()
        .fill(theme::bubble())
        .stroke(egui::Stroke::new(1.0_f32, theme::accent().gamma_multiply(0.5)))
        .rounding(egui::Rounding::same(6.0))
        .inner_margin(egui::Margin::same(8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Commands")
                        .strong()
                        .color(theme::accent())
                        .small(),
                );
                ui.label(
                    RichText::new("↑↓ move · Enter run · Tab complete · Esc close")
                        .color(theme::dim())
                        .small(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} match(es)", hits.len()))
                            .color(theme::dim())
                            .small(),
                    );
                });
            });
            ui.add_space(2.0);

            if hits.is_empty() {
                ui.label(
                    RichText::new("Nothing matches. Enter sends the line as typed.")
                        .color(theme::dim())
                        .small(),
                );
            }

            egui::ScrollArea::vertical()
                .max_height(220.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (i, spec) in hits.iter().take(40).enumerate() {
                        let selected = i == state.selected;
                        let row = crate::ui::row_button(
                            ui,
                            24.0,
                            selected,
                            &format!("{}   {}", spec.id, spec.label),
                            spec.category,
                        );
                        if row.hovered() {
                            state.selected = i;
                        }
                        if row.clicked() {
                            if spec.id.contains(' ') {
                                action = Some(PaletteAction::Fill(format!("{} ", spec.id)));
                            } else {
                                action = Some(PaletteAction::Run(spec.id.to_string()));
                            }
                        }
                        if selected {
                            ui.label(
                                RichText::new(format!("      {}", spec.description))
                                    .color(theme::dim())
                                    .small(),
                            );
                        }
                    }
                });
        });

    action.map(|a| match a {
        PaletteAction::Fill(text) => {
            draft.clear();
            draft.push_str(&text);
            state.open = false;
            PaletteAction::Fill(text)
        }
        other => other,
    })
}
