//! Searchable browser of connectable plugins and agent tools.
//!
//! Five sources, in order: servers already on this machine, the local
//! catalogue, live **online search** (MCP registry + GitHub), the
//! **agenttool.sh** service directory (browse + open; the user decides what
//! a service is for), and a manual field that also accepts a bare GitHub
//! link — which is analyzed for its launch line and added automatically.

use crate::theme;
use crate::ui::settings::AvailableServer;
use egui::{RichText, Ui};
use harness_core::catalog;
use harness_core::config;
use harness_core::online::{self, OnlineEntry, Source};

/// What the user asked the browser to do this frame.
#[derive(Debug, Clone)]
pub enum BrowserAction {
    /// Add from a known name + launch line (catalogue, machine, manual).
    Add { name: String, line: String },
    /// Import one or more servers from pasted `mcpServers` JSON.
    AddJson(Vec<(String, config::McpServer)>),
    /// Analyze a GitHub repo URL, derive its launch line, add automatically.
    Analyze(String),
    /// Open an external URL in the system browser (directory entries).
    Open(String),
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
    pub category: &'a mut String,
    /// The manual launch line, e.g. `npx -y rea-agents mcp`, a GitHub URL, or JSON.
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

    crate::ui::text_edit(
        ui,
        state.query,
        crate::ui::TextEditSpec::singleline()
            .hint("Search MCP plugins — chrome, memory, db, playwright, github…")
            .width(f32::INFINITY),
    );
    ui.add_space(4.0);

    ui.horizontal_wrapped(|ui| {
        if ui
            .selectable_label(state.category.is_empty(), "All")
            .clicked()
        {
            state.category.clear();
        }
        for &cat in catalog::categories() {
            let selected = state.category == cat;
            if ui.selectable_label(selected, cat).clicked() {
                *state.category = cat.to_string();
            }
        }
    });
    ui.add_space(4.0);

    let q = state.query.trim().to_lowercase();

    // ---- already on this machine ------------------------------------------
    let machine: Vec<&AvailableServer> = if state.category.is_empty() {
        state
            .available
            .iter()
            .filter(|s| q.is_empty() || s.name.contains(&q) || s.summary().to_lowercase().contains(&q))
            .collect()
    } else {
        Vec::new()
    };
    let machine_empty = machine.is_empty();
    if !machine.is_empty() {
        ui.label(
            RichText::new("Detected on this machine (~/.aura/mcp.json)")
                .color(theme::ok())
                .strong(),
        );
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
    let cat_filter = (!state.category.is_empty()).then_some(state.category.as_str());
    let local_hits: Vec<_> = catalog::search_filtered(state.query, cat_filter)
        .into_iter()
        .filter(|e| !state.configured.contains(&catalog::plugin_key(e)))
        .collect();
    let local_empty = local_hits.is_empty();
    if !local_hits.is_empty() {
        ui.label(
            RichText::new(format!("Curated MCP servers ({})", local_hits.len()))
                .color(theme::text())
                .strong(),
        );
        for entry in local_hits {
            let mut desc = entry.description.to_string();
            if entry.needs_key {
                desc.push_str(" [needs env key]");
            }
            if entry.needs_args {
                desc.push_str(" [needs arg]");
            }
            if add_row(ui, entry.name, &desc) {
                action = Some(BrowserAction::Add {
                    name: entry.name.to_string(),
                    line: entry.launch_line.to_string(),
                });
            }
        }
        ui.add_space(6.0);
    }

    if q.is_empty() && machine_empty && local_empty {
        ui.label(
            RichText::new(
                "All matching curated servers are already configured. Type 2+ characters to search the online MCP registry + GitHub, or paste a launch line / mcpServers JSON below.",
            )
            .color(theme::dim())
            .italics()
            .small(),
        );
        ui.add_space(6.0);
    }

    // ---- live online search ----------------------------------------------
    render_online(ui, state, &q, &mut action);
    ui.add_space(6.0);

    // ---- agenttool.sh directory -------------------------------------------
    render_directory(ui, state, &q, &mut action);

    // ---- manual command, GitHub link, or mcpServers JSON ------------------
    ui.add_space(8.0);
    ui.separator();
    ui.label(
        RichText::new("Paste launch command, GitHub repo URL, or `mcpServers` JSON")
            .color(theme::dim())
            .small(),
    );
    ui.horizontal(|ui| {
        let parsed_json = online::parse_mcp_json_snippet(state.line);
        let is_url = online::looks_like_github_url(state.line);
        let field = crate::ui::text_edit(
            ui,
            state.line,
            crate::ui::TextEditSpec::singleline()
                .hint(r#"npx -y pkg  ·  https://github.com/owner/repo  ·  {"mcpServers":{...}}"#)
                .width(f32::INFINITY),
        );
        let submitted = field.lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
            && !state.line.trim().is_empty();
        let button_label = if parsed_json.is_some() {
            "Import JSON"
        } else if is_url {
            "Analyze"
        } else {
            "Add"
        };
        if ui.button(button_label).clicked() || submitted {
            if let Some(servers) = parsed_json {
                action = Some(BrowserAction::AddJson(servers));
            } else if is_url {
                action = Some(BrowserAction::Analyze(state.line.trim().to_string()));
            } else {
                action = Some(BrowserAction::Add {
                    name: config::derive_name(state.line),
                    line: state.line.clone(),
                });
            }
        }
    });
    if let Some(servers) = online::parse_mcp_json_snippet(state.line) {
        ui.label(
            RichText::new(format!(
                "Detected {} MCP server(s) in JSON: {}",
                servers.len(),
                servers.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")
            ))
            .color(theme::ok())
            .small(),
        );
    } else if online::looks_like_github_url(state.line) {
        ui.label(
            RichText::new(
                "GitHub link — the README will be read for its launch line and the plugin added automatically",
            )
            .color(theme::ok())
            .small(),
        );
    } else if !state.line.trim().is_empty() {
        let tokens = config::split_args(state.line);
        if let Some(cmd) = tokens.first() {
            if let Err(reason) = config::validate_mcp_command(cmd, &tokens[1..]) {
                ui.label(RichText::new(reason).color(theme::warn()).small());
            } else {
                ui.label(
                    RichText::new(format!(
                        "will be added as \u{201c}{}\u{201d}",
                        config::derive_name(state.line)
                    ))
                    .color(theme::structure())
                    .small(),
                );
            }
        }
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

/// One row with an Add affordance. The whole line is clickable — the tiny
/// right-hand button is only a hint, not the target.
fn add_row(ui: &mut Ui, name: &str, detail: &str) -> bool {
    let label = if detail.is_empty() {
        name.to_string()
    } else {
        format!("{name} — {detail}")
    };
    let row = crate::ui::row_button(ui, 24.0, false, &label, "+ Add");
    let clicked = row.clicked();
    row.on_hover_text(format!("{name}\n{detail}\nClick to add"));
    clicked
}

/// One row with an Open affordance (external link), also fully clickable.
fn open_row(ui: &mut Ui, name: &str, detail: &str, description: &str) -> bool {
    let mut label = name.to_string();
    if !detail.is_empty() {
        label.push_str(" — ");
        label.push_str(detail);
    }
    let row = crate::ui::row_button(ui, 24.0, false, &label, "Open");
    let clicked = row.clicked();
    row.on_hover_text(if description.is_empty() {
        format!("{name}\n{detail}\nClick to open")
    } else {
        format!("{name}\n{detail}\n{description}\nClick to open")
    });
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_state_borrows_plain_app_state() {
        let mut query = String::from("git");
        let mut category = String::new();
        let mut line = String::new();
        let available: Vec<AvailableServer> = Vec::new();
        let configured = vec!["mnemai".to_string()];
        let online = OnlineResults::default();
        let directory: Vec<OnlineEntry> = Vec::new();
        let state = BrowserState {
            query: &mut query,
            category: &mut category,
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
