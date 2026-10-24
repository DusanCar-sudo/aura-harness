//! Aura Harness — the DeepSeek harness as a standalone desktop app.
//!
//! This crate owns all disk I/O (config, credentials, sessions) and the tokio
//! runtime; `harness-core` stays GUI-free and testable.

mod markdown;
mod preview;
mod theme;
mod layout;
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

use ui::chat::LiveTurn;
use ui::settings::{AvailableServer, SettingsAction};
use ui::sidebar::SidebarAction;
use ui::{RightTab, ToolCard};

/// Window icon, embedded so the binary is self-contained.
const ICON_RGBA: &[u8] = include_bytes!("../../../assets/icon.rgba");

/// Result of an MCP boot pass, delivered back to the UI thread.
struct McpBoot {
    tools: Vec<RegisteredTool>,
    status: Vec<String>,
    failures: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum View {
    Chat,
    Settings,
}

/// A finished online search delivered on a channel: (query, entries, warning).
type OnlineResult = (String, Vec<harness_core::online::OnlineEntry>, Option<String>);

/// A provider's live model answer: (provider id, models or error).
type ModelFetchResult = (
    String,
    Result<Vec<harness_core::models::ModelInfo>, String>,
);

pub(crate) struct App {
    pub(crate) rt: tokio::runtime::Runtime,
    pub(crate) http: reqwest::Client,

    pub(crate) config: Config,
    pub(crate) plugins: Plugins,
    pub(crate) api_key: Option<String>,

    pub(crate) sessions: Vec<SessionHeader>,
    pub(crate) session: Option<Session>,

    pub(crate) draft: String,
    /// One-shot: the composer grabs keyboard focus on the next frame.
    pub(crate) focus_composer: bool,
    pub(crate) attachments: Vec<Attachment>,
    pub(crate) model: ModelChoice,
    pub(crate) effort: EffortChoice,

    /// Full provider → model factory, opened from the composer or Settings.
    pub(crate) model_factory: ui::model_factory::ModelFactoryState,
    pub(crate) model_fetch_rx: Option<std_mpsc::Receiver<ModelFetchResult>>,

    /// Voice: microphone capture, transcription and speech playback.
    pub(crate) voice_status: ui::input::VoiceStatus,
    pub(crate) recorder: Option<harness_core::voice::Recorder>,
    pub(crate) stt_rx: Option<std_mpsc::Receiver<Result<String, String>>>,
    pub(crate) voice_result_rx: Option<std_mpsc::Receiver<Result<(), String>>>,
    pub(crate) speaking: bool,

    pub(crate) live: LiveTurn,
    pub(crate) cards: Vec<ToolCard>,
    pub(crate) busy: bool,
    pub(crate) events: Option<tokio::sync::mpsc::Receiver<LoopEvent>>,
    pub(crate) approvals: Option<tokio::sync::mpsc::Sender<Approval>>,

    pub(crate) registry: ToolRegistry,
    pub(crate) mcp: Arc<Mutex<BTreeMap<String, McpConnection>>>,
    pub(crate) mcp_status: Vec<String>,
    pub(crate) mcp_rx: Option<std_mpsc::Receiver<McpBoot>>,
    pub(crate) mcp_failures: Vec<String>,

    pub(crate) view: View,
    pub(crate) key_input: String,
    pub(crate) error: Option<String>,
    /// Success notes (green), e.g. "added X from GitHub".
    pub(crate) status: Option<String>,
    pub(crate) md_cache: CommonMarkCache,
    /// Decoded attachment thumbnails shown in the transcript.
    pub(crate) image_cache: ui::chat::ImageCache,

    /// MCP servers found on this machine but not yet configured here.
    pub(crate) available: Vec<AvailableServer>,
    /// Theme currently painted, so a change repaints at once.
    pub(crate) applied_theme: harness_core::config::ThemeChoice,
    /// The Aura mark, loaded from the real artwork.
    pub(crate) logo: Option<egui::TextureHandle>,
    pub(crate) bg_loaded_path: String,
    pub(crate) bg_texture: Option<egui::TextureHandle>,
    /// True while panels are blended over the background image.
    pub(crate) bg_blend_active: bool,
    /// Last background values written to disk.
    pub(crate) bg_saved_image: String,
    pub(crate) bg_saved_opacity: f32,
    pub(crate) bg_saved_auto_approve: bool,
    /// Popup window state for the plugin browser.
    pub(crate) show_browser: bool,
    /// Right-click rename: the conversation being renamed and its edit buffer.
    pub(crate) renaming: Option<String>,
    pub(crate) rename_buf: String,
    pub(crate) right_panel: bool,
    pub(crate) right_tab: RightTab,
    pub(crate) file_tree: ui::filetree::FileTreeState,
    pub(crate) preview: ui::preview::PreviewState,
    pub(crate) catalog_query: String,
    /// Launch line typed in the browser's manual field.
    pub(crate) browser_line: String,
    /// Live online search results (registry + GitHub).
    pub(crate) online: ui::plugin_browser::OnlineResults,
    /// Receives finished online searches: (query, entries, warning).
    pub(crate) online_rx: Option<std_mpsc::Receiver<OnlineResult>>,
    /// When the query last changed, for the search debounce.
    pub(crate) online_dirty_since: Option<std::time::Instant>,
    /// Receives the downloaded agenttool.sh directory.
    pub(crate) directory_rx: Option<std_mpsc::Receiver<Vec<harness_core::online::OnlineEntry>>>,
    /// The agenttool.sh directory, downloaded once at startup.
    pub(crate) directory: Vec<harness_core::online::OnlineEntry>,
    /// Receives GitHub README analyses for paste-to-install.
    pub(crate) analyze_rx: Option<std_mpsc::Receiver<Result<harness_core::online::DerivedPlugin, String>>>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");

        // First run: drop a commented config + plugins file to edit.
        let _ = config::ensure_templates();

        // Older builds created every chat as "New chat" and never titled it
        // from the first prompt. Repair those sessions once, on launch.
        repair_untitled_sessions();

        let config = Config::load().unwrap_or_default();
        let auto_approve = config.auto_approve;
        let project_root = config.project_root.trim().to_string();
        let show_hidden = config.show_hidden_files;
        let right_tab = match config.right_panel_tab.trim() {
            "preview" => RightTab::Preview,
            "plugins" => RightTab::Plugins,
            _ => RightTab::Files,
        };
        theme::apply(&cc.egui_ctx, config.theme);
        let plugins = config::with_default_plugins(Plugins::load().unwrap_or_default());
        let api_key = config::load_api_key();
        let sessions = Session::list_all().unwrap_or_default();

        let model = config.model;
        let effort = config.reasoning_effort;
        let theme_choice = config.theme;
        let key_input = api_key.clone().unwrap_or_default();

        // Optional deep link used by scripts and screenshot checks:
        // AURA_HARNESS_OPEN_SESSION=<session id> opens that chat at launch.
        let initial_session = std::env::var("AURA_HARNESS_OPEN_SESSION")
            .ok()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .and_then(|id| {
                Session::load(&sessions_dir().join(format!("{id}.jsonl"))).ok()
            });

        let mut app = Self {
            rt: runtime,
            http: reqwest::Client::new(),
            config,
            plugins,
            api_key,
            sessions,
            session: initial_session,
            draft: String::new(),
            focus_composer: true,
            attachments: Vec::new(),
            model,
            effort,
            model_factory: ui::model_factory::ModelFactoryState::default(),
            model_fetch_rx: None,
            voice_status: ui::input::VoiceStatus::Idle,
            recorder: None,
            stt_rx: None,
            voice_result_rx: None,
            speaking: false,
            live: LiveTurn::default(),
            cards: Vec::new(),
            busy: false,
            events: None,
            approvals: None,
            registry: ToolRegistry::new(auto_approve),
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
            image_cache: ui::chat::ImageCache::new(),
            available: Vec::new(),
            applied_theme: theme_choice,
            logo: load_logo(&cc.egui_ctx),
            bg_loaded_path: String::new(),
            bg_texture: None,
            bg_blend_active: false,
            bg_saved_image: String::new(),
            bg_saved_opacity: -1.0,
            bg_saved_auto_approve: false,
            show_browser: false,
            renaming: None,
            rename_buf: String::new(),
            right_panel: true,
            right_tab,
            file_tree: ui::filetree::FileTreeState::new(if project_root.is_empty() {
                std::path::PathBuf::from(".")
            } else {
                std::path::PathBuf::from(project_root)
            })
            .with_hidden(show_hidden),
            preview: ui::preview::PreviewState::new(),
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

        app.registry = tools::from_plugins(&app.plugins, app.config.auto_approve);
        app.boot_mcp();
        app.boot_directory();
        app
    }

    // ---- sessions ---------------------------------------------------------

    /// Start a fresh conversation. If a turn is still streaming, stop it first
    /// so its events cannot land in the new chat; the partial answer is kept
    /// in the old conversation.
    fn new_session(&mut self) {
        // Clicking + twice should not pile up identical empty chats.
        let already_blank = self
            .session
            .as_ref()
            .is_some_and(|s| s.messages.is_empty() && s.header.title.trim() == "New chat");
        if already_blank {
            self.stop();
            self.live.clear();
            self.cards.clear();
            self.error = None;
            self.focus_composer = true;
            return;
        }

        // Unconditional: a turn can be between `Finished` and `TurnComplete`,
        // and its last event must not land in the new chat.
        self.stop();

        let fresh = Session::new("New chat");
        // Save it at once so it appears in the sidebar; the title becomes the
        // first prompt when the user sends.
        let _ = fresh.save();
        self.session = Some(fresh);
        self.refresh_sessions();
        self.live.clear();
        self.cards.clear();
        self.error = None;
        self.focus_composer = true;
    }

    fn open_session(&mut self, id: &str) {
        // Switching chats must not leave the old stream feeding this one.
        self.stop();
        let path = sessions_dir().join(format!("{id}.jsonl"));
        match Session::load(&path) {
            Ok(s) => {
                self.session = Some(s);
                self.live.clear();
                self.cards.clear();
                self.error = None;
            }
            Err(e) => self.error = Some(format!("could not open conversation: {e}")),
        }
    }

    fn delete_session(&mut self, id: &str) {
        if self.session.as_ref().map(|s| s.header.id.as_str()) == Some(id) {
            self.stop();
        }
        let path = sessions_dir().join(format!("{id}.jsonl"));
        if let Err(e) = std::fs::remove_file(&path) {
            self.error = Some(format!("could not delete conversation: {e}"));
        }
        if self.session.as_ref().map(|s| s.header.id.as_str()) == Some(id) {
            self.session = None;
        }
        self.refresh_sessions();
    }

    /// Take the edited title out of the buffer, leaving it clean for next time.
    fn take_rename(&mut self, _id: &str) -> Option<String> {
        let title = self.rename_buf.trim().to_string();
        self.rename_buf = String::new();
        Some(title)
    }

    /// Rename a conversation on disk (header line) and, if open, in memory.
    fn rename_session(&mut self, id: &str, title: &str) {
        let title = title.trim();
        if title.is_empty() {
            return;
        }
        let path = sessions_dir().join(format!("{id}.jsonl"));
        match Session::load(&path) {
            Ok(mut session) => {
                if let Err(e) = session.set_title(title).and_then(|_| session.save()) {
                    self.error = Some(format!("could not rename conversation: {e}"));
                }
            }
            Err(e) => self.error = Some(format!("could not rename conversation: {e}")),
        }
        if self.session.as_ref().map(|s| s.header.id.as_str()) == Some(id) {
            if let Some(open) = self.session.as_mut() {
                let _ = open.set_title(title);
            }
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
        self.registry = tools::from_plugins(&self.plugins, self.config.auto_approve);
        for tool in boot.tools {
            self.registry.register(tool);
        }
        self.mcp_status = boot.status;
        self.mcp_failures = boot.failures;
    }

    fn reconnect(&mut self) {
        self.registry = tools::from_plugins(&self.plugins, self.config.auto_approve);
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

    // ---- model factory ----------------------------------------------------

    /// Open the factory on the provider that owns the current model, and
    /// start a live fetch when that provider is ready and not cached.
    fn open_model_factory(&mut self) {
        let model_id = if self.config.model_id.trim().is_empty() {
            "deepseek-flash".to_string()
        } else {
            self.config.model_id.trim().to_string()
        };
        self.model_factory.open_for(&model_id);
        self.maybe_fetch_selected_model_provider();
    }

    fn maybe_fetch_selected_model_provider(&mut self) {
        let id = self.model_factory.selected_provider.clone();
        let Some(spec) = harness_core::models::provider(&id) else { return };
        if spec.live
            && harness_core::models::is_configured(spec)
            && !self.model_factory.live.contains_key(&id)
            && self.model_factory.fetching.is_none()
        {
            self.start_model_fetch(&id);
        }
    }

    fn start_model_fetch(&mut self, provider_id: &str) {
        let Some(spec) = harness_core::models::provider(provider_id) else { return };
        if !spec.live || self.model_factory.fetching.is_some() {
            return;
        }
        let http = self.http.clone();
        let id = provider_id.to_string();
        let (tx, rx) = std_mpsc::channel();
        self.model_fetch_rx = Some(rx);
        self.model_factory.fetching = Some(id.clone());
        self.model_factory.error = None;
        self.rt.spawn(async move {
            let result = harness_core::models::fetch_live_models(&http, spec).await;
            let _ = tx.send((id, result));
        });
    }

    fn pump_model_fetch(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.model_fetch_rx.as_ref() else { return };
        let Ok((id, result)) = rx.try_recv() else { return };
        self.model_fetch_rx = None;
        self.model_factory.fetching = None;
        match result {
            Ok(models) => {
                let count = models.len();
                self.model_factory.live.insert(id.clone(), models);
                self.status = Some(format!("{count} live models loaded from {id}"));
            }
            Err(e) => {
                self.model_factory.error = Some(format!("{id}: {e}"));
            }
        }
        ctx.request_repaint();
    }

    fn apply_model_factory(&mut self, action: ui::model_factory::ModelFactoryAction, ctx: &egui::Context) {
        match action {
            ui::model_factory::ModelFactoryAction::Select(model) => {
                self.select_model(&model.id, &model.family);
                self.model_factory.open = false;
            }
            ui::model_factory::ModelFactoryAction::UseManual { provider, id } => {
                let id = route_manual_model_id(&provider, &id);
                self.select_model(&id, &provider);
                self.model_factory.open = false;
            }
            ui::model_factory::ModelFactoryAction::Refresh(provider) => {
                self.start_model_fetch(&provider);
            }
        }
        ctx.request_repaint();
    }

    fn select_model(&mut self, model_id: &str, family: &str) {
        self.config.model_id = model_id.to_string();
        self.config.base_url = harness_core::provider::base_url_for(model_id);
        // Keep the legacy DeepSeek enum in step so older config paths agree.
        self.config.model = if model_id.contains("v4-pro") {
            ModelChoice::V4Pro
        } else {
            ModelChoice::Flash
        };
        self.status = Some(format!(
            "Model: {}  ·  {}",
            harness_core::models::display_name(model_id),
            family
        ));
        self.model_factory.manual = model_id.to_string();
        self.error = None;
        if let Err(e) = self.config.save() {
            self.error = Some(format!("could not save the model choice: {e}"));
        }
    }

    // ---- voice ------------------------------------------------------------

    fn recording_secs(&self) -> Option<f32> {
        self.recorder.as_ref().map(|r| r.elapsed_secs())
    }

    fn toggle_mic(&mut self, ctx: &egui::Context) {
        match self.voice_status {
            ui::input::VoiceStatus::Idle => match harness_core::voice::Recorder::start() {
                Ok(recorder) => {
                    self.recorder = Some(recorder);
                    self.voice_status = ui::input::VoiceStatus::Recording;
                    self.error = None;
                }
                Err(e) => self.error = Some(format!("microphone: {e}")),
            },
            ui::input::VoiceStatus::Recording => {
                let Some(recorder) = self.recorder.take() else {
                    self.voice_status = ui::input::VoiceStatus::Idle;
                    return;
                };
                match recorder.stop() {
                    Ok(wav) => {
                        self.voice_status = ui::input::VoiceStatus::Transcribing;
                        self.status = Some("Transcribing…".into());
                        let http = self.http.clone();
                        let (tx, rx) = std_mpsc::channel();
                        self.stt_rx = Some(rx);
                        self.rt.spawn(async move {
                            let result = harness_core::voice::transcribe(&http, &wav)
                                .await
                                .map_err(|e| e.to_string());
                            let _ = tokio::fs::remove_file(&wav).await;
                            let _ = tx.send(result);
                        });
                    }
                    Err(e) => {
                        self.voice_status = ui::input::VoiceStatus::Idle;
                        self.error = Some(format!("recording: {e}"));
                    }
                }
            }
            ui::input::VoiceStatus::Transcribing => {}
        }
        ctx.request_repaint();
    }

    fn pump_voice(&mut self, ctx: &egui::Context) {
        if let Some(rx) = self.stt_rx.as_ref() {
            if let Ok(result) = rx.try_recv() {
                self.stt_rx = None;
                self.voice_status = ui::input::VoiceStatus::Idle;
                match result {
                    Ok(text) => {
                        if self.draft.chars().last().is_some_and(|c| !c.is_whitespace()) {
                            self.draft.push(' ');
                        }
                        self.draft.push_str(&text);
                        self.status = Some("Transcribed into the composer".into());
                        self.focus_composer = true;
                    }
                    Err(e) => self.error = Some(format!("speech to text: {e}")),
                }
            }
        }
        if let Some(rx) = self.voice_result_rx.as_ref() {
            if let Ok(result) = rx.try_recv() {
                self.voice_result_rx = None;
                self.speaking = false;
                match result {
                    Ok(()) => self.status = Some("Spoken".into()),
                    Err(e) => self.error = Some(format!("text to speech: {e}")),
                }
            }
        }
        if self.voice_status != ui::input::VoiceStatus::Idle || self.speaking {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
    }

    fn speak(&mut self, text: String, ctx: &egui::Context) {
        if text.trim().is_empty() || self.speaking {
            return;
        }
        self.speaking = true;
        self.status = Some("Generating speech…".into());
        let http = self.http.clone();
        let voice = self.config.tts_voice.clone();
        let (tx, rx) = std_mpsc::channel();
        self.voice_result_rx = Some(rx);
        self.rt.spawn(async move {
            let result = match harness_core::voice::synthesize(&http, &text, &voice).await {
                Ok(clip) => tokio::task::spawn_blocking(move || harness_core::voice::play_clip(&clip))
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.map_err(|e| e.to_string()))
                    .map(|_| ()),
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(result);
        });
        ctx.request_repaint();
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
        let images = attach::image_parts(&self.attachments);
        if !images.is_empty() {
            let model_id = self.current_model_id();
            if !harness_core::models::supports_images(&model_id) {
                self.error = Some(format!(
                    "{model_id} can't read images — switch to a vision model in Model, or remove the image"
                ));
                return;
            }
        }
        if !self.attachments.is_empty() {
            text.push_str(&attach::render_into_prompt(&self.attachments));
        }

        let title = if self.draft.trim().is_empty() {
            attach::title_for(&self.attachments)
        } else {
            title_from(&self.draft)
        };
        let session = self.session.get_or_insert_with(|| Session::new(title.clone()));
        // A chat created with "+ New chat" starts as "New chat"; give it a
        // real title from the first prompt instead of leaving every session
        // indistinguishable in the sidebar.
        if session.header.title.trim().is_empty() || session.header.title == "New chat" {
            session.header.title = title;
        }
        let mut turn = Message::user(text);
        turn.images = images;
        session.messages.push(turn);

        self.draft.clear();
        self.attachments.clear();
        self.live.clear();
        self.cards.clear();
        self.error = None;

        // Build the request history while the session borrow is still live.
        // The folder note is rebuilt every send (not stored in the session) so
        // it follows the file tree's root.
        let project_dir = absolute_dir(&self.file_tree.root);
        let mut history = Vec::new();
        let system = system_message(&self.config.system_prompt, &project_dir);
        if !system.is_empty() {
            history.push(Message::system(system));
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
        let mut registry = self.registry.clone();
        if project_dir_is_usable(&project_dir) {
            registry.set_cwd(&project_dir);
        }
        let input = LoopInput {
            http: self.http.clone(),
            api_key: key,
            history,
            opts,
            registry,
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
        self.focus_composer = true;
        ctx.request_repaint();
    }

    /// Abort the in-flight turn: dropping the channels unwinds the agent loop.
    /// Whatever has already streamed is saved as a partial assistant message
    /// in the conversation it belongs to — stopping should not erase the work.
    fn stop(&mut self) {
        if let Some(partial) = partial_assistant_message(&self.live) {
            if let Some(session) = self.session.as_mut() {
                session.messages.push(partial);
            }
        }
        self.events = None;
        self.approvals = None;
        self.busy = false;
        self.focus_composer = true;
        self.live.clear();
        self.persist();
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
            SettingsAction::OpenModelFactory => self.open_model_factory(),
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
                self.focus_composer = true;
            }
            SidebarAction::Open(id) => {
                self.open_session(&id);
                self.view = View::Chat;
                self.focus_composer = true;
            }
            SidebarAction::Delete(id) => self.delete_session(&id),
            SidebarAction::RenameRequest(id) => {
                self.rename_buf = self
                    .sessions
                    .iter()
                    .find(|h| h.id == id)
                    .map(|h| h.title.clone())
                    .unwrap_or_default();
                self.renaming = Some(id);
            }
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

    /// Drop one attachment. The composer only offers rows that exist, but a
    /// stale index must never panic the frame.
    fn remove_attachment(&mut self, i: usize) {
        if i < self.attachments.len() {
            self.attachments.remove(i);
        }
    }

    /// The model id the next send will use, after config override.
    fn current_model_id(&self) -> String {
        if self.config.model_id.trim().is_empty() {
            match self.model {
                ModelChoice::Flash => "deepseek-flash".to_string(),
                ModelChoice::V4Pro => "deepseek-v4-pro".to_string(),
            }
        } else {
            self.config.model_id.trim().to_string()
        }
    }

    /// Pick text or image files and read them as prompt context.
    ///
    /// `rfd` is built with the tokio backend, so the dialog must run on the
    /// app's tokio runtime. The synchronous `FileDialog::pick_files()` blocks
    /// on pollster without a reactor and panics — which the old
    /// `catch_unwind` turned into a silent no-op.
    fn attach_files(&mut self) {
        let picked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.rt.block_on(async {
                rfd::AsyncFileDialog::new()
                    .set_title("Attach text or images")
                    .pick_files()
                    .await
            })
        }));
        let handles = match picked {
            Ok(Some(handles)) => handles,
            Ok(None) => return,
            Err(_) => {
                self.error = Some("file dialog failed to open (desktop portal error)".into());
                return;
            }
        };
        let paths: Vec<std::path::PathBuf> =
            handles.iter().map(|h| h.path().to_path_buf()).collect();
        self.add_attachments(paths);
    }

    /// Read a batch of picked or dropped paths into the composer, skipping
    /// duplicates and reporting every rejection.
    pub(crate) fn add_attachments(&mut self, paths: Vec<std::path::PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let mut added = 0usize;
        let mut errors = Vec::new();
        for path in paths {
            let display = path.display().to_string();
            if self.attachments.iter().any(|a| a.path == display) {
                continue;
            }
            match attach::read_attachment(&path) {
                Ok(a) => {
                    self.attachments.push(a);
                    added += 1;
                }
                Err(e) => errors.push(e.to_string()),
            }
        }
        if errors.is_empty() {
            self.error = None;
            if added > 0 {
                self.status = Some(format!(
                    "attached {added} file{}",
                    if added == 1 { "" } else { "s" }
                ));
            }
        } else {
            self.status = None;
            self.error = Some(errors.join(" · "));
        }
    }

    /// Files dropped anywhere on the window join the composer.
    pub(crate) fn absorb_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if dropped.is_empty() {
            return;
        }
        if self.view != View::Chat {
            self.view = View::Chat;
        }
        self.add_attachments(dropped);
    }
}


/// The project folder as an absolute path (the file tree starts at "." when none is set).
fn absolute_dir(root: &std::path::Path) -> std::path::PathBuf {
    root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}

/// False for a saved project_root that has since been deleted or moved: the
/// tools then keep the process's own folder rather than fail every shell call.
fn project_dir_is_usable(dir: &std::path::Path) -> bool {
    dir.is_dir()
}

/// The user's system prompt plus where the tools run, so the model does not
/// spend tool rounds exploring to find the project. Empty when there is
/// neither a prompt nor a usable folder.
pub(crate) fn system_message(prompt: &str, root: &std::path::Path) -> String {
    if !project_dir_is_usable(root) {
        return prompt.trim().to_string();
    }
    let note = format!(
        "Working directory: {}. The shell starts there and relative file paths resolve against it. \
         A website's entry page should be index.html in that folder (or in dist/, out/ or \
         public/) so the Preview tab can show it.",
        absolute_dir(root).display()
    );
    match prompt.trim() {
        "" => note,
        p => format!("{p}\n\n{note}"),
    }
}

/// Turn the live streaming buffers into an assistant message, if anything has
/// arrived. Used when the user stops a turn or starts another chat.
pub(crate) fn partial_assistant_message(live: &LiveTurn) -> Option<Message> {
    if live.content.trim().is_empty() && live.reasoning.trim().is_empty() {
        return None;
    }
    let mut message = Message::user("");
    message.role = "assistant".into();
    message.content = live.content.clone();
    message.reasoning_content = live.reasoning.clone();
    Some(message)
}

/// Give legacy chats a real title from their first user message.
fn repair_untitled_sessions() {
    let Ok(headers) = Session::list_all() else { return };
    for header in headers {
        let title = header.title.trim();
        if !title.is_empty() && title != "New chat" {
            continue;
        }
        let path = sessions_dir().join(format!("{}.jsonl", header.id));
        let Ok(mut session) = Session::load(&path) else { continue };
        let Some(first) = session
            .messages
            .iter()
            .find(|m| m.role == "user" && !m.content.trim().is_empty())
        else {
            continue;
        };
        let title = title_from(&first.content);
        if title != "New chat" {
            session.header.title = title;
            let _ = session.save();
        }
    }
}

/// Give a hand-typed model id the routing prefix its provider expects,
/// unless it already carries one that routes to the same family.
fn route_manual_model_id(provider: &str, id: &str) -> String {
    let Some(spec) = harness_core::models::provider(provider) else {
        return id.to_string();
    };
    if spec.prefix.is_empty() || id.starts_with(spec.prefix) {
        return id.to_string();
    }
    if harness_core::provider::provider_family(id) == provider {
        return id.to_string();
    }
    format!("{}{}", spec.prefix, id)
}

/// A session title drawn from the first user message.
pub(crate) fn title_from(text: &str) -> String {
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

pub(crate) fn key_is_set(key: &str) -> bool {
    !key.trim().is_empty()
}

/// Decode the Aura mark from the real artwork, once.
pub(crate) fn load_logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let bytes = include_bytes!("../../../assets/mark.png");
    let image = egui_extras::image::load_image_bytes(bytes).ok()?;
    Some(ctx.load_texture("aura-mark", image, egui::TextureOptions::LINEAR))
}

/// Decode an image and make a GPU texture, with hard limits.
///
/// A raw photo (6000x4000) overflowed the wgpu texture budget and took the
/// whole app down, so we downscale first and refuse absurd sizes instead of
/// panicking inside eframe.
const BG_MAX_EDGE: u32 = 2048;
const BG_MAX_PIXELS: u64 = 4_000_000;

/// Resolve a possibly-relative path. A relative path is taken against $HOME so
/// "./Pictures/x.png" works no matter where the process was launched from.
fn resolve_path(path: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let mut base = std::path::PathBuf::from(home);
    // Strip a leading "./" so joining is a no-op for bare relative names.
    if let Ok(stripped) = p.strip_prefix("./") {
        base = base.join(stripped);
    } else if let Ok(stripped) = p.strip_prefix("~/") {
        base = base.join(stripped);
    } else if path.starts_with('~') {
        // "~/Pictures/x.png" — expand the tilde against $HOME.
        let rest = path.trim_start_matches('~').trim_start_matches('/');
        base = base.join(rest);
    } else {
        base = base.join(p);
    }
    base
}

fn load_img_texture(ctx: &egui::Context, path: &str) -> Result<egui::TextureHandle, String> {
    let resolved = resolve_path(path);
    let path = &resolved;
    // egui asserts (debug) / the GPU rejects textures past the device limit.
    let max_edge = ctx.input(|i| i.max_texture_side).min(BG_MAX_EDGE as usize).max(64) as u32;
    let img = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (w, h) = (img.width(), img.height());
    let pixels = w as u64 * h as u64;
    if w == 0 || h == 0 {
        return Err(format!("{}: empty image", path.display()));
    }
    if pixels > BG_MAX_PIXELS * 4 {
        return Err(format!(
            "{}: {w}x{h} is too large for a background (limit {} px)",
            path.display(),
            BG_MAX_PIXELS * 4
        ));
    }
    let rgba = if w > max_edge || h > max_edge {
        let scale = (max_edge as f64 / w.max(h) as f64).min(1.0);
        let nw = ((w as f64 * scale) as u32).max(1);
        let nh = ((h as f64 * scale) as u32).max(1);
        img.resize(nw, nh, image::imageops::FilterType::Triangle).to_rgba8()
    } else {
        img.to_rgba8()
    };
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    // A GPU that refuses the allocation must not take the window down.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ctx.load_texture("aura-bg", color, egui::TextureOptions::LINEAR)
    }))
    .map_err(|_| format!("{}: GPU refused a {}x{} texture", path.display(), size[0], size[1]))
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
            // Stable Wayland app_id so the desktop can target the window
            // (sway floats it here instead of tiling it full-screen).
            .with_app_id("aura-harness")
            .with_title("AURA harness")
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
    fn stopping_keeps_the_partial_answer() {
        let mut live = LiveTurn::default();
        assert!(partial_assistant_message(&live).is_none());

        live.reasoning.push_str("thinking");
        live.content.push_str("half an answer");
        let message = partial_assistant_message(&live).expect("partial answer");
        assert_eq!(message.role, "assistant");
        assert_eq!(message.content, "half an answer");
        assert_eq!(message.reasoning_content, "thinking");
    }

    #[test]
    fn manual_model_ids_are_prefixed_for_their_provider() {
        assert_eq!(
            route_manual_model_id("openrouter", "anthropic/claude-3.5-sonnet"),
            "openrouter/anthropic/claude-3.5-sonnet"
        );
        assert_eq!(
            route_manual_model_id("openrouter", "openrouter/anthropic/claude-3.5-sonnet"),
            "openrouter/anthropic/claude-3.5-sonnet"
        );
        assert_eq!(route_manual_model_id("opencode", "go-anthropic/claude-opus-5"), "go-anthropic/claude-opus-5");
        assert_eq!(route_manual_model_id("fpt", "DeepSeek-V4-Flash"), "fpt/DeepSeek-V4-Flash");
        assert_eq!(route_manual_model_id("deepseek", "deepseek-flash"), "deepseek-flash");
    }

    #[test]
    fn system_message_names_the_project_folder() {
        let dir = std::env::temp_dir();
        let abs = dir.canonicalize().unwrap();
        let abs = abs.to_str().unwrap();

        let with_prompt = system_message("  be brief ", &dir);
        assert!(with_prompt.starts_with("be brief"), "{with_prompt}");
        assert!(with_prompt.contains(abs), "{with_prompt}");
        assert!(with_prompt.contains("index.html"), "{with_prompt}");

        // No user prompt: the folder note still goes out, with no leading blank lines.
        let bare = system_message("", &dir);
        assert!(bare.starts_with("Working directory:"), "{bare}");

        // The empty-project-root default "." must become a real path the model can use.
        let dot = system_message("", std::path::Path::new("."));
        assert!(!dot.contains("directory: ."), "{dot}");
    }

    #[test]
    fn a_missing_project_folder_is_not_announced() {
        let gone = std::env::temp_dir().join("aura-harness-no-such-folder");
        // A stale saved project_root must not be promised to the model as the working directory.
        assert_eq!(system_message("be brief", &gone), "be brief");
        assert_eq!(system_message("", &gone), "");
        assert!(!project_dir_is_usable(&gone));
        assert!(project_dir_is_usable(&std::env::temp_dir()));
    }

    #[test]
    fn key_detection_ignores_whitespace() {
        assert!(!key_is_set("   "));
        assert!(key_is_set("sk-abc"));
    }

    #[test]
    fn relative_background_paths_resolve_against_home() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(
            resolve_path("./Pictures/x.png"),
            std::path::PathBuf::from(&home).join("Pictures/x.png")
        );
        assert_eq!(
            resolve_path("~/Pictures/y.png"),
            std::path::PathBuf::from(&home).join("Pictures/y.png")
        );
        assert_eq!(
            resolve_path("/abs/z.png"),
            std::path::PathBuf::from("/abs/z.png")
        );
        assert_eq!(
            resolve_path("wallpaper.png"),
            std::path::PathBuf::from(&home).join("wallpaper.png")
        );
    }

    #[test]
    fn embedded_icon_is_64x64_rgba() {
        assert_eq!(ICON_RGBA.len(), 64 * 64 * 4);
    }

    #[test]
    fn background_image_is_downscaled_and_failures_are_errors() {
        let dir = std::env::temp_dir().join("aura-bg-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // A photo-sized image used to blow the wgpu texture budget and kill
        // the app; it must come back downscaled instead.
        let big = dir.join("big.png");
        let mut img = image::RgbaImage::new(4000, 3000);
        img.put_pixel(0, 0, image::Rgba([10, 20, 30, 255]));
        img.save(&big).unwrap();

        let ctx = egui::Context::default();
        let tex = load_img_texture(&ctx, big.to_str().unwrap()).expect("big png loads");
        let [w, h] = tex.size();
        assert!(w as u32 <= BG_MAX_EDGE && h as u32 <= BG_MAX_EDGE, "{w}x{h}");
        assert_eq!([w, h], tex.size(), "texture matches the requested size");

        // Absurd input is refused, not fatal.
        let bad = dir.join("bad.png");
        std::fs::write(&bad, b"definitely not an image").unwrap();
        assert!(load_img_texture(&ctx, bad.to_str().unwrap()).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
