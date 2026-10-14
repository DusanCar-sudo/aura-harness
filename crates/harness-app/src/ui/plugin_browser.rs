//! Searchable browser of connectable plugins and agent tools.
//!
//! Five sources, in order: servers already on this machine, the local
//! catalogue, live **online search** (MCP registry + GitHub), the
//! **agenttool.sh** service directory (browse + open; the user decides what
//! a service is for), and a manual field that also accepts a bare GitHub
//! link — which is analyzed for its launch line and added automatically.

use crate::theme;
use crate::ui::settings::AvailableServer;
use harness_core::catalog;
use harness_core::config;
use harness_core::online::{self, OnlineEntry, Source};
use egui::{RichText, Ui};

/// What the user asked the browser to do this frame.
#[derive(Debug, Clone)]
pub enum BrowserAction {
    /// Add from a known name + launch line (catalogue, machine, manual).
    Add { name: String, line: String },
    /// Analyze a GitHub repo URL, derive its launch line, add automatically.
    Analyze(String),
    /// Open an external URL in the system browser (directory entries).
    Open(String),
    Close,
}

/// Live search results, owned by `main.rs` (which also owns the debounce).
#[derive(Debug, Default)]
pub struct OnlineResults {
    pub query: String,
    pub entries: Vec<OnlineEntry>,
    pub warning: Option<String>,
    pub busy: bool,
}

pub struct BrowserState<'a> {
    pub query: &'a mut String,
    /// The manual launch line, e.g. `npx -y rea-agents mcp` — or a GitHub URL.
    pub line: &'a mut String,
    pub available: &'a [AvailableServer],
    /// Names already configured, so they are not offered twice.
    pub configured: &'a [String],
    /// Live search results for the current query.
    pub online: &'a OnlineResults,
    /// The full agenttool.sh directory (loaded once; filtered here).
    pub directory: &'a [OnlineEntry],
}

pub fn show(ui: &mut Ui, state: &mut BrowserState<'_>) -> Option<BrowserAction> {
    let mut action = None;

    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Add a plugin")
                .color(theme::accent())
                .strong()
                .size(16.0),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Close").clicked() {
                action = Some(BrowserAction::Close);
            }
        });
    });
    theme::stripes(ui, ui.available_width().min(120.0));
    ui.add_space(6.0);

    ui.add(
        egui::TextEdit::singleline(state.query)
            .hint_text("Search — game dev, memory, fetch, github…")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(6.0);

    let q = state.query.trim().to_lowercase();

    // ---- already on this machine ------------------------------------------
    let machine: Vec<&AvailableServer> = state
        .available
        .iter()
        .filter(|s| {
            q.is_empty()
                || s.name.contains(&q)
                || s.summary().to_lowercase().contains(&q)
        })
        .collect();
    if !machine.is_empty() {
        ui.label(RichText::new("On this machine").color(theme::text()).strong());
        for server in machine {
            if add_row(ui, &server.name, &server.summary()) {
                action = Some(BrowserAction::Add {
                    name: server.name.clone(),
                    line: server.launch_line(),
                });
            }
        }
        ui.add_space(6.0);
    }

    // ---- local catalogue --------------------------------------------------
    let local_hits: Vec<_> = catalog::search(&state.query)
        .into_iter()
        .filter(|e| !state.configured.contains(&catalog::plugin_key(e)))
        .collect();
    if !local_hits.is_empty() {
        ui.label(RichText::new("MCP servers").color(theme::text()).strong());
        for entry in local_hits {
            if add_row(ui, entry.name, entry.description) {
                action = Some(BrowserAction::Add {
                    name: entry.name.to_string(),
                    line: entry.launch_line.to_string(),
                });
            }
            if entry.needs_key {
                ui.label(
                    RichText::new("     needs an API key in the environment before it starts")
                        .color(theme::warn())
                        .small(),
                );
            }
            if entry.needs_args {
                ui.label(
                    RichText::new("     append a path or argument after adding")
                        .color(theme::warn())
                        .small(),
                );
            }
        }
        ui.add_space(6.0);
    }

    // ---- live online search ----------------------------------------------
    render_online(ui, state, &q, &mut action);
    ui.add_space(6.0);

    // ---- agenttool.sh directory -------------------------------------------
    render_directory(ui, state, &q, &mut action);

    // ---- manual ------------------------------------------------------------
    ui.add_space(8.0);
    ui.separator();
    ui.label(
        RichText::new("Paste a launch command — or a GitHub repo link")
            .color(theme::dim())
            .small(),
    );
    ui.horizontal(|ui| {
        let is_url = online::looks_like_github_url(state.line);
        let field = ui.add(
            egui::TextEdit::singleline(state.line)
                .hint_text("npx -y rea-agents mcp   ·   https://github.com/owner/repo")
                .desired_width(f32::INFINITY),
        );
        let submitted = field.lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
            && !state.line.trim().is_empty();
        let button_label = if is_url { "Analyze" } else { "Add" };
        if ui.button(button_label).clicked() || submitted {
            if is_url {
                action = Some(BrowserAction::Analyze(state.line.trim().to_string()));
            } else {
                action = Some(BrowserAction::Add {
                    name: config::derive_name(state.line),
                    line: state.line.clone(),
                });
            }
        }
    });
    if online::looks_like_github_url(state.line) {
        ui.label(
            RichText::new(
                "GitHub link — the README will be read for its launch line and the plugin added automatically",
            )
            .color(theme::ok())
            .small(),
        );
    } else if !state.line.trim().is_empty() {
        ui.label(
            RichText::new(format!(
                "will be added as \u{201c}{}\u{201d}",
                config::derive_name(state.line)
            ))
            .color(theme::structure())
            .small(),
        );
    }

    action
}

/// Live search: MCP registry + GitHub, driven by `main.rs`'s debounce.
fn render_online(
    ui: &mut Ui,
    state: &BrowserState<'_>,
    q: &str,
    action: &mut Option<BrowserAction>,
) {
    if q.len() < 2 {
        return;
    }
    let same_query = state.online.query.eq_ignore_ascii_case(state.query.trim());
    if !same_query && !state.online.busy {
        return; // debounce has not fired yet for this query
    }

    ui.horizontal(|ui| {
        ui.label(RichText::new("Online").color(theme::text()).strong());
        if state.online.busy {
            ui.spinner();
            ui.label(RichText::new("searching…").color(theme::dim()).small());
        }
    });
    if let Some(warning) = &state.online.warning {
        ui.label(RichText::new(warning).color(theme::warn()).small());
    }
    if state.online.busy {
        return;
    }

    let hits: Vec<&OnlineEntry> = state
        .online
        .entries
        .iter()
        .filter(|e| !state.configured.contains(&e.name))
        .collect();
    if hits.is_empty() {
        ui.label(
            RichText::new("no online matches")
                .color(theme::dim())
                .small()
                .italics(),
        );
        return;
    }
    for entry in hits {
        match entry.source {
            // Registry entries that can be spawned get a one-click Add.
            Source::Registry if entry.launch_line.is_some() => {
                let line = entry.launch_line.clone().unwrap_or_default();
                if add_row(ui, &entry.name, &entry.description) {
                    *action = Some(BrowserAction::Add {
                        name: entry.name.clone(),
                        line,
                    });
                }
                if !entry.needs_env.is_empty() {
                    ui.label(
                        RichText::new(format!(
                            "     needs {} in the plugins.toml env table",
                            entry.needs_env.join(", ")
                        ))
                        .color(theme::warn())
                        .small(),
                    );
                }
            }
            // Everything else is a link: open it, decide there.
            _ => {
                if open_row(ui, &entry.name, &entry.detail(), &entry.description) {
                    *action = Some(BrowserAction::Open(entry.url.clone()));
                }
            }
        }
    }
}

/// agenttool.sh service directory: filtered locally, linked out.
fn render_directory(
    ui: &mut Ui,
    state: &BrowserState<'_>,
    q: &str,
    action: &mut Option<BrowserAction>,
) {
    if q.len() < 2 || state.directory.is_empty() {
        return;
    }
    let hits = online::filter_directory(state.directory, q);
    if hits.is_empty() {
        return;
    }

    ui.label(
        RichText::new("Agent tools (agenttool.sh)")
            .color(theme::text())
            .strong(),
    );
    ui.label(
        RichText::new(
            "Hosted services rated for agent use — open one to see what it offers; you decide how to use it.",
        )
        .color(theme::dim())
        .small(),
    );
    for entry in hits.iter().take(12) {
        if open_row(ui, &entry.name, &entry.detail(), &entry.description) {
            *action = Some(BrowserAction::Open(entry.url.clone()));
        }
    }
    if hits.len() > 12 {
        ui.label(
            RichText::new(format!("…and {} more — refine the search", hits.len() - 12))
                .color(theme::dim())
                .small(),
        );
    }
}

/// One row with an Add button. Returns true when Add was hit.
fn add_row(ui: &mut Ui, name: &str, detail: &str) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(name).color(theme::text()).monospace().small());
        ui.label(RichText::new(detail).color(theme::dim()).small());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Add").clicked() {
                clicked = true;
            }
        });
    });
    clicked
}

/// One row with an Open button (external link).
fn open_row(ui: &mut Ui, name: &str, detail: &str, description: &str) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(name).color(theme::text()).monospace().small());
            if !detail.is_empty() {
                ui.label(RichText::new(detail).color(theme::structure()).small());
            }
            if !description.is_empty() {
                ui.label(RichText::new(description).color(theme::dim()).small());
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Open").clicked() {
                clicked = true;
            }
        });
    });
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_state_borrows_plain_app_state() {
        let mut query = String::from("git");
        let mut line = String::new();
        let available: Vec<AvailableServer> = Vec::new();
        let configured = vec!["mnemai".to_string()];
        let online = OnlineResults::default();
        let directory: Vec<OnlineEntry> = Vec::new();
        let state = BrowserState {
            query: &mut query,
            line: &mut line,
            available: &available,
            configured: &configured,
            online: &online,
            directory: &directory,
        };
        assert_eq!(state.query, "git");
        assert!(state.available.is_empty());
        assert_eq!(state.configured[0], "mnemai");
        assert!(state.online.entries.is_empty());
        assert!(state.directory.is_empty());
    }
}
