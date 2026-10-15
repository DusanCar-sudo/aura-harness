//! Settings sheet: API key, model defaults, system prompt, plugin inventory.
//!
//! The panel only edits state and reports an action; `main.rs` owns the disk
//! writes (credentials 0600, config.toml, plugins.toml).

use crate::theme;
use harness_core::config::{Config, EffortChoice, ModelChoice, Plugins, ThemeChoice};
use harness_core::provider::{KNOWN_PROVIDERS, base_url_for, get_api_key_for_model};
use egui::{RichText, Ui};

/// One model offered by the universal multi-provider selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownModel {
    pub id: &'static str,
    pub name: &'static str,
    pub family: &'static str,
}

/// Universal model catalogue (at least 30 entries) covering the major providers.
pub const KNOWN_MODELS: &[KnownModel] = &[
    // Anthropic / Claude (4)
    KnownModel { id: "claude-opus-4", name: "Claude Opus 4", family: "anthropic" },
    KnownModel { id: "claude-sonnet-4", name: "Claude Sonnet 4", family: "anthropic" },
    KnownModel { id: "claude-3-7-sonnet", name: "Claude 3.7 Sonnet", family: "anthropic" },
    KnownModel { id: "claude-3-5-haiku", name: "Claude 3.5 Haiku", family: "anthropic" },
    // OpenAI (5)
    KnownModel { id: "gpt-4o", name: "GPT-4o", family: "openai" },
    KnownModel { id: "gpt-4o-mini", name: "GPT-4o mini", family: "openai" },
    KnownModel { id: "gpt-4.1", name: "GPT-4.1", family: "openai" },
    KnownModel { id: "o3", name: "o3", family: "openai" },
    KnownModel { id: "o4-mini", name: "o4-mini", family: "openai" },
    // Google (4)
    KnownModel { id: "gemini-2.5-pro", name: "Gemini 2.5 Pro", family: "google" },
    KnownModel { id: "gemini-2.5-flash", name: "Gemini 2.5 Flash", family: "google" },
    KnownModel { id: "gemini-2.0-flash", name: "Gemini 2.0 Flash", family: "google" },
    KnownModel { id: "gemini-1.5-pro", name: "Gemini 1.5 Pro", family: "google" },
    // DeepSeek (3)
    KnownModel { id: "deepseek-chat", name: "DeepSeek Chat", family: "deepseek" },
    KnownModel { id: "deepseek-reasoner", name: "DeepSeek Reasoner", family: "deepseek" },
    KnownModel { id: "deepseek-v4-pro", name: "DeepSeek V4 Pro", family: "deepseek" },
    // xAI (2)
    KnownModel { id: "grok-3", name: "Grok 3", family: "xai" },
    KnownModel { id: "grok-2", name: "Grok 2", family: "xai" },
    // Ollama (4)
    KnownModel { id: "ollama/llama3.1", name: "Llama 3.1 (Ollama)", family: "ollama" },
    KnownModel { id: "ollama/qwen2.5", name: "Qwen 2.5 (Ollama)", family: "ollama" },
    KnownModel { id: "ollama/codellama", name: "CodeLlama (Ollama)", family: "ollama" },
    KnownModel { id: "ollama/mistral", name: "Mistral (Ollama)", family: "ollama" },
    // Others
    KnownModel { id: "mistral/mistral-large", name: "Mistral Large", family: "mistral" },
    KnownModel { id: "groq/llama-3.3-70b", name: "Llama 3.3 70B (Groq)", family: "groq" },
    KnownModel { id: "qwen/qwen2.5", name: "Qwen 2.5", family: "qwen" },
    KnownModel { id: "cerebras/llama3.1-8b", name: "Llama 3.1 8B (Cerebras)", family: "cerebras" },
    KnownModel { id: "nvidia/llama-3.1", name: "Llama 3.1 (NVIDIA)", family: "nvidia" },
    KnownModel { id: "minimax/minimax-m1", name: "MiniMax M1", family: "minimax" },
    KnownModel { id: "openrouter/claude-3.5-sonnet", name: "Claude 3.5 Sonnet (OpenRouter)", family: "openrouter" },
    KnownModel { id: "lmstudio/local", name: "Local model (LM Studio)", family: "lmstudio" },
    KnownModel { id: "gpt-4.1-mini", name: "GPT-4.1 mini", family: "openai" },
];

pub fn providers_for_known_models() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for m in KNOWN_MODELS {
        if !out.contains(&m.family) {
            out.push(m.family);
        }
    }
    out
}

pub fn models_for_provider(family: &str) -> Vec<&'static KnownModel> {
    KNOWN_MODELS.iter().filter(|m| m.family == family).collect()
}

pub fn auto_base_url(model_id: &str) -> Option<String> {
    base_url_for(model_id)
}

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
                ui.label("Background image");
                ui.add(
                    egui::TextEdit::singleline(&mut state.config.background_image)
                        .desired_width(220.0)
                        .hint_text("path to png/jpg/webp"),
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
            // Ensure universal selector has a default model id.
            if state.config.model_id.trim().is_empty() {
                state.config.model_id = "deepseek-flash".to_string();
            }
            if state.config.base_url.is_none() {
                state.config.base_url = base_url_for(&state.config.model_id);
            }

            let current_family = KNOWN_MODELS
                .iter()
                .find(|m| m.id == state.config.model_id)
                .map(|m| m.family)
                .unwrap_or_else(|| {
                    // Fall back to provider descriptor family when not in list.
                    harness_core::provider::provider_family(&state.config.model_id)
                });

            ui.horizontal(|ui| {
                ui.label("Provider");
                egui::ComboBox::from_id_salt("cfg-provider")
                    .selected_text(current_family)
                    .show_ui(ui, |ui| {
                        for fam in providers_for_known_models() {
                            if ui.selectable_label(current_family == fam, fam).clicked() {
                                if let Some(first) = KNOWN_MODELS.iter().find(|m| m.family == fam) {
                                    state.config.model_id = first.id.to_string();
                                    state.config.base_url = auto_base_url(first.id);
                                }
                            }
                        }
                    });
                ui.label("Model");
                egui::ComboBox::from_id_salt("cfg-model")
                    .selected_text(state.config.model_id.clone())
                    .show_ui(ui, |ui| {
                        for m in models_for_provider(current_family) {
                            if ui
                                .selectable_label(state.config.model_id == m.id, m.name)
                                .clicked()
                            {
                                state.config.model_id = m.id.to_string();
                                state.config.base_url = auto_base_url(m.id);
                            }
                        }
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

            // Show the auto-filled base URL for the selected model.
            let _key = get_api_key_for_model(&state.config.model_id);
            let base = state
                .config
                .base_url
                .clone()
                .or_else(|| base_url_for(&state.config.model_id))
                .unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label("Base URL");
                ui.label(RichText::new(base).color(theme::dim()).small());
            });
            // Keep provider descriptor usage visible (no-op safety).
            let _ = KNOWN_PROVIDERS.len();

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
