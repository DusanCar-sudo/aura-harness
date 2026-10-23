//! Settings sheet: API key, model defaults, system prompt, plugin inventory.
//!
//! The panel only edits state and reports an action; `main.rs` owns the disk
//! writes (credentials 0600, config.toml, plugins.toml).

use crate::theme;
use harness_core::config::{Config, EffortChoice, Plugins, ThemeChoice};
use harness_core::models;
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
    /// Open the full provider → model factory.
    OpenModelFactory,
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
            // ---- built-in tools ---------------------------------------------
            ui.label(RichText::new("Built-in tools").color(theme::text()).strong());
            ui.small("Always on. Mutating tools ask for your approval before running.");
            for tool in harness_core::builtins::BuiltinTool::all() {
                let def = tool.def();
                let policy = if tool.needs_approval() {
                    "approval"
                } else {
                    "auto"
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(def.name.as_str()).monospace().strong());
                    ui.small(def.description);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.small(policy);
                    });
                });
            }
            ui.add_space(6.0);
            ui.checkbox(
                &mut state.config.auto_approve,
                "Auto-approve tool calls (full access)",
            );
            ui.small(
                "On: shell, write_file, edit_file and command tools run without asking.                  Off: every mutating tool shows an approve/reject card first.",
            );
            ui.add_space(8.0);
            ui.separator();

            // ---- credentials ------------------------------------------------
            ui.label(RichText::new("API keys").color(theme::text()).strong());
            ui.label(
                RichText::new(
                    "The factory reads ~/.aura/keys.json and ~/.aura/.env — the same keys \
                     Aura Code uses. The field below is a DeepSeek fallback stored in \
                     ~/.config/aura-harness/credentials (0600).",
                )
                .color(theme::dim())
                .small(),
            );
            ui.label(
                RichText::new(if state.key_present {
                    "A fallback key is stored."
                } else {
                    "No fallback key stored. Keys from ~/.aura are enough for most providers."
                })
                .color(theme::dim())
                .small(),
            );
            ui.horizontal(|ui| {
                crate::ui::text_edit(
                    ui,
                    state.key_input,
                    crate::ui::TextEditSpec::singleline()
                        .password(true)
                        .hint("sk-… (DeepSeek fallback)")
                        .width(360.0),
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
                        // Nine palettes; hover one for what it promises.
                        for choice in ThemeChoice::all() {
                            ui.selectable_value(&mut state.config.theme, choice, choice.label())
                                .on_hover_text(choice.hint());
                        }
                    });
                ui.small(if state.config.theme.is_dark() {
                    "dark"
                } else {
                    "light"
                })
                .on_hover_text("whether the palette is drawn for a dark or light surface");
                // A live swatch of the theme's own colours.
                let p = crate::theme::palette_for(state.config.theme);
                for color in [p.bg, p.panel, p.accent, p.stripes[1], p.text] {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, egui::Rounding::same(3.0), color);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Background image");
                crate::ui::text_edit(
                    ui,
                    &mut state.config.background_image,
                    crate::ui::TextEditSpec::singleline()
                        .width(220.0)
                        .hint("path to png/jpg/webp"),
                );
                if ui.button("Pick...").clicked() {
                    // A portal hiccup must not take the harness down.
                    let picked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        rfd::FileDialog::new()
                            .add_filter("image", &["png", "jpg", "jpeg", "webp", "bmp"])
                            .pick_file()
                    }))
                    .unwrap_or(None);
                    if let Some(path) = picked {
                        state.config.background_image = path.display().to_string();
                        // Save at once: a background that needs a restart to
                        // appear reads as broken.
                        let _ = state.config.save();
                    }
                }
                if ui.button("Clear").clicked() {
                    state.config.background_image.clear();
                    let _ = state.config.save();
                }
            });
            let bg_set = !state.config.background_image.trim().is_empty();
            ui.add_enabled(
                bg_set,
                egui::Slider::new(&mut state.config.background_opacity, 0.0..=1.0)
                    .text("background strength"),
            );
            if bg_set {
                ui.small(format!("active: {}", state.config.background_image));
                if ui.link("clear it").clicked() {
                    state.config.background_image.clear();
                    let _ = state.config.save();
                }
            } else {
                ui.small("No background set. Pick a png/jpg/webp; it is drawn behind every panel.");
            }
            // Ensure the universal selector always has a model id.
            if state.config.model_id.trim().is_empty() {
                state.config.model_id = "deepseek-flash".to_string();
            }
            let family = models::provider_for_model(&state.config.model_id);
            let provider_name = models::provider(family).map(|p| p.name).unwrap_or(family);
            if state.config.base_url.is_none() {
                state.config.base_url = harness_core::provider::base_url_for(&state.config.model_id);
            }

            ui.add_space(4.0);
            ui.label(RichText::new("Model").color(theme::text()).strong());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(models::display_name(&state.config.model_id))
                        .color(theme::accent())
                        .strong(),
                );
                ui.label(
                    RichText::new(format!("{} · {}", provider_name, state.config.model_id))
                        .color(theme::dim())
                        .small(),
                );
                if ui.button("Choose model…").clicked() {
                    action = Some(SettingsAction::OpenModelFactory);
                }
            });
            ui.horizontal(|ui| {
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

            // Show the auto-filled base URL for the selected model.
            let base = state
                .config
                .base_url
                .clone()
                .or_else(|| harness_core::provider::base_url_for(&state.config.model_id))
                .unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label("Base URL");
                ui.label(RichText::new(base).color(theme::dim()).small());
            });

            ui.label("System prompt");
            crate::ui::text_edit(
                ui,
                &mut state.config.system_prompt,
                crate::ui::TextEditSpec::multiline()
                    .hint("Optional instructions prepended to every conversation")
                    .rows(3)
                    .width(f32::INFINITY),
            );
            ui.checkbox(
                &mut state.config.agentmem_mirror,
                "Mirror session summaries to the shared AGENTMEM store",
            );
            if ui.button("Save defaults").clicked() {
                action = Some(SettingsAction::SaveConfig);
            }
            ui.add_space(10.0);

            // ---- voice ------------------------------------------------------
            ui.separator();
            ui.label(RichText::new("Voice").color(theme::text()).strong());
            let stt = harness_core::voice::stt_providers();
            let tts = harness_core::voice::tts_providers();
            ui.label(
                RichText::new(format!(
                    "Speech to text: {}",
                    if stt.is_empty() { "no provider key found".to_string() } else { stt.join(" → ") }
                ))
                .color(if stt.is_empty() { theme::warn() } else { theme::ok() })
                .small(),
            );
            ui.label(
                RichText::new(format!(
                    "Text to speech: {}",
                    if tts.is_empty() { "no provider key found".to_string() } else { tts.join(" → ") }
                ))
                .color(if tts.is_empty() { theme::warn() } else { theme::ok() })
                .small(),
            );
            if !harness_core::voice::recorder_available() {
                ui.label(RichText::new("No microphone recorder found — install pw-record or arecord.").color(theme::warn()).small());
            }
            if !harness_core::voice::player_available() {
                ui.label(RichText::new("No audio player found — install ffplay, paplay or pw-play.").color(theme::warn()).small());
            }
            ui.horizontal(|ui| {
                ui.label("TTS voice");
                crate::ui::text_edit(
                    ui,
                    &mut state.config.tts_voice,
                    crate::ui::TextEditSpec::singleline()
                        .hint("mimo_default, Mia, or an ElevenLabs id")
                        .width(260.0),
                );
            });
            ui.small("Mic and speaker buttons live in the composer and on every assistant message.");
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
        assert_eq!(effort_label(EffortChoice::Max), "max");
        assert_eq!(effort_label(EffortChoice::None), "thinking off");
    }
}
