//! Settings sheet: API key, model defaults, system prompt, plugin inventory.
//!
//! The panel only edits state and reports an action; `main.rs` owns the disk
//! writes (credentials 0600, config.toml, plugins.toml).

use crate::theme;
use harness_core::config::{Config, EffortChoice, ModelChoice, Plugins, ThemeChoice};
use egui::{RichText, Ui};

/// An MCP server found on this machine that is not yet configured here.
#[derive(Debug, Clone)]
pub struct AvailableServer {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

impl AvailableServer {
    /// One-line description for the picker row.
    pub fn summary(&self) -> String {
        self.launch_line()
    }

    /// The launch line handed back to the app when this server is added.
    pub fn launch_line(&self) -> String {
        std::iter::once(&self.command)
            .chain(self.args.iter())
            .map(|token| {
                if token.contains(' ') {
                    format!("\"{token}\"")
                } else {
                    token.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// What the user asked settings to do this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsAction {
    SaveKey(String),
    SaveConfig,
    SavePlugins,
    Reconnect,
    /// Open the searchable plugin browser.
    OpenBrowser,
    RemoveServer(String),
    Close,
}

pub struct SettingsState<'a> {
    pub config: &'a mut Config,
    pub plugins: &'a mut Plugins,
    pub key_input: &'a mut String,
    pub key_present: bool,
    /// One line per configured MCP server: "name — connected|disabled|failed".
    pub mcp_status: &'a [String],
    /// One line per configured command tool: "name — command".
    pub cmd_tools: &'a [String],
    pub tool_count: usize,
}

pub fn show(ui: &mut Ui, state: &mut SettingsState<'_>) -> Option<SettingsAction> {
    let mut action = None;

    ui.horizontal(|ui| {
        ui.label(RichText::new("Settings").color(theme::accent()).strong().size(18.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Close").clicked() {
                action = Some(SettingsAction::Close);
            }
        });
    });
    theme::stripes(ui, ui.available_width().min(180.0));
    ui.add_space(6.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // ---- credentials ------------------------------------------------
            ui.label(RichText::new("DeepSeek API key").color(theme::text()).strong());
            ui.label(
                RichText::new(if state.key_present {
                    "A key is stored at ~/.config/aura-harness/credentials (0600)."
                } else {
                    "No key yet — paste one from platform.deepseek.com."
                })
                .color(theme::dim())
                .small(),
            );
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(state.key_input)
                        .password(true)
                        .hint_text("sk-…")
                        .desired_width(360.0),
                );
                if ui.button("Save key").clicked() {
                    action = Some(SettingsAction::SaveKey(state.key_input.clone()));
                }
            });
            ui.add_space(10.0);

            // ---- defaults ---------------------------------------------------
            ui.separator();
            ui.label(RichText::new("Defaults").color(theme::text()).strong());
            ui.horizontal(|ui| {
                ui.label("Theme");
                egui::ComboBox::from_id_salt("cfg-theme")
                    .selected_text(state.config.theme.label())
                    .show_ui(ui, |ui| {
                        for choice in ThemeChoice::all() {
                            ui.selectable_value(&mut state.config.theme, choice, choice.label());
                        }
                    });
                // A live swatch of the theme's own colours.
                let p = crate::theme::palette_for(state.config.theme);
                for color in [p.bg, p.panel, p.accent, p.stripes[1], p.text] {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, egui::Rounding::same(3.0), color);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Model");
                egui::ComboBox::from_id_salt("cfg-model")
                    .selected_text(model_label(state.config.model))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.config.model,
                            ModelChoice::Flash,
                            "deepseek-flash",
                        );
                        ui.selectable_value(
                            &mut state.config.model,
                            ModelChoice::V4Pro,
                            "deepseek-v4-pro",
                        );
                    });
                ui.label("Effort");
                egui::ComboBox::from_id_salt("cfg-effort")
                    .selected_text(effort_label(state.config.reasoning_effort))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.config.reasoning_effort,
                            EffortChoice::None,
                            "thinking off",
                        );
                        ui.selectable_value(
                            &mut state.config.reasoning_effort,
                            EffortChoice::Low,
                            "low",
                        );
                        ui.selectable_value(
                            &mut state.config.reasoning_effort,
                            EffortChoice::High,
                            "high",
                        );
                        ui.selectable_value(
                            &mut state.config.reasoning_effort,
                            EffortChoice::Max,
                            "max",
                        );
                    });
            });

            ui.label("System prompt");
            ui.add(
                egui::TextEdit::multiline(&mut state.config.system_prompt)
                    .hint_text("Optional instructions prepended to every conversation")
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            ui.checkbox(
                &mut state.config.agentmem_mirror,
                "Mirror session summaries to the shared AGENTMEM store",
            );
            if ui.button("Save defaults").clicked() {
                action = Some(SettingsAction::SaveConfig);
            }
            ui.add_space(10.0);

            // ---- plugins ----------------------------------------------------
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Plugins").color(theme::text()).strong());
                ui.label(
                    RichText::new(format!("{} tools loaded", state.tool_count))
                        .color(theme::dim())
                        .small(),
                );
            });

            ui.label(RichText::new("MCP servers").color(theme::dim()).small());
            if state.mcp_status.is_empty() {
                ui.label(RichText::new("none configured").color(theme::dim()).italics());
            }
            for line in state.mcp_status {
                let color = if line.contains("failed") {
                    theme::danger()
                } else if line.contains("disabled") {
                    theme::dim()
                } else {
                    theme::ok()
                };
                ui.label(RichText::new(line).color(color).monospace().small());
            }

            ui.add_space(4.0);
            ui.label(RichText::new("Command tools").color(theme::dim()).small());
            if state.cmd_tools.is_empty() {
                ui.label(RichText::new("none configured").color(theme::dim()).italics());
            }
            for line in state.cmd_tools {
                ui.label(RichText::new(line).color(theme::text()).monospace().small());
            }

            // ---- get plugins ------------------------------------------------
            ui.add_space(8.0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Get plugins").color(theme::text()).strong());
                if ui
                    .button("Browse…")
                    .on_hover_text("Search a list of connectable MCP servers")
                    .clicked()
                {
                    action = Some(SettingsAction::OpenBrowser);
                }
            });
            ui.label(
                RichText::new(
                    "Search a curated catalogue, servers found on this machine, or paste a launch command.",
                )
                .color(theme::dim())
                .small(),
            );

            // ---- configured -------------------------------------------------
            ui.add_space(8.0);
            ui.separator();
            ui.label(RichText::new("Configured plugins").color(theme::text()).strong());
            if state.plugins.mcp.is_empty() && state.plugins.cmd.is_empty() {
                ui.label(RichText::new("none yet").color(theme::dim()).italics());
            }

            let mut remove: Option<String> = None;
            for (name, server) in state.plugins.mcp.iter_mut() {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut server.enabled, "");
                    ui.label(RichText::new(name).color(theme::text()).monospace().small());
                    ui.label(
                        RichText::new(format!("{} {}", server.command, server.args.join(" ")))
                            .color(theme::dim())
                            .small(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Remove").clicked() {
                            remove = Some(name.clone());
                        }
                    });
                });
            }
            for (name, tool) in state.plugins.cmd.iter_mut() {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut tool.enabled, "");
                    ui.label(RichText::new(name).color(theme::text()).monospace().small());
                    ui.label(RichText::new(&tool.command).color(theme::dim()).small());
                });
            }
            if let Some(name) = remove {
                action = Some(SettingsAction::RemoveServer(name));
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .button("Save")
                    .on_hover_text("Write these plugins to ~/.config/aura-harness/plugins.toml")
                    .clicked()
                {
                    action = Some(SettingsAction::SavePlugins);
                }
                if ui
                    .button("Reconnect")
                    .on_hover_text("Restart MCP servers and reload tools")
                    .clicked()
                {
                    action = Some(SettingsAction::Reconnect);
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
        EffortChoice::Low => "low",
        EffortChoice::High => "high",
        EffortChoice::Max => "max",
    }
}

/// One-line MCP status used by both settings and the sidebar footer.
pub fn mcp_line(name: &str, enabled: bool, connected: bool) -> String {
    let state = if !enabled {
        "disabled"
    } else if connected {
        "connected"
    } else {
        "failed"
    };
    format!("{name} — {state}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_lines_report_state() {
        assert_eq!(mcp_line("mnemai", true, true), "mnemai — connected");
        assert_eq!(mcp_line("mnemai", true, false), "mnemai — failed");
        assert_eq!(mcp_line("mnemai", false, false), "mnemai — disabled");
    }

    #[test]
    fn labels_are_stable() {
        assert_eq!(model_label(ModelChoice::V4Pro), "deepseek-v4-pro");
        assert_eq!(effort_label(EffortChoice::Max), "max");
    }
}
