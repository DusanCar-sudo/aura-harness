//! Aura Harness — the DeepSeek harness as a standalone desktop app.
//!
//! This crate owns all disk I/O (config, credentials, sessions) and the tokio
//! runtime; `harness-core` stays GUI-free and testable.

mod markdown;
mod theme;
mod ui;

use std::collections::BTreeMap;
use std::sync::{mpsc as std_mpsc, Arc};
use std::time::Duration;

use harness_core::agent::{self, Approval, LoopEvent, LoopInput};
use harness_core::api::{ChatOptions, Message};
use harness_core::attach::{self, Attachment};
use harness_core::config::{self, Config, EffortChoice, ModelChoice, Plugins};
use harness_core::mcp::{self, McpConnection};
use harness_core::session::{sessions_dir, Session, SessionHeader};
use harness_core::tools::{self, RegisteredTool, ToolRegistry};
use eframe::egui;
use egui_commonmark::CommonMarkCache;
use tokio::sync::Mutex;

use ui::chat::{ChatAction, ChatState, LiveTurn};
use ui::input::{ComposerAction, ComposerState};
use ui::plugin_browser::{BrowserAction, BrowserState};
use ui::settings::{AvailableServer, SettingsAction, SettingsState};
use ui::sidebar::{SidebarAction, SidebarState};
use ui::ToolCard;

/// Window icon, embedded so the binary is self-contained.
const ICON_RGBA: &[u8] = include_bytes!("../../../assets/icon.rgba");

/// Result of an MCP boot pass, delivered back to the UI thread.
struct McpBoot {
    tools: Vec<RegisteredTool>,
    status: Vec<String>,
    failures: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Chat,
    Settings,
}

struct App {
    rt: tokio::runtime::Runtime,
    http: reqwest::Client,

    config: Config,
    plugins: Plugins,
    api_key: Option<String>,

    sessions: Vec<SessionHeader>,
    session: Option<Session>,

    draft: String,
    attachments: Vec<Attachment>,
    model: ModelChoice,
    effort: EffortChoice,

    live: LiveTurn,
    cards: Vec<ToolCard>,
    busy: bool,
    events: Option<tokio::sync::mpsc::Receiver<LoopEvent>>,
    approvals: Option<tokio::sync::mpsc::Sender<Approval>>,

    registry: ToolRegistry,
    mcp: Arc<Mutex<BTreeMap<String, McpConnection>>>,
    mcp_status: Vec<String>,
    mcp_rx: Option<std_mpsc::Receiver<McpBoot>>,
    mcp_failures: Vec<String>,

    view: View,
    key_input: String,
    error: Option<String>,
    /// Success notes (green), e.g. "added X from GitHub".
    status: Option<String>,
    md_cache: CommonMarkCache,

    /// MCP servers found on this machine but not yet configured here.
    available: Vec<AvailableServer>,
    /// Theme currently painted, so a change repaints at once.
    applied_theme: harness_core::config::ThemeChoice,
    /// The Aura mark, loaded from the real artwork.
    logo: Option<egui::TextureHandle>,
    /// Popup window state for the plugin browser.
    show_browser: bool,
    right_panel: bool,
    file_tree: ui::filetree::FileTreeState,
    current_dir: std::path::PathBuf,
    catalog_query: String,
    /// Launch line typed in the browser's manual field.
    browser_line: String,
    /// Live online search results (registry + GitHub).
    online: ui::plugin_browser::OnlineResults,
    /// Receives finished online searches: (query, entries, warning).
    online_rx: Option<
        std_mpsc::Receiver<(
            String,
            Vec<harness_core::online::OnlineEntry>,
            Option<String>,
        )>,
    >,
    /// When the query last changed, for the search debounce.
    online_dirty_since: Option<std::time::Instant>,
    /// Receives the downloaded agenttool.sh directory.
    directory_rx: Option<std_mpsc::Receiver<Vec<harness_core::online::OnlineEntry>>>,
    /// The agenttool.sh directory, downloaded once at startup.
    directory: Vec<harness_core::online::OnlineEntry>,
    /// Receives GitHub README analyses for paste-to-install.
    analyze_rx: Option<std_mpsc::Receiver<Result<harness_core::online::DerivedPlugin, String>>>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");

        // First run: drop a commented config + plugins file to edit.
        let _ = config::ensure_templates();

        let config = Config::load().unwrap_or_default();
        theme::apply(&cc.egui_ctx, config.theme);
        let plugins = config::with_default_plugins(Plugins::load().unwrap_or_default());
        let api_key = config::load_api_key();
        let sessions = Session::list_all().unwrap_or_default();

        let model = config.model;
        let effort = config.reasoning_effort;
        let theme_choice = config.theme;
        let key_input = api_key.clone().unwrap_or_default();

        let mut app = Self {
            rt: runtime,
            http: reqwest::Client::new(),
            config,
            plugins,
            api_key,
            sessions,
            session: None,
            draft: String::new(),
            attachments: Vec::new(),
            model,
            effort,
            live: LiveTurn::default(),
            cards: Vec::new(),
            busy: false,
            events: None,
            approvals: None,
            registry: ToolRegistry::new(),
            mcp: Arc::new(Mutex::new(BTreeMap::new())),
            mcp_status: Vec::new(),
            mcp_rx: None,
            mcp_failures: Vec::new(),
            view: if key_is_set(&key_input) {
                View::Chat
            } else {
                View::Settings
            },
            key_input,
            error: None,
            status: None,
            md_cache: CommonMarkCache::default(),
            available: Vec::new(),
            applied_theme: theme_choice,
            logo: load_logo(&cc.egui_ctx),
            show_browser: false,
            right_panel: true,
            file_tree: ui::filetree::FileTreeState::new("."),
            current_dir: std::path::PathBuf::from("."),
            catalog_query: String::new(),
            browser_line: String::new(),
            online: ui::plugin_browser::OnlineResults::default(),
            online_rx: None,
            online_dirty_since: None,
            directory_rx: None,
            directory: Vec::new(),
            analyze_rx: None,
        };
        app.refresh_available();

        app.registry = tools::from_plugins(&app.plugins);
        app.boot_mcp();
        app.boot_directory();
        app
    }

    // ---- sessions ---------------------------------------------------------

    /// Start a fresh conversation, saving whatever we had.
    fn new_session(&mut self) {
        self.persist();
        self.session = Some(Session::new("New chat"));
        self.live.clear();
        self.cards.clear();
        self.error = None;
    }

    fn open_session(&mut self, id: &str) {
        self.persist();
        let path = sessions_dir().join(format!("{id}.jsonl"));
        match Session::load(&path) {
            Ok(s) => {
                self.session = Some(s);
                self.live.clear();
                self.cards.clear();
            }
            Err(e) => self.error = Some(format!("could not open conversation: {e}")),
        }
    }

    fn delete_session(&mut self, id: &str) {
        let path = sessions_dir().join(format!("{id}.jsonl"));
        if let Err(e) = std::fs::remove_file(&path) {
            self.error = Some(format!("could not delete conversation: {e}"));
        }
        if self.session.as_ref().map(|s| s.header.id.as_str()) == Some(id) {
            self.session = None;
        }
        self.refresh_sessions();
    }

    fn refresh_sessions(&mut self) {
        self.sessions = Session::list_all().unwrap_or_default();
    }

    fn persist(&mut self) {
        if let Some(session) = &self.session {
            if let Err(e) = session.save() {
                self.error = Some(format!("could not save conversation: {e}"));
            }
            self.refresh_sessions();
        }
    }

    // ---- plugins ----------------------------------------------------------

    /// Connect MCP servers off-thread and report tools back to the UI.
    fn boot_mcp(&mut self) {
        let servers = self.plugins.mcp.clone();
        let conns = Arc::clone(&self.mcp);
        let (tx, rx) = std_mpsc::channel();
        self.mcp_rx = Some(rx);

        self.rt.spawn(async move {
            let (mut connected, failures) = mcp::connect_all(&servers).await;
            let live: Vec<String> = connected.keys().cloned().collect();
            let registered = mcp::collect_registered(&mut connected).await;

            let status: Vec<String> = servers
                .iter()
                .map(|(name, server)| {
                    ui::settings::mcp_line(name, server.enabled, live.contains(name))
                })
                .collect();

            *conns.lock().await = connected;

            let _ = tx.send(McpBoot {
                tools: registered,
                status,
                failures,
            });
        });
    }

    /// Fold a completed boot pass into the live registry.
    fn absorb_mcp(&mut self) {
        let Some(rx) = self.mcp_rx.as_ref() else { return };
        let Ok(boot) = rx.try_recv() else { return };
        self.mcp_rx = None;

        // Rebuild so a reconnect drops tools that disappeared.
        self.registry = tools::from_plugins(&self.plugins);
        for tool in boot.tools {
            self.registry.register(tool);
        }
        self.mcp_status = boot.status;
        self.mcp_failures = boot.failures;
    }

    fn reconnect(&mut self) {
        self.registry = tools::from_plugins(&self.plugins);
        self.boot_mcp();
    }

    /// Download the agenttool.sh directory once, in the background.
    fn boot_directory(&mut self) {
        let http = self.http.clone();
        let (tx, rx) = std_mpsc::channel();
        self.directory_rx = Some(rx);
        self.rt.spawn(async move {
            let entries = harness_core::online::fetch_directory(&http).await;
            let _ = tx.send(entries);
        });
    }

    /// Kick an online search when the query has settled (600 ms debounce).
    fn pump_online_search(&mut self) {
        // Drain any finished search.
        if let Some(rx) = self.online_rx.as_ref() {
            if let Ok((query, entries, warning)) = rx.try_recv() {
                self.online_rx = None;
                self.online.busy = false;
                self.online.query = query;
                self.online.entries = entries;
                self.online.warning = warning;
            }
        }

        let q = self.catalog_query.trim().to_string();
        if q.len() < 2 || self.online.busy {
            self.online_dirty_since = None;
            return;
        }
        // Already showing results for this exact query.
        if self.online.query.eq_ignore_ascii_case(&q) && self.online_rx.is_none() {
            self.online_dirty_since = None;
            return;
        }

        match self.online_dirty_since {
            None => self.online_dirty_since = Some(std::time::Instant::now()),
            Some(since) if since.elapsed() >= Duration::from_millis(600) => {
                self.online_dirty_since = None;
                let http = self.http.clone();
                let query = q.clone();
                let (tx, rx) = std_mpsc::channel();
                self.online_rx = Some(rx);
                self.online.busy = true;
                self.rt.spawn(async move {
                    let (entries, warning) = harness_core::online::search(&query, &http).await;
                    let _ = tx.send((query, entries, warning));
                });
            }
            Some(_) => {} // still settling
        }
    }

    /// Drain the directory download when it lands.
    fn pump_directory(&mut self) {
        if let Some(rx) = self.directory_rx.as_ref() {
            if let Ok(entries) = rx.try_recv() {
                self.directory_rx = None;
                self.directory = entries;
            }
        }
    }

    /// Drain a GitHub README analysis; auto-add on success.
    fn pump_analyze(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.analyze_rx.as_ref() else { return };
        let Ok(result) = rx.try_recv() else { return };
        self.analyze_rx = None;
        match result {
            Ok(derived) => {
                self.status = Some(format!(
                    "added “{}” from {} — {}",
                    derived.name, derived.repo_url, derived.launch_line
                ));
                self.add_mcp_server(&derived.name, &derived.launch_line);
                self.browser_line.clear();
            }
            Err(e) => self.error = Some(format!("GitHub analyze: {e}")),
        }
        ctx.request_repaint();
    }

    /// Analyze a pasted GitHub link, then add what the README offers.
    fn start_analyze(&mut self, url: &str) {
        let http = self.http.clone();
        let url = url.to_string();
        let (tx, rx) = std_mpsc::channel();
        self.analyze_rx = Some(rx);
        self.rt.spawn(async move {
            let result = harness_core::online::analyze_github(&url, &http).await;
            let _ = tx.send(result);
        });
    }

    /// Add (or replace) an MCP server from a launch line, persist, reconnect.
    fn add_mcp_server(&mut self, name: &str, line: &str) {
        let mut argv = config::split_args(line);
        if argv.is_empty() {
            self.error = Some("a plugin needs a command to run".into());
            return;
        }
        let command = argv.remove(0);
        let name = config::normalize_name(name);
        self.plugins.mcp.insert(
            name,
            config::McpServer {
                command,
                args: argv,
                env: Default::default(),
                enabled: true,
            },
        );
        self.browser_line.clear();
        match self.plugins.save() {
            Ok(()) => self.error = None,
            Err(e) => self.error = Some(format!("could not save plugins: {e}")),
        }
        self.refresh_available();
        self.reconnect();
    }

    /// Servers already configured for other Aura agents, minus the ones here.
    fn refresh_available(&mut self) {
        self.available = config::aura_code_servers()
            .into_iter()
            .filter(|(name, _)| !self.plugins.mcp.contains_key(name))
            .map(|(name, server)| AvailableServer {
                name,
                command: server.command,
                args: server.args,
            })
            .collect();
    }

    fn plugin_status_line(&self) -> String {
        let mcp_up = self
            .mcp_status
            .iter()
            .filter(|l| l.ends_with("connected"))
            .count();
        let mcp_all = self.mcp_status.len();
        format!("{} tools · {mcp_up}/{mcp_all} MCP servers", self.registry.len())
    }

    // ---- sending ----------------------------------------------------------

    fn send(&mut self, ctx: &egui::Context) {
        if self.busy || !ui::input::is_sendable(&self.draft, self.attachments.len()) {
            return;
        }
        // Resolve provider key from the selected model_id, falling back to
        // the stored key. Makes the app model-agnostic, not DeepSeek-only.
        let resolved_key = if !self.config.model_id.trim().is_empty() {
            harness_core::provider::get_api_key_for_model(self.config.model_id.trim())
        } else {
            self.api_key.clone()
        };
        let Some(key) = resolved_key else {
            self.error = Some("add an API key for your selected model in Settings (or set AURA_HOME / env vars)".into());
            self.view = View::Settings;
            return;
        };

        let base_url = if !self.config.model_id.trim().is_empty() {
            harness_core::provider::base_url_for(self.config.model_id.trim())
        } else {
            None
        };

        let mut text = self.draft.trim().to_string();
        if !self.attachments.is_empty() {
            text.push_str(&attach::render_into_prompt(&self.attachments));
        }

        let title = if self.draft.trim().is_empty() {
            "Attached files".to_string()
        } else {
            title_from(&self.draft)
        };
        let session = self.session.get_or_insert_with(|| Session::new(title));
        session.messages.push(Message::user(text));

        self.draft.clear();
        self.attachments.clear();
        self.live.clear();
        self.cards.clear();
        self.error = None;

        // Build the request history while the session borrow is still live.
        let mut history = Vec::new();
        if !self.config.system_prompt.trim().is_empty() {
            history.push(Message::system(self.config.system_prompt.trim()));
        }
        history.extend(session.messages.iter().cloned());

        self.persist();

        let opts = ChatOptions {
            model: if self.config.model_id.trim().is_empty() {
                Some(self.model.into())
            } else {
                None
            },
            model_name: if self.config.model_id.trim().is_empty() {
                None
            } else {
                Some(self.config.model_id.trim().to_string())
            },
            base_url,
            effort: Some(self.effort.into()),
            ..Default::default()
        };

        let (appr_tx, appr_rx) = agent::approval_channel();
        let input = LoopInput {
            http: self.http.clone(),
            api_key: key,
            history,
            opts,
            registry: self.registry.clone(),
            mcp: Arc::clone(&self.mcp),
            approvals: appr_rx,
        };

        let (tx, rx) = tokio::sync::mpsc::channel(256);
        self.rt.spawn(async move {
            let _ = agent::run(input, tx).await;
        });

        self.events = Some(rx);
        self.approvals = Some(appr_tx);
        self.busy = true;
        ctx.request_repaint();
    }

    /// Abort the in-flight turn: dropping the channels unwinds the agent loop.
    fn stop(&mut self) {
        self.events = None;
        self.approvals = None;
        self.busy = false;
        self.live.clear();
    }

    // ---- event pump -------------------------------------------------------

    fn drain_events(&mut self, ctx: &egui::Context) {
        let mut finished = false;
        let mut batch = Vec::new();

        // Collect first so the receiver borrow ends before we touch `self`.
        if let Some(rx) = self.events.as_mut() {
            loop {
                match rx.try_recv() {
                    Ok(event) => batch.push(event),
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
            ctx.request_repaint();
        }

        for event in batch {
            self.apply_event(event);
        }
        if finished {
            self.events = None;
            self.approvals = None;
            self.busy = false;
            self.live.freeze_reasoning_clock();
            self.persist();
        }
    }

    fn apply_event(&mut self, event: LoopEvent) {
        match event {
            LoopEvent::Reasoning(text) => {
                self.live.touch_reasoning();
                self.live.reasoning.push_str(&text);
            }
            LoopEvent::Content(text) => self.live.content.push_str(&text),
            LoopEvent::ToolRequested {
                call_id,
                name,
                summary,
                needs_approval,
            } => {
                self.cards
                    .push(ToolCard::new(call_id, name, summary, needs_approval));
            }
            LoopEvent::ToolFinished {
                call_id,
                name,
                output,
                is_error,
            } => {
                let shown = agent::truncate_for_display(&output, 4000);
                if let Some(card) = self.cards.iter_mut().find(|c| c.call_id == call_id) {
                    card.output = Some(shown);
                    card.is_error = is_error;
                } else {
                    let mut card = ToolCard::new(call_id.clone(), name, String::new(), false);
                    card.output = Some(shown);
                    card.is_error = is_error;
                    self.cards.push(card);
                }
                // Mirror the tool result into the transcript so the next
                // request carries the full call/result pair.
                if let Some(session) = self.session.as_mut() {
                    session.messages.push(Message::tool_result(call_id, output));
                }
                self.persist();
            }
            LoopEvent::TurnComplete(message) => {
                if let Some(session) = self.session.as_mut() {
                    session.messages.push(message);
                }
                self.live.clear();
                self.persist();
            }
            LoopEvent::Finished { .. } => {
                self.busy = false;
                self.live.freeze_reasoning_clock();
            }
            LoopEvent::Failed(message) => {
                self.error = Some(message);
                self.busy = false;
            }
        }
    }

    fn decide(&mut self, call_id: &str, decision: Approval) {
        if let Some(card) = self.cards.iter_mut().find(|c| c.call_id == call_id) {
            card.decision = Some(decision.clone());
        }
        if let Some(tx) = self.approvals.as_mut() {
            let _ = tx.try_send(decision);
        }
    }

    // ---- settings actions -------------------------------------------------

    fn apply_settings(&mut self, action: SettingsAction, ctx: &egui::Context) {
        match action {
            SettingsAction::SaveKey(key) => {
                let trimmed = key.trim().to_string();
                if trimmed.is_empty() {
                    self.error = Some("the API key is empty".into());
                } else {
                    match config::store_api_key(&trimmed) {
                        Ok(()) => {
                            self.api_key = Some(trimmed.clone());
                            self.key_input = trimmed;
                            self.error = None;
                        }
                        Err(e) => self.error = Some(format!("could not save key: {e}")),
                    }
                }
            }
            SettingsAction::SaveConfig => {
                if let Err(e) = self.config.save() {
                    self.error = Some(format!("could not save settings: {e}"));
                }
            }
            SettingsAction::SavePlugins => {
                if let Err(e) = self.plugins.save() {
                    self.error = Some(format!("could not save plugins: {e}"));
                }
                self.reconnect();
            }
            SettingsAction::OpenBrowser => self.show_browser = true,
            SettingsAction::RemoveServer(name) => {
                self.plugins.mcp.remove(&name);
                if let Err(e) = self.plugins.save() {
                    self.error = Some(format!("could not save plugins: {e}"));
                }
                self.refresh_available();
                self.reconnect();
            }
            SettingsAction::Reconnect => self.reconnect(),
            SettingsAction::Close => self.view = View::Chat,
        }
        ctx.request_repaint();
    }

    fn handle_sidebar(&mut self, action: SidebarAction) {
        match action {
            // Choosing a conversation means you want to read it, so leave
            // Settings rather than making you close it first.
            SidebarAction::NewSession => {
                self.new_session();
                self.view = View::Chat;
            }
            SidebarAction::Open(id) => {
                self.open_session(&id);
                self.view = View::Chat;
            }
            SidebarAction::Delete(id) => self.delete_session(&id),
            SidebarAction::OpenSettings => self.view = View::Settings,
            SidebarAction::Theme(choice) => {
                self.config.theme = choice;
                self.applied_theme = choice;
                if let Err(e) = self.config.save() {
                    self.error = Some(format!("could not save settings: {e}"));
                }
            }
        }
    }

    /// Pick text files and read them as prompt context.
    fn attach_files(&mut self) {
        let Some(paths) = rfd::FileDialog::new().pick_files() else {
            return;
        };
        for path in paths {
            match attach::read_attachment(&path) {
                Ok(a) => self.attachments.push(a),
                Err(e) => self.error = Some(e.to_string()),
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events(ctx);
        self.absorb_mcp();
        self.pump_online_search();
        self.pump_directory();
        self.pump_analyze(ctx);

        // Repaint immediately when the theme picker moves, so it can be tried
        // before it is saved.
        if self.config.theme != self.applied_theme {
            self.applied_theme = self.config.theme;
            theme::apply(ctx, self.applied_theme);
        }

        let current_id = self.session.as_ref().map(|s| s.header.id.clone());
        let plugin_line = self.plugin_status_line();

        let mut sidebar_action = None;
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(230.0)
            .width_range(180.0..=340.0)
            .show(ctx, |ui| {
                sidebar_action = ui::sidebar::show(
                    ui,
                    &SidebarState {
                        sessions: &self.sessions,
                        current_id: current_id.as_deref(),
                        plugin_status: &plugin_line,
                        plugin_ok: self.mcp_failures.is_empty(),
                        mark: self.logo.as_ref(),
                        theme: self.config.theme,
                    },
                );
            });
        if let Some(action) = sidebar_action {
            self.handle_sidebar(action);
        }

        // Right-hand panel: file tree (attach files) + curated plugin browser.
        if self.right_panel {
            let mut close_right = false;
            let mut clicked_add: Option<(String, String)> = None;
            let mut clicked_file: Option<std::path::PathBuf> = None;
            let configured: Vec<String> = self.plugins.mcp.keys().cloned().collect();
            let mut query_edit = self.catalog_query.clone();
            egui::SidePanel::right("right-panel")
                .resizable(true)
                .default_width(270.0)
                .width_range(220.0..=420.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("📁 Files  •  🔌 Plugins");
                        if ui.small_button("✕").clicked() {
                            close_right = true;
                        }
                    });
                    ui.separator();
                    egui::CollapsingHeader::new("Project files")
                        .default_open(true)
                        .show(ui, |ui| {
                            if let Some(p) = self.file_tree.show(ui) {
                                clicked_file = Some(p);
                            }
                        });
                    ui.separator();
                    ui.label("Plugin search");
                    ui.text_edit_singleline(&mut query_edit);
                    if query_edit.trim().len() < 2 {
                        ui.small("Type to search (e.g. gaming, social media, 3d game, summertime)");
                    } else {
                        let hits = harness_core::catalog::search(&query_edit);
                        if hits.is_empty() {
                            ui.small("No matches.");
                        }
                        for e in hits {
                            if configured.contains(&harness_core::catalog::plugin_key(e)) {
                                continue;
                            }
                            ui.horizontal(|ui| {
                                ui.add(egui::Label::new(e.name).truncate());
                                if ui.small_button("Add").clicked() {
                                    clicked_add = Some((e.name.to_string(), e.launch_line.to_string()));
                                }
                            });
                        }
                    }
                });
            self.catalog_query = query_edit;
            if close_right {
                self.right_panel = false;
            }
            if let Some(p) = clicked_file {
                let hint = format!("@{}", p.display());
                if self.draft.is_empty() {
                    self.draft.push_str(&hint);
                } else {
                    self.draft.push_str(&format!(" {}", hint));
                }
            }
            if let Some((n, l)) = clicked_add {
                self.add_mcp_server(&n, &l);
            }
        }

        // Composer (hidden in settings so the form gets the room).
        if self.view == View::Chat {
            let mut composer_action = None;
            egui::TopBottomPanel::bottom("composer").show(ctx, |ui| {
                ui.add_space(4.0);
                composer_action = ui::input::show(
                    ui,
                    &mut ComposerState {
                        draft: &mut self.draft,
                        attachments: &self.attachments,
                        model: &mut self.model,
                        effort: &mut self.effort,
                        busy: self.busy,
                        tool_count: self.registry.len(),
                    },
                );
                if let Some(err) = &self.error {
                    ui.colored_label(theme::danger(), err);
                }
                ui.add_space(4.0);
            });

            match composer_action {
                Some(ComposerAction::Send) => self.send(ctx),
                Some(ComposerAction::Stop) => self.stop(),
                Some(ComposerAction::Attach) => self.attach_files(),
                Some(ComposerAction::RemoveAttachment(i)) => {
                    if i < self.attachments.len() {
                        self.attachments.remove(i);
                    }
                }
                None => {}
            }
        }

        let mut settings_action = None;
        let mut chat_action = None;
        egui::CentralPanel::default().show(ctx, |ui| match self.view {
            View::Chat => {
                let messages: &[Message] = self
                    .session
                    .as_ref()
                    .map(|s| s.messages.as_slice())
                    .unwrap_or(&[]);
                chat_action = ui::chat::show(
                    ui,
                    &mut ChatState {
                        messages,
                        live: &self.live,
                        cards: &self.cards,
                        busy: self.busy,
                        cache: &mut self.md_cache,
                    },
                );
            }
            View::Settings => {
                let cmd_tools: Vec<String> = self
                    .plugins
                    .cmd
                    .iter()
                    .map(|(name, tool)| format!("{name} — {}", tool.command))
                    .collect();
                settings_action = ui::settings::show(
                    ui,
                    &mut SettingsState {
                        config: &mut self.config,
                        plugins: &mut self.plugins,
                        key_input: &mut self.key_input,
                        key_present: self.api_key.is_some(),
                        mcp_status: &self.mcp_status,
                        cmd_tools: &cmd_tools,
                        tool_count: self.registry.len(),
                    },
                );
            }
        });

        match chat_action {
            Some(ChatAction::Approve(id)) => self.decide(&id, Approval::Approve),
            Some(ChatAction::Reject(id)) => self.decide(&id, Approval::Reject),
            None => {}
        }
        if let Some(action) = settings_action {
            self.apply_settings(action, ctx);
        }

        // Plugin browser: a searchable window over catalogue + machine.
        if self.show_browser {
            let mut browser_action = None;
            let configured: Vec<String> = self.plugins.mcp.keys().cloned().collect();
            egui::Window::new("Add a plugin")
                .id(egui::Id::new("plugin-browser"))
                .default_width(520.0)
                .resizable(true)
                .open(&mut self.show_browser)
                .show(ctx, |ui| {
                    if let Some(note) = &self.status {
                        ui.label(egui::RichText::new(note).color(theme::ok()).small());
                    }
                    if let Some(err) = &self.error {
                        ui.label(egui::RichText::new(err).color(theme::danger()).small());
                    }
                    browser_action = ui::plugin_browser::show(
                        ui,
                        &mut BrowserState {
                            query: &mut self.catalog_query,
                            line: &mut self.browser_line,
                            available: &self.available,
                            configured: &configured,
                            online: &self.online,
                            directory: &self.directory,
                        },
                    );
                });
            match browser_action {
                Some(BrowserAction::Add { name, line }) => {
                    self.add_mcp_server(&name, &line);
                    // Keep the window open so more can be chosen.
                    self.show_browser = true;
                }
                Some(BrowserAction::Analyze(url)) => {
                    self.start_analyze(&url);
                    self.show_browser = true;
                }
                Some(BrowserAction::Open(url)) => {
                    ctx.open_url(egui::OpenUrl::new_tab(url));
                }
                Some(BrowserAction::Close) => self.show_browser = false,
                None => {}
            }
        }

        if self.busy {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
}

/// A session title drawn from the first user message.
fn title_from(text: &str) -> String {
    let first_line = text.lines().next().unwrap_or("").trim();
    let mut title: String = first_line.chars().take(48).collect();
    if first_line.chars().count() > 48 {
        title.push('…');
    }
    if title.is_empty() {
        "New chat".to_string()
    } else {
        title
    }
}

fn key_is_set(key: &str) -> bool {
    !key.trim().is_empty()
}

/// Decode the Aura mark from the real artwork, once.
fn load_logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let bytes = include_bytes!("../../../assets/mark.png");
    let image = egui_extras::image::load_image_bytes(bytes).ok()?;
    Some(ctx.load_texture("aura-mark", image, egui::TextureOptions::LINEAR))
}

fn window_icon() -> egui::IconData {
    egui::IconData {
        rgba: ICON_RGBA.to_vec(),
        width: 64,
        height: 64,
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1320.0, 820.0])
            .with_min_inner_size([1024.0, 680.0])
            .with_min_inner_size([760.0, 520.0])
            .with_title("Aura Harness")
            .with_icon(window_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "Aura Harness",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_come_from_the_first_line() {
        assert_eq!(title_from("hello there\nsecond"), "hello there");
        assert_eq!(title_from("   "), "New chat");
        let long = "x".repeat(60);
        let t = title_from(&long);
        assert!(t.ends_with('…'));
        assert_eq!(t.chars().count(), 49);
    }

    #[test]
    fn key_detection_ignores_whitespace() {
        assert!(!key_is_set("   "));
        assert!(key_is_set("sk-abc"));
    }

    #[test]
    fn embedded_icon_is_64x64_rgba() {
        assert_eq!(ICON_RGBA.len(), 64 * 64 * 4);
    }
}
