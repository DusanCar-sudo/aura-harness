//! Model factory — organized provider directory, automatic Coding/PAYG plan
//! setup, custom OpenAI-compatible providers, and concurrent live `/models`
//! fetching.

use std::collections::{BTreeMap, BTreeSet};

use egui::{RichText, Ui};
use harness_core::config::{load_api_key_named, CustomProvider};
use harness_core::models::{self, ModelInfo, ProviderGroup, ProviderPlan, ProviderSpec};

use crate::theme;
use crate::ui::row_button;

#[derive(Debug, Clone)]
pub enum ModelFactoryAction {
    Select(ModelInfo),
    UseManual {
        provider: String,
        id: String,
    },
    Refresh(String),
    RefreshAllReady,
    SaveProviderPlan {
        provider_id: String,
        api_key: String,
        base_url: String,
        api_key_env: String,
        base_url_env: String,
    },
    SaveCustomProvider {
        provider: CustomProvider,
        api_key: String,
    },
    DeleteCustomProvider(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupFilter {
    #[default]
    All,
    Ready,
    CodingPlan,
    Direct,
    Cloud,
    Local,
    Custom,
}

impl GroupFilter {
    const ALL: [Self; 7] = [
        Self::All,
        Self::Ready,
        Self::CodingPlan,
        Self::Direct,
        Self::Cloud,
        Self::Local,
        Self::Custom,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Ready => "Ready",
            Self::CodingPlan => "Coding Plans",
            Self::Direct => "Direct",
            Self::Cloud => "Cloud",
            Self::Local => "Local",
            Self::Custom => "Custom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CapFilter {
    #[default]
    All,
    Free,
    Vision,
}

#[derive(Debug, Clone, Default)]
pub struct CustomDraft {
    pub id: String,
    pub name: String,
    pub plan: String,
    pub base_url: String,
    pub api_key_env: String,
    pub api_key: String,
    pub prefix: String,
    pub default_model: String,
}

impl CustomDraft {
    fn from_existing(p: &CustomProvider) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            plan: if p.plan.is_empty() {
                "Coding Plan".into()
            } else {
                p.plan.clone()
            },
            base_url: p.base_url.clone(),
            api_key_env: p.api_key_env.clone(),
            api_key: String::new(),
            prefix: p.prefix.clone(),
            default_model: p.default_model.clone(),
        }
    }

    fn fresh() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            plan: "Coding Plan".into(),
            base_url: String::new(),
            api_key_env: String::new(),
            api_key: String::new(),
            prefix: String::new(),
            default_model: String::new(),
        }
    }
}

pub struct ModelFactoryState {
    pub open: bool,
    pub provider_query: String,
    pub model_query: String,
    pub selected_provider: String,
    pub group_filter: GroupFilter,
    pub cap_filter: CapFilter,
    /// Live lists, keyed by provider family (built-in or custom id).
    pub live: BTreeMap<String, Vec<ModelInfo>>,
    /// Providers currently awaiting a `/models` response.
    pub fetching: BTreeSet<String>,
    /// Last fetch error per provider.
    pub errors: BTreeMap<String, String>,
    pub manual: String,
    /// Inline provider setup drawer (API key + Coding/PAYG plan endpoints).
    pub show_setup: bool,
    pub setup_plan_id: String,
    pub setup_api_key: String,
    pub setup_base_url: String,
    pub setup_prefix: String,
    /// Creating or editing a custom OpenAI-compatible provider.
    pub adding_custom: bool,
    pub custom_draft: CustomDraft,
}

impl Default for ModelFactoryState {
    fn default() -> Self {
        let mut s = Self {
            open: false,
            provider_query: String::new(),
            model_query: String::new(),
            selected_provider: "deepseek".to_string(),
            group_filter: GroupFilter::All,
            cap_filter: CapFilter::All,
            live: BTreeMap::new(),
            fetching: BTreeSet::new(),
            errors: BTreeMap::new(),
            manual: String::new(),
            show_setup: false,
            setup_plan_id: String::new(),
            setup_api_key: String::new(),
            setup_base_url: String::new(),
            setup_prefix: String::new(),
            adding_custom: false,
            custom_draft: CustomDraft::fresh(),
        };
        s.sync_setup_fields("deepseek", &[]);
        s
    }
}

impl ModelFactoryState {
    /// Open on the provider that owns the current model.
    pub fn open_for(&mut self, model_id: &str, custom: &[CustomProvider]) {
        self.open = true;
        self.adding_custom = false;
        let owner = custom
            .iter()
            .find(|c| {
                let pfx = if c.prefix.trim().is_empty() {
                    format!("{}/", c.id.trim())
                } else {
                    c.prefix.trim().to_string()
                };
                !pfx.is_empty() && model_id.to_lowercase().starts_with(&pfx.to_lowercase())
            })
            .map(|c| c.id.clone())
            .unwrap_or_else(|| models::provider_for_model(model_id).to_string());
        self.selected_provider = owner.clone();
        self.manual = model_id.to_string();
        self.sync_setup_fields(&owner, custom);
    }

    pub fn sync_setup_fields(&mut self, provider_id: &str, custom: &[CustomProvider]) {
        self.setup_api_key.clear();
        if let Some(c) = custom.iter().find(|c| c.id == provider_id) {
            self.custom_draft = CustomDraft::from_existing(c);
            self.setup_base_url = c.base_url.clone();
            self.setup_prefix = c.prefix.clone();
            self.show_setup = false;
            return;
        }
        let plans = models::plans_for(provider_id);
        let active_plan = plans.first().copied();
        if let Some(plan) = active_plan {
            self.setup_plan_id = plan.id.to_string();
            let saved_url = if plan.base_url_env.is_empty() {
                None
            } else {
                load_api_key_named(plan.base_url_env)
            };
            self.setup_base_url = saved_url
                .filter(|u| u.starts_with("http"))
                .unwrap_or_else(|| plan.base_url.to_string());
            self.setup_prefix = plan.prefix.to_string();
        } else {
            self.setup_plan_id.clear();
            self.setup_base_url = models::provider(provider_id)
                .map(models::base_url_for)
                .unwrap_or_default();
            self.setup_prefix = models::provider(provider_id)
                .map(|p| p.prefix.to_string())
                .unwrap_or_default();
        }
        if let Some(spec) = models::provider(provider_id) {
            self.show_setup = !spec.local && !models::is_configured(spec);
        }
    }

    pub fn models(&self, family: &str) -> Vec<ModelInfo> {
        self.live
            .get(family)
            .cloned()
            .unwrap_or_else(|| models::models_for(family))
    }

    pub fn all_known_models(&self, custom: &[CustomProvider]) -> Vec<ModelInfo> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        // Prefer freshly fetched live models first.
        for list in self.live.values() {
            for m in list {
                if seen.insert(m.id.clone()) {
                    out.push(m.clone());
                }
            }
        }
        for c in custom {
            if !c.default_model.trim().is_empty() {
                let pfx = if c.prefix.trim().is_empty() {
                    format!("{}/", c.id.trim())
                } else {
                    let t = c.prefix.trim();
                    if t.ends_with('/') {
                        t.to_string()
                    } else {
                        format!("{t}/")
                    }
                };
                let id = if c.default_model.contains('/') {
                    c.default_model.trim().to_string()
                } else {
                    format!("{pfx}{}", c.default_model.trim())
                };
                if seen.insert(id.clone()) {
                    out.push(ModelInfo {
                        id,
                        name: format!("{} ({})", c.name, c.default_model.trim()),
                        family: c.id.clone(),
                        speed: "Custom".into(),
                        free: false,
                    });
                }
            }
        }
        for m in models::KNOWN_MODELS {
            if seen.insert(m.id.to_string()) {
                out.push(ModelInfo {
                    id: m.id.to_string(),
                    name: m.name.to_string(),
                    family: m.family.to_string(),
                    speed: m.speed.to_string(),
                    free: m.speed.to_lowercase().contains("free"),
                });
            }
        }
        out
    }
}

pub fn show(
    ui: &mut Ui,
    state: &mut ModelFactoryState,
    current_model: &str,
    custom: &[CustomProvider],
) -> Option<ModelFactoryAction> {
    let mut action = None;

    ui.horizontal(|ui| {
        let live_total: usize = state.live.values().map(|v| v.len()).sum();
        let summary = if state.fetching.is_empty() {
            format!(
                "{} providers ({} custom) · {} curated · {} live loaded",
                models::PROVIDERS.len(),
                custom.len(),
                models::KNOWN_MODELS.len(),
                live_total
            )
        } else {
            format!(
                "Fetching live models from {} provider(s)… · {} live loaded",
                state.fetching.len(),
                live_total
            )
        };
        ui.label(RichText::new(summary).color(theme::dim()).small());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("+ Add custom provider")
                .on_hover_text("Add any Coding Plan / PAYG / OpenAI-compatible endpoint")
                .clicked()
            {
                state.adding_custom = true;
                state.custom_draft = CustomDraft::fresh();
            }
            if ui
                .button("Refresh all live")
                .on_hover_text("Fetch fresh /models lists from every configured provider")
                .clicked()
            {
                action = Some(ModelFactoryAction::RefreshAllReady);
            }
        });
    });
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label(RichText::new("Provider").color(theme::dim()).small());
        crate::ui::text_edit(
            ui,
            &mut state.provider_query,
            crate::ui::TextEditSpec::singleline()
                .hint("deepseek, alibaba, opencode, glm…")
                .width(190.0),
        );
        ui.add_space(10.0);
        ui.label(RichText::new("Model").color(theme::dim()).small());
        crate::ui::text_edit(
            ui,
            &mut state.model_query,
            crate::ui::TextEditSpec::singleline()
                .hint("filter models by name or id…")
                .width(240.0),
        );
    });
    ui.add_space(4.0);

    ui.horizontal_wrapped(|ui| {
        for g in GroupFilter::ALL {
            if g == GroupFilter::Custom && custom.is_empty() {
                continue;
            }
            ui.selectable_value(&mut state.group_filter, g, g.label());
        }
    });
    ui.separator();

    ui.columns(2, |columns| {
        // ---- left: organized providers -----------------------------------
        let left = &mut columns[0];
        let q = state.provider_query.trim().to_lowercase();
        let filtered_custom: Vec<&CustomProvider> = custom
            .iter()
            .filter(|c| {
                q.is_empty()
                    || c.name.to_lowercase().contains(&q)
                    || c.id.to_lowercase().contains(&q)
                    || c.base_url.to_lowercase().contains(&q)
            })
            .collect();

        egui::ScrollArea::vertical()
            .id_salt("model-factory-providers")
            .max_height(410.0)
            .auto_shrink([false, false])
            .show(left, |ui| {
                let mut select_builtin: Option<&ProviderSpec> = None;

                if !filtered_custom.is_empty()
                    && matches!(
                        state.group_filter,
                        GroupFilter::All | GroupFilter::Ready | GroupFilter::Custom
                    )
                {
                    ui.small(RichText::new("Custom Providers").color(theme::accent()).strong());
                    for c in &filtered_custom {
                        let selected = !state.adding_custom && state.selected_provider == c.id;
                        let right = if state.fetching.contains(&c.id) {
                            "fetching…".to_string()
                        } else if let Some(list) = state.live.get(&c.id) {
                            format!("{} live", list.len())
                        } else if c.plan.is_empty() {
                            "custom".to_string()
                        } else {
                            c.plan.clone()
                        };
                        let row = row_button(
                            ui,
                            24.0,
                            selected,
                            &format!("{} — {}", c.name, c.base_url),
                            &right,
                        );
                        if row.clicked() {
                            state.adding_custom = false;
                            state.selected_provider = c.id.clone();
                            state.model_query.clear();
                            state.manual.clear();
                            state.sync_setup_fields(&c.id, custom);
                            action = Some(ModelFactoryAction::Refresh(c.id.clone()));
                        }
                    }
                    ui.add_space(6.0);
                }

                let builtins = models::filter_providers(&state.provider_query);

                // In `All` mode (unfiltered query), surface configured remote providers first.
                if state.group_filter == GroupFilter::All && q.is_empty() {
                    let ready: Vec<&ProviderSpec> = builtins
                        .iter()
                        .copied()
                        .filter(|p| {
                            (!p.local && models::is_configured(p))
                                || (p.local && state.live.contains_key(p.id))
                        })
                        .collect();
                    if !ready.is_empty() {
                        ui.small(
                            RichText::new(format!("Ready ({})", ready.len()))
                                .color(theme::ok())
                                .strong(),
                        );
                        for spec in ready {
                            if render_provider_row(ui, state, spec).clicked() {
                                select_builtin = Some(spec);
                            }
                        }
                        ui.add_space(6.0);
                    }
                }

                let groups = match state.group_filter {
                    GroupFilter::All | GroupFilter::Ready => vec![
                        ProviderGroup::CodingPlan,
                        ProviderGroup::Direct,
                        ProviderGroup::Cloud,
                        ProviderGroup::Local,
                    ],
                    GroupFilter::CodingPlan => vec![ProviderGroup::CodingPlan],
                    GroupFilter::Direct => vec![ProviderGroup::Direct],
                    GroupFilter::Cloud => vec![ProviderGroup::Cloud],
                    GroupFilter::Local => vec![ProviderGroup::Local],
                    GroupFilter::Custom => vec![],
                };

                for grp in groups {
                    let members: Vec<&ProviderSpec> = builtins
                        .iter()
                        .copied()
                        .filter(|p| p.group == grp)
                        .filter(|p| {
                            state.group_filter != GroupFilter::Ready
                                || (!p.local && models::is_configured(p))
                                || (p.local && state.live.contains_key(p.id))
                        })
                        .collect();
                    if members.is_empty() {
                        continue;
                    }
                    ui.small(RichText::new(grp.label()).color(theme::dim()).strong());
                    for spec in members {
                        if render_provider_row(ui, state, spec).clicked() {
                            select_builtin = Some(spec);
                        }
                    }
                    ui.add_space(6.0);
                }

                if let Some(spec) = select_builtin {
                    state.adding_custom = false;
                    state.selected_provider = spec.id.to_string();
                    state.model_query.clear();
                    state.manual.clear();
                    state.sync_setup_fields(spec.id, custom);
                    if spec.live && (spec.local || models::is_configured(spec)) {
                        action = Some(ModelFactoryAction::Refresh(spec.id.to_string()));
                    }
                }
            });

        // ---- right: setup/plans + live/curated models -------------------
        let right = &mut columns[1];
        if state.adding_custom {
            render_custom_provider_form(right, state, &mut action, false);
            return;
        }

        if let Some(custom_spec) = custom.iter().find(|c| c.id == state.selected_provider) {
            render_custom_provider_right(right, state, custom_spec, current_model, &mut action);
            return;
        }

        let Some(spec) = models::provider(&state.selected_provider) else {
            right.label("Pick a provider on the left or click + Add custom provider.");
            return;
        };

        right.horizontal(|ui| {
            ui.label(RichText::new(spec.name).color(theme::text()).strong().size(15.0));
            ui.label(
                RichText::new(models::key_status(spec))
                    .color(if models::is_configured(spec) {
                        theme::ok()
                    } else {
                        theme::warn()
                    })
                    .small(),
            );
            if state.live.contains_key(spec.id) {
                ui.label(RichText::new("live").color(theme::ok()).small());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.toggle_value(&mut state.show_setup, "Key & Plan setup");
                if spec.live {
                    if state.fetching.contains(spec.id) {
                        ui.spinner();
                        ui.label(RichText::new("fetching…").color(theme::dim()).small());
                    } else if ui
                        .button("Fetch live")
                        .on_hover_text("Call this provider's /models endpoint now")
                        .clicked()
                    {
                        action = Some(ModelFactoryAction::Refresh(spec.id.to_string()));
                    }
                }
            });
        });
        right.label(RichText::new(spec.desc).color(theme::dim()).small());

        if let Some(err) = state.errors.get(spec.id) {
            if !state.fetching.contains(spec.id) {
                right.label(RichText::new(err).color(theme::warn()).small());
            }
        }

        // Inline Coding Plan vs PAYG API Plan configuration panel
        if state.show_setup {
            render_builtin_plan_setup(right, state, spec, &mut action);
        }

        right.add_space(4.0);
        let all = state.models(spec.id);
        let mut hits = models::filter_models(&all, &state.model_query);
        hits.retain(|m| match state.cap_filter {
            CapFilter::All => true,
            CapFilter::Free => m.free,
            CapFilter::Vision => models::supports_images(&m.id),
        });
        let live_on = state.live.contains_key(spec.id);
        right.horizontal(|ui| {
            ui.selectable_value(&mut state.cap_filter, CapFilter::All, "All");
            ui.selectable_value(&mut state.cap_filter, CapFilter::Free, "Free");
            ui.selectable_value(&mut state.cap_filter, CapFilter::Vision, "Vision");
            ui.add_space(8.0);
            let count = if live_on {
                format!("{} shown · {} live", hits.len(), all.len())
            } else {
                format!("{} shown · {} curated", hits.len(), all.len())
            };
            ui.label(RichText::new(count).color(theme::dim()).small());
        });

        let list_height = if state.show_setup { 170.0 } else { 315.0 };
        egui::ScrollArea::vertical()
            .id_salt("model-factory-models")
            .max_height(list_height)
            .auto_shrink([false, false])
            .show(right, |ui| {
                if hits.is_empty() {
                    ui.label(
                        RichText::new("No matching models. Enter an id below or adjust filter.")
                            .color(theme::dim())
                            .italics(),
                    );
                }
                for model in &hits {
                    let selected = current_model == model.id;
                    let hint = if selected {
                        "current".to_string()
                    } else if model.free && models::supports_images(&model.id) {
                        "free · vision".to_string()
                    } else if model.free {
                        "free".to_string()
                    } else if models::supports_images(&model.id) {
                        "vision".to_string()
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
                        let chosen = rewrite_for_selected_plan(state, spec, model.clone());
                        action = Some(ModelFactoryAction::Select(chosen));
                    }
                    row.on_hover_text(format!("{}\n{}\n{}", model.name, model.id, model.speed));
                }
            });

        right.add_space(4.0);
        right.separator();
        right.horizontal(|ui| {
            let prefix = if state.setup_prefix.is_empty() {
                spec.prefix
            } else {
                state.setup_prefix.as_str()
            };
            let hint = if prefix.is_empty() {
                "any model id this provider accepts".to_string()
            } else {
                format!("bare id (routed as {prefix})")
            };
            crate::ui::text_edit(
                ui,
                &mut state.manual,
                crate::ui::TextEditSpec::singleline()
                    .hint(&hint)
                    .width(270.0),
            );
            if ui.button("Use id").clicked() && !state.manual.trim().is_empty() {
                let raw = state.manual.trim();
                let id = if prefix.is_empty() || raw.contains('/') {
                    raw.to_string()
                } else {
                    format!("{prefix}{raw}")
                };
                action = Some(ModelFactoryAction::UseManual {
                    provider: spec.id.to_string(),
                    id,
                });
            }
        });
        let shown_endpoint = if !state.setup_base_url.trim().is_empty() {
            state.setup_base_url.trim().to_string()
        } else {
            models::base_url_for(spec)
        };
        right.label(
            RichText::new(format!("Endpoint: {shown_endpoint}"))
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

fn render_provider_row(
    ui: &mut Ui,
    state: &ModelFactoryState,
    spec: &ProviderSpec,
) -> egui::Response {
    let selected = !state.adding_custom && state.selected_provider == spec.id;
    let right = if state.fetching.contains(spec.id) {
        "fetching…".to_string()
    } else if let Some(list) = state.live.get(spec.id) {
        format!("{} live", list.len())
    } else if state.errors.contains_key(spec.id) && !spec.local {
        "error".to_string()
    } else {
        models::key_status(spec).to_string()
    };
    let row = row_button(
        ui,
        24.0,
        selected,
        &format!("{} — {}", spec.name, spec.desc),
        &right,
    );
    let hover = format!("{} — {}", spec.name, spec.desc);
    row.on_hover_text(hover)
}

fn render_builtin_plan_setup(
    ui: &mut Ui,
    state: &mut ModelFactoryState,
    spec: &ProviderSpec,
    action: &mut Option<ModelFactoryAction>,
) {
    let plans = models::plans_for(spec.id);
    egui::Frame::none()
        .fill(theme::card())
        .rounding(egui::Rounding::same(6.0))
        .inner_margin(egui::Margin::same(8.0))
        .show(ui, |ui| {
            let mut active_plan: Option<ProviderPlan> = plans
                .iter()
                .copied()
                .find(|p| p.id == state.setup_plan_id)
                .or_else(|| plans.first().copied());

            if plans.len() > 1 {
                ui.small(RichText::new("Plan (auto-selects endpoint & routing):").color(theme::dim()));
                ui.horizontal_wrapped(|ui| {
                    for p in &plans {
                        let is_sel = state.setup_plan_id == p.id;
                        if ui.selectable_label(is_sel, p.label).clicked() {
                            state.setup_plan_id = p.id.to_string();
                            let saved = if p.base_url_env.is_empty() {
                                None
                            } else {
                                load_api_key_named(p.base_url_env)
                            };
                            state.setup_base_url = saved
                                .filter(|u| u.starts_with("http"))
                                .unwrap_or_else(|| p.base_url.to_string());
                            state.setup_prefix = p.prefix.to_string();
                            active_plan = Some(*p);
                        }
                    }
                });
                if let Some(p) = active_plan {
                    ui.small(RichText::new(p.desc).color(theme::dim()));
                }
            }

            let key_env = active_plan
                .map(|p| p.api_key_env)
                .or_else(|| spec.env_keys.first().copied())
                .unwrap_or("");
            let base_env = active_plan
                .map(|p| p.base_url_env)
                .or_else(|| spec.base_url_env.first().copied())
                .unwrap_or("");

            if !spec.local && !key_env.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Key ({key_env})")).small());
                    let key_hint = if load_api_key_named(key_env).is_some() {
                        "saved in ~/.aura/keys.json — paste new key to replace"
                    } else {
                        "paste API key (saved to ~/.aura/keys.json 0600)"
                    };
                    let resp = crate::ui::text_edit(
                        ui,
                        &mut state.setup_api_key,
                        crate::ui::TextEditSpec::singleline()
                            .password(true)
                            .hint(key_hint)
                            .width(260.0),
                    );
                    if resp.changed() {
                        if let Some(inferred_id) =
                            models::infer_plan_for_key(spec.id, &state.setup_api_key)
                        {
                            if let Some(p) = plans.iter().find(|p| p.id == inferred_id) {
                                state.setup_plan_id = p.id.to_string();
                                state.setup_base_url = p.base_url.to_string();
                                state.setup_prefix = p.prefix.to_string();
                            }
                        }
                    }
                });
            }

            ui.horizontal(|ui| {
                ui.label(RichText::new("Endpoint URL").small());
                crate::ui::text_edit(
                    ui,
                    &mut state.setup_base_url,
                    crate::ui::TextEditSpec::singleline()
                        .hint("https://.../v1")
                        .width(260.0),
                );
                if let Some(p) = active_plan {
                    if ui.small_button("Reset URL").clicked() {
                        state.setup_base_url = p.base_url.to_string();
                    }
                }
            });

            ui.horizontal(|ui| {
                if ui.button("Save Plan & Fetch Live").clicked() {
                    *action = Some(ModelFactoryAction::SaveProviderPlan {
                        provider_id: spec.id.to_string(),
                        api_key: state.setup_api_key.trim().to_string(),
                        base_url: state.setup_base_url.trim().to_string(),
                        api_key_env: key_env.to_string(),
                        base_url_env: base_env.to_string(),
                    });
                }
                if spec.local {
                    ui.small(RichText::new("No API key required for local server.").color(theme::dim()));
                }
            });
        });
}

fn rewrite_for_selected_plan(
    state: &ModelFactoryState,
    spec: &ProviderSpec,
    mut model: ModelInfo,
) -> ModelInfo {
    if spec.id == "zhipu" && state.setup_plan_id == "zhipu-coding" && !model.id.starts_with("zhipu-coding/") {
        let bare = model.id.strip_prefix("zhipu/").unwrap_or(&model.id);
        model.id = format!("zhipu-coding/{bare}");
    } else if spec.id == "kimi" && state.setup_plan_id == "kimi-coding" && !model.id.starts_with("kimi-coding/") {
        let bare = model.id.strip_prefix("kimi/").unwrap_or(&model.id);
        model.id = format!("kimi-coding/{bare}");
    } else if (spec.id == "opencode" || spec.id == "opencode-go")
        && state.setup_plan_id == "opencode-go"
        && !model.id.starts_with("go-anthropic/")
        && !model.id.starts_with("opencode-go/")
    {
        let bare = model.id.strip_prefix("opencode/").unwrap_or(&model.id);
        model.id = format!("opencode-go/{bare}");
    }
    model
}

fn render_custom_provider_form(
    ui: &mut Ui,
    state: &mut ModelFactoryState,
    action: &mut Option<ModelFactoryAction>,
    editing_existing: bool,
) {
    ui.label(
        RichText::new(if editing_existing {
            "Edit Custom Provider"
        } else {
            "Add Custom OpenAI-Compatible Provider"
        })
        .color(theme::accent())
        .strong()
        .size(15.0),
    );
    ui.small(RichText::new("Works with any Coding Plan proxy, vLLM gateway, or OpenAI-compatible API.").color(theme::dim()));
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        ui.label("Plan");
        ui.selectable_value(&mut state.custom_draft.plan, "Coding Plan".into(), "Coding Plan");
        ui.selectable_value(&mut state.custom_draft.plan, "PAYG API".into(), "PAYG API Plan");
        ui.selectable_value(&mut state.custom_draft.plan, "Local / Proxy".into(), "Local / Proxy");
    });
    ui.horizontal(|ui| {
        ui.label("Display name");
        crate::ui::text_edit(
            ui,
            &mut state.custom_draft.name,
            crate::ui::TextEditSpec::singleline()
                .hint("e.g. My Coding Gateway")
                .width(240.0),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Provider ID");
        let resp = crate::ui::text_edit(
            ui,
            &mut state.custom_draft.id,
            crate::ui::TextEditSpec::singleline()
                .hint("e.g. my-gateway (letters/digits/-)")
                .width(220.0),
        );
        if resp.changed() && !editing_existing {
            let clean = slug_id(&state.custom_draft.id);
            if !clean.is_empty() {
                state.custom_draft.prefix = format!("{clean}/");
                if state.custom_draft.api_key_env.is_empty() {
                    state.custom_draft.api_key_env =
                        format!("{}_API_KEY", clean.to_uppercase().replace('-', "_"));
                }
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Endpoint URL");
        crate::ui::text_edit(
            ui,
            &mut state.custom_draft.base_url,
            crate::ui::TextEditSpec::singleline()
                .hint("https://api.example.com/v1")
                .width(280.0),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Routing prefix");
        crate::ui::text_edit(
            ui,
            &mut state.custom_draft.prefix,
            crate::ui::TextEditSpec::singleline()
                .hint("my-gateway/")
                .width(160.0),
        );
        ui.label("Env key name");
        crate::ui::text_edit(
            ui,
            &mut state.custom_draft.api_key_env,
            crate::ui::TextEditSpec::singleline()
                .hint("MY_GATEWAY_API_KEY")
                .width(170.0),
        );
    });
    ui.horizontal(|ui| {
        ui.label("API Key");
        crate::ui::text_edit(
            ui,
            &mut state.custom_draft.api_key,
            crate::ui::TextEditSpec::singleline()
                .password(true)
                .hint("saved in ~/.aura/keys.json (optional for local)")
                .width(280.0),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Default model");
        crate::ui::text_edit(
            ui,
            &mut state.custom_draft.default_model,
            crate::ui::TextEditSpec::singleline()
                .hint("optional starter model id")
                .width(240.0),
        );
    });
    ui.add_space(6.0);
    let can_save = !slug_id(&state.custom_draft.id).is_empty()
        && state.custom_draft.base_url.trim().starts_with("http");
    ui.horizontal(|ui| {
        if ui
            .add_enabled(can_save, egui::Button::new("Save & Fetch Live Models"))
            .clicked()
        {
            let id = slug_id(&state.custom_draft.id);
            let name = if state.custom_draft.name.trim().is_empty() {
                id.clone()
            } else {
                state.custom_draft.name.trim().to_string()
            };
            let prefix = if state.custom_draft.prefix.trim().is_empty() {
                format!("{id}/")
            } else {
                let p = state.custom_draft.prefix.trim();
                if p.ends_with('/') {
                    p.to_string()
                } else {
                    format!("{p}/")
                }
            };
            *action = Some(ModelFactoryAction::SaveCustomProvider {
                provider: CustomProvider {
                    id,
                    name,
                    plan: state.custom_draft.plan.clone(),
                    base_url: state
                        .custom_draft
                        .base_url
                        .trim()
                        .trim_end_matches('/')
                        .to_string(),
                    api_key_env: state.custom_draft.api_key_env.trim().to_string(),
                    prefix,
                    default_model: state.custom_draft.default_model.trim().to_string(),
                },
                api_key: state.custom_draft.api_key.trim().to_string(),
            });
            state.adding_custom = false;
        }
        if ui.button("Cancel").clicked() {
            state.adding_custom = false;
        }
    });
}

fn render_custom_provider_right(
    ui: &mut Ui,
    state: &mut ModelFactoryState,
    custom: &CustomProvider,
    current_model: &str,
    action: &mut Option<ModelFactoryAction>,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(&custom.name).color(theme::text()).strong().size(15.0));
        if !custom.plan.is_empty() {
            ui.label(RichText::new(&custom.plan).color(theme::accent()).small());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Delete").clicked() {
                *action = Some(ModelFactoryAction::DeleteCustomProvider(custom.id.clone()));
            }
            if state.fetching.contains(&custom.id) {
                ui.spinner();
            } else if ui.button("Fetch live").clicked() {
                *action = Some(ModelFactoryAction::Refresh(custom.id.clone()));
            }
        });
    });
    ui.small(
        RichText::new(format!("{} · env: {}", custom.base_url, custom.api_key_env))
            .color(theme::dim()),
    );
    if let Some(err) = state.errors.get(&custom.id) {
        ui.small(RichText::new(err).color(theme::warn()));
    }
    ui.separator();

    let all = state.live.get(&custom.id).cloned().unwrap_or_default();
    let hits = models::filter_models(&all, &state.model_query);
    egui::ScrollArea::vertical()
        .id_salt("model-factory-custom-models")
        .max_height(260.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if hits.is_empty() && !custom.default_model.trim().is_empty() {
                let pfx = if custom.prefix.is_empty() {
                    format!("{}/", custom.id)
                } else {
                    custom.prefix.clone()
                };
                let full_id = if custom.default_model.contains('/') {
                    custom.default_model.clone()
                } else {
                    format!("{pfx}{}", custom.default_model)
                };
                let row = row_button(ui, 24.0, current_model == full_id, &full_id, "default");
                if row.clicked() {
                    *action = Some(ModelFactoryAction::Select(ModelInfo {
                        id: full_id,
                        name: custom.default_model.clone(),
                        family: custom.id.clone(),
                        speed: "Custom".into(),
                        free: false,
                    }));
                }
            }
            for model in &hits {
                let selected = current_model == model.id;
                let row = row_button(
                    ui,
                    24.0,
                    selected,
                    &format!("{}  ·  {}", model.name, model.id),
                    if selected { "current" } else { "live" },
                );
                if row.clicked() {
                    *action = Some(ModelFactoryAction::Select(model.clone()));
                }
            }
        });

    ui.separator();
    ui.horizontal(|ui| {
        let hint = format!("bare id (prefixed with {})", custom.prefix);
        crate::ui::text_edit(
            ui,
            &mut state.manual,
            crate::ui::TextEditSpec::singleline()
                .hint(&hint)
                .width(260.0),
        );
        if ui.button("Use id").clicked() && !state.manual.trim().is_empty() {
            let raw = state.manual.trim();
            let id = if custom.prefix.is_empty() || raw.starts_with(&custom.prefix) {
                raw.to_string()
            } else {
                format!("{}{raw}", custom.prefix)
            };
            *action = Some(ModelFactoryAction::UseManual {
                provider: custom.id.clone(),
                id,
            });
        }
    });
}

fn slug_id(raw: &str) -> String {
    raw.trim()
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}
