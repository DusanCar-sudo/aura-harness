//! Model factory — the full provider → model selector ported from Aura Code.
//!
//! The composer opens this instead of the old two-entry DeepSeek dropdown.
//! Providers are listed on the left with their key status; the right side
//! shows the curated catalogue immediately and can replace it with the
//! provider's live `/models` answer.

use std::collections::BTreeMap;

use egui::{RichText, Ui};
use harness_core::models::{self, ModelInfo};

use crate::theme;
use crate::ui::row_button;

#[derive(Debug, Clone)]
pub enum ModelFactoryAction {
    Select(ModelInfo),
    UseManual { provider: String, id: String },
    Refresh(String),
}

pub struct ModelFactoryState {
    pub open: bool,
    pub provider_query: String,
    pub model_query: String,
    pub selected_provider: String,
    /// Live lists, keyed by provider family. Presence means "live is fresher
    /// than the curated list" and is shown as a badge.
    pub live: BTreeMap<String, Vec<ModelInfo>>,
    pub fetching: Option<String>,
    pub error: Option<String>,
    pub manual: String,
}

impl Default for ModelFactoryState {
    fn default() -> Self {
        Self {
            open: false,
            provider_query: String::new(),
            model_query: String::new(),
            selected_provider: "deepseek".to_string(),
            live: BTreeMap::new(),
            fetching: None,
            error: None,
            manual: String::new(),
        }
    }
}

impl ModelFactoryState {
    /// Open on the provider that owns the current model.
    pub fn open_for(&mut self, model_id: &str) {
        self.open = true;
        self.selected_provider = models::provider_for_model(model_id).to_string();
        self.manual = model_id.to_string();
    }

    pub fn models(&self, family: &str) -> Vec<ModelInfo> {
        self.live
            .get(family)
            .cloned()
            .unwrap_or_else(|| models::models_for(family))
    }
}

pub fn show(
    ui: &mut Ui,
    state: &mut ModelFactoryState,
    current_model: &str,
) -> Option<ModelFactoryAction> {
    let mut action = None;

    ui.label(
        RichText::new(format!(
            "{} providers · {} curated models · live lists replace the curated ones for the selected provider",
            models::PROVIDERS.len(),
            models::KNOWN_MODELS.len()
        ))
        .color(theme::dim())
        .small(),
    );
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label(RichText::new("Find provider").color(theme::dim()).small());
        ui.add(
            egui::TextEdit::singleline(&mut state.provider_query)
                .hint_text("deepseek, groq, openrouter…")
                .desired_width(180.0),
        );
        ui.add_space(12.0);
        ui.label(RichText::new("Find model").color(theme::dim()).small());
        ui.add(
            egui::TextEdit::singleline(&mut state.model_query)
                .hint_text("name or id")
                .desired_width(240.0),
        );
    });
    ui.add_space(4.0);

    ui.columns(2, |columns| {
        // ---- providers ---------------------------------------------------
        let left = &mut columns[0];
        left.label(RichText::new("Providers").color(theme::text()).strong());
        left.label(
            RichText::new("A key in ~/.aura/keys.json or ~/.aura/.env marks a provider ready.")
                .color(theme::dim())
                .small(),
        );
        egui::ScrollArea::vertical()
            .id_salt("model-factory-providers")
            .max_height(370.0)
            .auto_shrink([false, false])
            .show(left, |ui| {
                for spec in models::filter_providers(&state.provider_query) {
                    let right = models::key_status(spec).to_string();
                    let selected = state.selected_provider == spec.id;
                    let row = row_button(
                        ui,
                        24.0,
                        selected,
                        &format!("{} — {}", spec.name, spec.desc),
                        &right,
                    );
                    if row.clicked() {
                        state.selected_provider = spec.id.to_string();
                        state.model_query.clear();
                        state.manual.clear();
                        state.error = None;
                        // Fetch immediately when the provider can list live
                        // models and has not been fetched yet.
                        if spec.live && models::is_configured(spec) && !state.live.contains_key(spec.id) {
                            action = Some(ModelFactoryAction::Refresh(spec.id.to_string()));
                        }
                    }
                    row.on_hover_text(format!("{} — {}", spec.name, spec.desc));
                }
            });

        // ---- models ------------------------------------------------------
        let right = &mut columns[1];
        let Some(spec) = models::provider(&state.selected_provider) else {
            right.label("Pick a provider first.");
            return;
        };
        right.horizontal(|ui| {
            ui.label(RichText::new(spec.name).color(theme::text()).strong().size(15.0));
            ui.label(RichText::new(models::key_status(spec)).color(if models::is_configured(spec) { theme::ok() } else { theme::warn() }).small());
            let live = state.live.contains_key(spec.id);
            if live {
                ui.label(RichText::new("live").color(theme::ok()).small());
            }
        });
        right.label(RichText::new(spec.desc).color(theme::dim()).small());

        right.horizontal(|ui| {
            let fetching = state.fetching.as_deref() == Some(spec.id);
            if spec.live {
                if fetching {
                    ui.spinner();
                    ui.label(RichText::new("fetching live models…").color(theme::dim()).small());
                } else if ui
                    .button("Fetch live models")
                    .on_hover_text("Call this provider's /models endpoint; the curated list stays as a fallback")
                    .clicked()
                {
                    action = Some(ModelFactoryAction::Refresh(spec.id.to_string()));
                }
            } else {
                ui.small("Curated list only for this provider.");
            }
        });
        if let Some(err) = &state.error {
            if state.fetching.as_deref() != Some(spec.id) {
                right.label(RichText::new(err).color(theme::warn()).small());
            }
        }

        let all = state.models(spec.id);
        let hits = models::filter_models(&all, &state.model_query);
        let live = state.live.contains_key(spec.id);
        let count = if live {
            format!(
                "{} shown · {} live · {} curated fallback",
                hits.len(),
                all.len(),
                models::models_for(spec.id).len()
            )
        } else {
            format!("{} shown · {} curated", hits.len(), all.len())
        };
        right.label(RichText::new(count).color(theme::dim()).small());

        egui::ScrollArea::vertical()
            .id_salt("model-factory-models")
            .max_height(330.0)
            .auto_shrink([false, false])
            .show(right, |ui| {
                if hits.is_empty() {
                    ui.label(RichText::new("No match. Type the id manually below.").color(theme::dim()).italics());
                }
                for model in &hits {
                    let selected = current_model == model.id;
                    let hint = if selected {
                        "current".to_string()
                    } else if model.free {
                        "free".to_string()
                    } else {
                        String::new()
                    };
                    let row = row_button(
                        ui,
                        24.0,
                        selected,
                        &format!("{}  ·  {}", model.name, model.id),
                        &hint,
                    );
                    if row.clicked() {
                        action = Some(ModelFactoryAction::Select(model.clone()));
                    }
                    row.on_hover_text(format!("{}\n{}\n{}", model.name, model.id, model.speed));
                }
            });

        right.add_space(6.0);
        right.separator();
        right.horizontal(|ui| {
            let hint = if spec.prefix.is_empty() {
                "any model id this provider accepts".to_string()
            } else {
                format!("bare id (routed as {})", spec.prefix)
            };
            ui.add(
                egui::TextEdit::singleline(&mut state.manual)
                    .hint_text(hint)
                    .desired_width(280.0),
            );
            if ui.button("Use id").clicked() && !state.manual.trim().is_empty() {
                action = Some(ModelFactoryAction::UseManual {
                    provider: spec.id.to_string(),
                    id: state.manual.trim().to_string(),
                });
            }
        });
        right.label(
            RichText::new(format!(
                "Endpoint: {}",
                models::base_url_for(spec)
            ))
            .color(theme::dim())
            .small(),
        );
    });

    ui.add_space(6.0);
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Current").color(theme::dim()).small());
        ui.label(
            RichText::new(format!(
                "{}  ·  {}",
                models::display_name(current_model),
                current_model
            ))
            .color(theme::text())
            .monospace()
            .small(),
        );
    });

    action
}
