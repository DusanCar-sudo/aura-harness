//! The Aura Code command surface, imported into the harness.
//!
//! The TUI advertises its commands in `PALETTE_COMMANDS` (served to every
//! client over `command.list`). This module is that same list, so the
//! harness's `/` palette offers exactly what `aura` offers — no second,
//! quietly-shrinking copy.
//!
//! Dispatch has two halves:
//!
//! * **Native** — commands whose answer lives in this app's own state:
//!   sessions, model, theme, effort, approvals, voice, quit.
//! * **Engine** — everything else runs on `aura sidecar` through
//!   `command.run` (see `engine_bridge`), which is the implementation the TUI
//!   and web client already share. Memory, workflows, `:machina`, `:designx`,
//!   lessons, the cost ledger and the rest stay single-sourced.
//!
//! Nothing beginning with `/` or `:` is ever sent to the model as prose.

use harness_core::config::{EffortChoice, ThemeChoice};
use harness_core::models;

use crate::engine_bridge::EngineResult;
use crate::{App, View};

/// One advertised command.
#[derive(Debug, Clone, Copy)]
pub struct CommandSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub category: &'static str,
}

/// Every command Aura Code advertises, plus the handful that are specific to
/// the harness itself (settings, plugins, project root).
pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec { id: "/coder", label: "Coder mode", description: "Full coding agent — tools and project context", category: "Modes" },
    CommandSpec { id: "/gazelle", label: "Gazelle mode", description: "Lean conversation — no tools, no project context", category: "Modes" },
    CommandSpec { id: "/id", label: "Show session ID", description: "Current chat ID", category: "Session" },
    CommandSpec { id: "/sessions", label: "List sessions", description: "All saved sessions", category: "Session" },
    CommandSpec { id: "/resume", label: "Resume session", description: "Resume latest session", category: "Session" },
    CommandSpec { id: "/new", label: "New session", description: "Start fresh session", category: "Session" },
    CommandSpec { id: "/history", label: "Show history", description: "Turn count in current session", category: "Session" },
    CommandSpec { id: "/save", label: "Save session", description: "Rename/save current session", category: "Session" },
    CommandSpec { id: "/sessions all", label: "Sessions everywhere", description: "Saved sessions across every project", category: "Session" },
    CommandSpec { id: "/clear-history", label: "Clear history", description: "Wipe the conversation, keep the session id", category: "Session" },
    CommandSpec { id: "/delete", label: "Delete a session", description: "Remove a saved session by id", category: "Session" },
    CommandSpec { id: "/model", label: "Switch model", description: "Interactive model selector", category: "Model / API" },
    CommandSpec { id: "/provider", label: "Provider selector", description: "Pick provider, then model", category: "Model / API" },
    CommandSpec { id: "/apikey", label: "Set API key", description: "Set API key for session", category: "Model / API" },
    CommandSpec { id: "/effort", label: "Reasoning effort", description: "Show or set the effort rung for this model", category: "Model / API" },
    CommandSpec { id: "/skills", label: "List skills", description: "Skills the agent can route to", category: "Model / API" },
    CommandSpec { id: "/workflows", label: "List workflows", description: "All saved workflows", category: "Workflows" },
    CommandSpec { id: "/workflow", label: "Create workflow", description: "Multi-step workflow", category: "Workflows" },
    CommandSpec { id: "/machina", label: "Machina task", description: "Self-verification + auto-retry", category: "Workflows" },
    CommandSpec { id: "/council", label: "Council", description: "Parallel read-only specialists", category: "Workflows" },
    CommandSpec { id: "/q add", label: "Queue task", description: "Enqueue a task", category: "Workflows" },
    CommandSpec { id: "/q list", label: "Queue list", description: "List queued tasks", category: "Workflows" },
    CommandSpec { id: "/resume-workflow", label: "Resume workflow", description: "Continue a paused or failed workflow", category: "Workflows" },
    CommandSpec { id: "/ecclesia", label: "Ecclesia", description: "5 independent research agents, then a synthesis verdict", category: "Workflows" },
    CommandSpec { id: "/nerds", label: "Nerds", description: "One writer at a time, readers in parallel", category: "Workflows" },
    CommandSpec { id: "/marathon", label: "Marathon", description: "Flag a long haul (24h, lapses on its own)", category: "Workflows" },
    CommandSpec { id: "/plans", label: "Execution plans", description: "Plans this project has run", category: "Workflows" },
    CommandSpec { id: "/stop", label: "Stop the task", description: "Abort whatever is running (alias: /cancel)", category: "Workflows" },
    CommandSpec { id: "/designx", label: "Design commission", description: "Route a style, scrape references, build the artefact", category: "Design" },
    CommandSpec { id: "/designx styles", label: "Design lexicon", description: "List the style directions /designx routes from", category: "Design" },
    CommandSpec { id: "/dream", label: "Dream", description: "Consolidate episodes", category: "Memory" },
    CommandSpec { id: "/rem", label: "Show memory", description: "Reconciled memory", category: "Memory" },
    CommandSpec { id: "/mine --refine", label: "Mine + refine patterns", description: "Mine episodes and judge concepts with the local model", category: "Memory" },
    CommandSpec { id: "/mine --corrections", label: "Collect correction pairs", description: "Write direct correction pairs from escalation episodes", category: "Memory" },
    CommandSpec { id: "/mine --stats", label: "Training data stats", description: "Show training-data row counts by provenance", category: "Memory" },
    CommandSpec { id: "/research", label: "Research", description: "Multi-step research pass", category: "Memory" },
    CommandSpec { id: "/btw", label: "Side question", description: "Quick read-only question", category: "Memory" },
    CommandSpec { id: "/lessons", label: "Lessons learned", description: "What Aura learned and now tells herself", category: "Memory" },
    CommandSpec { id: "/journey", label: "Learning journey", description: "Chart + list of everything Aura did and learned; Enter opens one", category: "Memory" },
    CommandSpec { id: "/lessons timeline", label: "Learning timeline", description: "When lessons were learned, per day", category: "Memory" },
    CommandSpec { id: "/forget", label: "Forget a lesson", description: "Remove one learned lesson from the prompt", category: "Memory" },
    CommandSpec { id: "/confess", label: "Confess", description: "Auto-detect and confess an anomalous episode", category: "Memory" },
    CommandSpec { id: "/confessions", label: "List confessions", description: "Everything confessed so far", category: "Memory" },
    CommandSpec { id: "/graph", label: "Codebase graph", description: "Knowledge-graph summary (also: extract, refresh)", category: "Memory" },
    CommandSpec { id: "/viz", label: "Memory dashboard", description: "Generate and open it (alias: /dashboard; /viz all for every project)", category: "Memory" },
    CommandSpec { id: "/speak", label: "Toggle voice", description: "Read replies aloud", category: "Voice" },
    CommandSpec { id: "/approve", label: "Toggle auto-approve", description: "Skip y/N prompts", category: "Safety" },
    CommandSpec { id: "/compon", label: "Computer use on", description: "See the screen, drive mouse and keyboard (shows the disclosure)", category: "Safety" },
    CommandSpec { id: "/compoff", label: "Computer use off", description: "Turn it off and release the input device", category: "Safety" },
    CommandSpec { id: "/comp", label: "Computer use status", description: "Is it on, and what is holding it there", category: "Safety" },
    CommandSpec { id: "/auraweb", label: "Open the web client", description: "Starts the browser surface and opens it (also `webaura` from a terminal)", category: "Session" },
    CommandSpec { id: "/catchthis", label: "Catch a job", description: "Record what you do once, then have the agent repeat it", category: "Safety" },
    CommandSpec { id: "/help", label: "Help", description: "Show all commands", category: "System" },
    CommandSpec { id: "/archon", label: "Archimedes On", description: "Enable Archimedes Alternator for this session", category: "System" },
    CommandSpec { id: "/archoff", label: "Archimedes Off", description: "Disable Archimedes Alternator for this session", category: "System" },
    CommandSpec { id: "/archmodel", label: "Archimedes Model", description: "Set Archimedes local model for this session  e.g. /archmodel qwen3-vl:4b", category: "System" },
    CommandSpec { id: "/turnsoff", label: "Turns Off", description: "Disable per-task turn limit (unlimited)", category: "System" },
    CommandSpec { id: "/turnson", label: "Turns On", description: "Enable per-task turn limit (default: 50)", category: "System" },
    CommandSpec { id: "/turns", label: "Turn limit", description: "Show or set the per-task turn cap", category: "System" },
    CommandSpec { id: "/small1", label: "Start with Archimedes", description: "Bypass the competence gate for this session", category: "System" },
    CommandSpec { id: "/cost", label: "Cost ledger", description: "Spent vs direct-large counterfactual, per outcome", category: "System" },
    CommandSpec { id: "/q", label: "Quit", description: "Exit Aura", category: "System" },
    CommandSpec { id: "/context", label: "Context health", description: "Token usage dashboard", category: "System" },
    CommandSpec { id: "/doctor", label: "Doctor", description: "Run health checks", category: "System" },
    CommandSpec { id: "/theme", label: "Theme", description: "Wear one of the Aura adOS palettes", category: "System" },
    CommandSpec { id: "/theme sync", label: "Theme: follow adOS", description: "Wear the theme the OS is wearing", category: "System" },
    CommandSpec { id: "/compact", label: "Force compact", description: "Manual context compaction (alias: /compress)", category: "System" },
    CommandSpec { id: "/compress", label: "Force compress", description: "Manual context compaction (alias: /compact)", category: "System" },
    CommandSpec { id: "/quit", label: "Quit", description: "Exit Aura (aliases: /q, /exit)", category: "System" },
    CommandSpec { id: "/stats", label: "Session usage", description: "Tokens and cost this session (alias: /usage)", category: "Stats" },
    CommandSpec { id: "/clear", label: "Reset usage stats", description: "Zero the counters — history untouched (alias: /reset)", category: "Stats" },
    CommandSpec { id: "/settings", label: "Harness settings", description: "Open the harness Settings sheet", category: "System" },
    CommandSpec { id: "/plugins", label: "Plugin browser", description: "Find and add MCP servers and command tools", category: "System" },
    CommandSpec { id: "/root", label: "Project folder", description: "Show or set the folder the file tree and engine work in", category: "System" },
    CommandSpec { id: "/version", label: "Version", description: "Harness and Aura engine versions", category: "System" },
    CommandSpec { id: "/theme reset", label: "Theme: default", description: "Back to the harness default (Nature)", category: "System" },
    CommandSpec { id: "/designx web", label: "Design: web", description: "Force a web artefact", category: "Design" },
    CommandSpec { id: "/designx deck", label: "Design: deck", description: "Force a slide deck", category: "Design" },
    CommandSpec { id: "/designx pdf", label: "Design: pdf", description: "Force a PDF document", category: "Design" },
    CommandSpec { id: "/context tune", label: "Compaction ladder", description: "Adjust when compaction fires (terminal)", category: "System" },
    CommandSpec { id: "/catchthis list", label: "Caught jobs", description: "Everything caught so far", category: "Safety" },
    CommandSpec { id: "/catchthis show", label: "Show a caught job", description: "The steps it saw (id)", category: "Safety" },
    CommandSpec { id: "/q run", label: "Run queued task", description: "Execute queued task #n", category: "Workflows" },
    CommandSpec { id: "/q drop", label: "Drop queued task", description: "Remove queued task #n", category: "Workflows" },
    CommandSpec { id: "/q clear", label: "Clear the queue", description: "Wipe the task queue", category: "Workflows" },
    CommandSpec { id: "/lessons global", label: "Lessons: global", description: "Only global-scope lessons", category: "Memory" },
    CommandSpec { id: "/lessons project", label: "Lessons: project", description: "Only project-scope lessons", category: "Memory" },
    CommandSpec { id: "/menu", label: "Command palette", description: "Browse every command (Ctrl+P)", category: "System" },
];

/// A command's answer, rendered in the transcript.
#[derive(Debug, Clone)]
pub struct CommandNote {
    pub title: String,
    pub lines: Vec<String>,
    pub error: bool,
}

/// A line of input split into its command head and arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub head: String,
    pub args: Vec<String>,
    /// The original line, for passing to the engine unchanged.
    pub raw: String,
}

/// True when the line is meant as a command rather than a prompt.
///
/// `:` always is — it is not how prose starts. `/` only counts when the first
/// token is a command and contains no further slash, so a pasted absolute
/// path like `/home/me/x.ts` stays a task.
pub fn is_command_input(input: &str) -> bool {
    let t = input.trim();
    if t.is_empty() {
        return false;
    }
    if let Some(rest) = t.strip_prefix(':') {
        return !rest.trim().is_empty();
    }
    let Some(rest) = t.strip_prefix('/') else {
        return false;
    };
    let head = rest.split_whitespace().next().unwrap_or("");
    !head.is_empty() && !head.contains('/')
}

/// Split a command line into head and arguments. Quotes group words, the way
/// `/workflow name "step one" "step two"` needs them to.
pub fn parse(input: &str) -> Option<Parsed> {
    let t = input.trim();
    let rest = t.strip_prefix(':').or_else(|| t.strip_prefix('/'))?;
    let mut parts = split_args(rest);
    if parts.is_empty() {
        return None;
    }
    let head = parts.remove(0).to_lowercase();
    Some(Parsed {
        head,
        args: parts,
        raw: t.to_string(),
    })
}

/// Whitespace split with `"double"` and `'single'` quoted spans kept whole.
pub fn split_args(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for ch in input.chars() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            None => match ch {
                '"' | '\'' => quote = Some(ch),
                c if c.is_whitespace() => {
                    if !current.is_empty() {
                        out.push(std::mem::take(&mut current));
                    }
                }
                c => current.push(c),
            },
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// The command ids that start with the typed head, best match first.
pub fn matches(head: &str) -> Vec<&'static CommandSpec> {
    let mut out: Vec<&'static CommandSpec> = COMMANDS
        .iter()
        .filter(|c| c.id.trim_start_matches('/') == head)
        .collect();
    if out.is_empty() {
        out = COMMANDS
            .iter()
            .filter(|c| c.id.trim_start_matches('/').starts_with(&format!("{head} ")))
            .collect();
    }
    out
}

/// True when `head` names at least one advertised command.
pub fn is_known(head: &str) -> bool {
    !matches(head).is_empty()
}

/// Fuzzy filter for the palette: id, label and description all count.
pub fn filter(query: &str) -> Vec<&'static CommandSpec> {
    let q = query.trim().trim_start_matches(['/', ':']).to_lowercase();
    if q.is_empty() {
        return COMMANDS.iter().collect();
    }
    let mut scored: Vec<(i32, &'static CommandSpec)> = COMMANDS
        .iter()
        .filter_map(|c| {
            [c.id, c.label, c.description, c.category]
                .iter()
                .filter_map(|text| score(&q, &text.to_lowercase()))
                .min()
                .map(|s| (s, c))
        })
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0));
    scored.into_iter().map(|(_, c)| c).collect()
}

/// Lower is better. Exact prefix beats substring beats subsequence.
fn score(query: &str, text: &str) -> Option<i32> {
    if let Some(i) = text.find(query) {
        return Some(i as i32);
    }
    let mut qi = 0;
    let mut gaps = 0;
    let mut last = 0;
    for (ti, ch) in text.chars().enumerate() {
        if qi < query.chars().count() && query.chars().nth(qi) == Some(ch) {
            gaps += ti.saturating_sub(last);
            last = ti;
            qi += 1;
        }
    }
    (qi == query.chars().count()).then_some(100 + gaps as i32)
}

/// `/help` text, grouped by category — the palette in prose form.
pub fn help_lines() -> Vec<String> {
    let mut out = vec![
        "  Every Aura Code command runs here. Type / to browse, Ctrl+P to open the palette.".into(),
        String::new(),
    ];
    let mut category = "";
    for spec in COMMANDS {
        if spec.category != category {
            category = spec.category;
            out.push(String::new());
            out.push(format!("  ── {category} ──"));
        }
        out.push(format!("  {:<22} {}", spec.id, spec.description));
    }
    out.push(String::new());
    out.push("  Prefixes: / and : are equivalent. Anything else is a prompt.".into());
    out
}

/// Strip ANSI SGR/CSI escapes from engine output before it is drawn.
pub fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }
        match chars.peek() {
            Some('[') => {
                chars.next();
                // CSI: parameter bytes then one final byte in @..~
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                // OSC: terminated by BEL or ESC \
                for c in chars.by_ref() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {
                chars.next();
            }
        }
    }
    out
}

impl App {
    /// Handle a command line. Returns false when it is not a command (the
    /// caller then sends it as a prompt).
    pub(crate) fn run_command(&mut self, ctx: &egui::Context, input: &str) -> bool {
        if !is_command_input(input) {
            return false;
        }
        let Some(parsed) = parse(input) else {
            return false;
        };
        if !is_known(&parsed.head) && !parsed.head.is_empty() {
            // An unknown `/word` is still clearly a command, not a question.
            let hint = filter(&parsed.head)
                .into_iter()
                .take(3)
                .map(|c| c.id)
                .collect::<Vec<_>>()
                .join("  ·  ");
            self.note_error(
                format!("/{} — unknown command", parsed.head),
                vec![
                    if hint.is_empty() {
                        "Open the palette with /help or Ctrl+P.".to_string()
                    } else {
                        format!("Did you mean:  {hint}")
                    },
                ],
            );
            return true;
        }

        let arg = parsed.args.join(" ");
        match parsed.head.as_str() {
            "help" | "menu" | "?" => {
                self.palette.open = true;
                self.palette.selected = 0;
                self.note("Help — every command", help_lines());
            }
            "new" => {
                self.new_session();
                self.note("New chat", vec![self.session_id_line()]);
            }
            "sessions" => {
                let mut lines = Vec::new();
                for h in self.sessions.iter().take(60) {
                    let created = h.created.with_timezone(&chrono::Local);
                    let current = self
                        .session
                        .as_ref()
                        .is_some_and(|s| s.header.id == h.id);
                    lines.push(format!(
                        "{} {:36}  {}",
                        if current { "▸" } else { " " },
                        truncate(&h.title, 36),
                        created.format("%Y-%m-%d %H:%M")
                    ));
                }
                if lines.is_empty() {
                    lines.push("No saved sessions yet.".into());
                }
                self.note(format!("{} session(s)", self.sessions.len()), lines);
            }
            "resume" => {
                let id = parsed.args.first().cloned();
                match id {
                    Some(id) => {
                        let hit = self
                            .sessions
                            .iter()
                            .find(|h| h.id == id || h.id.starts_with(&id))
                            .map(|h| h.id.clone());
                        match hit {
                            Some(full) => {
                                self.open_session(&full);
                                self.note("Resumed", vec![self.session_id_line()]);
                            }
                            None => self.note_error(
                                format!("No session matches “{id}”"),
                                vec!["Run /sessions to see the list.".into()],
                            ),
                        }
                    }
                    None => match self.sessions.first().map(|h| h.id.clone()) {
                        Some(latest) => {
                            self.open_session(&latest);
                            self.note("Resumed the latest session", vec![self.session_id_line()]);
                        }
                        None => self.note_error("Nothing to resume", vec!["No saved sessions.".into()]),
                    },
                }
            }
            "delete" => {
                let Some(id) = parsed.args.first() else {
                    self.note_error("/delete needs an id", vec!["Run /sessions first.".into()]);
                    return true;
                };
                let hit = self
                    .sessions
                    .iter()
                    .find(|h| h.id == *id || h.id.starts_with(id))
                    .map(|h| h.id.clone());
                match hit {
                    Some(full) => {
                        self.delete_session(&full);
                        self.note("Deleted", vec![full]);
                    }
                    None => self.note_error(format!("No session matches “{id}”"), vec![]),
                }
            }
            "save" => {
                let title = if arg.trim().is_empty() {
                    self.session.as_ref().map(|s| s.header.title.clone()).unwrap_or_default()
                } else {
                    arg.trim().to_string()
                };
                if title.is_empty() {
                    self.note_error("/save needs a title", vec![]);
                    return true;
                }
                if let Some(session) = self.session.as_mut() {
                    if let Err(e) = session.set_title(title.clone()) {
                        self.note_error("Could not save", vec![e.to_string()]);
                    } else {
                        self.refresh_sessions();
                        self.note("Saved", vec![title]);
                    }
                } else {
                    self.note_error("No open conversation", vec![]);
                }
            }
            "history" => {
                let (turns, results) = self
                    .session
                    .as_ref()
                    .map(|s| {
                        let turns = s
                            .messages
                            .iter()
                            .filter(|m| m.role == "user")
                            .count();
                        let results = s.messages.iter().filter(|m| m.role == "tool").count();
                        (turns, results)
                    })
                    .unwrap_or((0, 0));
                self.note(
                    "History",
                    vec![format!(
                        "{turns} turn(s) · {results} tool result(s) · {} message(s) in the request",
                        self.session.as_ref().map(|s| s.messages.len()).unwrap_or(0)
                    )],
                );
            }
            "id" => {
                self.note("Session", vec![self.session_id_line()]);
            }
            "clear-history" => {
                if let Some(session) = self.session.as_mut() {
                    session.messages.clear();
                    self.live.clear();
                    self.cards.clear();
                    self.persist();
                    self.note("History cleared", vec![self.session_id_line()]);
                }
            }
            "clear" | "reset" => {
                self.notes.clear();
                self.note("Stats reset", vec!["No counters to zero in the harness — history untouched. Use /new for a fresh chat.".into()]);
            }
            "model" => {
                if arg.trim().is_empty() {
                    self.open_model_factory();
                } else {
                    let id = arg.trim();
                    let family = models::provider_for_model(id);
                    self.select_model(id, family);
                    self.note("Model", vec![id.to_string()]);
                }
            }
            "provider" => self.open_model_factory(),
            "apikey" => {
                if arg.trim().is_empty() {
                    self.view = View::Settings;
                    self.note("API key", vec!["Paste it in Settings → Access key.".into()]);
                } else {
                    match harness_core::config::store_api_key(arg.trim()) {
                        Ok(()) => {
                            self.api_key = Some(arg.trim().to_string());
                            self.key_input = arg.trim().to_string();
                            self.note("API key saved", vec!["Persisted to the harness credentials file.".into()]);
                        }
                        Err(e) => self.note_error("Could not save the key", vec![e.to_string()]),
                    }
                }
            }
            "effort" => {
                if arg.trim().is_empty() {
                    self.note(
                        "Reasoning effort",
                        vec![
                            format!("Current: {}", effort_label(self.effort)),
                            "Ladder: off · low · high · max".into(),
                            "Usage: /effort <level>".into(),
                        ],
                    );
                } else {
                    match effort_from(&arg) {
                        Some(level) => {
                            self.effort = level;
                            self.config.reasoning_effort = level;
                            let _ = self.config.save();
                            self.note("Effort", vec![effort_label(level).to_string()]);
                        }
                        None => self.note_error(
                            format!("Unknown effort “{}”", arg.trim()),
                            vec!["Expected: off, low, high, max".into()],
                        ),
                    }
                }
            }
            "theme" | "themes" => self.command_theme(&arg),
            "approve" => {
                let next = match arg.trim() {
                    "off" => false,
                    "all" | "on" => true,
                    "" => !self.config.auto_approve,
                    other => {
                        self.note_error(
                            format!("Unknown /approve argument “{other}”"),
                            vec!["Use /approve, /approve all or /approve off.".into()],
                        );
                        return true;
                    }
                };
                self.config.auto_approve = next;
                self.registry = harness_core::tools::from_plugins(&self.plugins, next);
                let _ = self.config.save();
                self.note(
                    "Auto-approve",
                    vec![if next {
                        "ON — tools run without asking. Dangerous commands are still blocked.".into()
                    } else {
                        "OFF — mutating tools ask for approval again.".into()
                    }],
                );
            }
            "speak" => {
                self.speak_replies = !self.speak_replies;
                let on = self.speak_replies;
                self.note(
                    "Voice replies",
                    vec![if on {
                        "ON — answers are read aloud as they finish.".into()
                    } else {
                        "OFF".into()
                    }],
                );
            }
            "stop" | "cancel" => {
                self.stop();
                self.note("Stopped", vec!["The partial answer is kept.".into()]);
            }
            "quit" | "exit" => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            "q" if parsed.args.is_empty() => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            "settings" => self.view = View::Settings,
            "plugins" => self.show_browser = true,
            "root" => self.command_root(&arg),
            "version" => {
                let engine = self
                    .engine
                    .as_ref()
                    .map(|b| b.binary().to_string())
                    .or_else(|| {
                        crate::engine_bridge::find_aura_binary()
                    })
                    .unwrap_or_else(|| "not found".into());
                self.note(
                    format!("Aura Harness {}", env!("CARGO_PKG_VERSION")),
                    vec![format!("engine: {engine}")],
                );
            }
            "context" | "stats" | "usage" => {
                let model = self.current_model_id();
                let root = self.file_tree.root.display().to_string();
                let mut lines = vec![
                    format!("model:   {}", models::display_name(&model)),
                    format!("effort:  {}", effort_label(self.effort)),
                    format!("project: {root}"),
                    format!("tools:   {}  ·  plugins: {} MCP", self.registry.len(), self.mcp_status.len()),
                    self.session_id_line(),
                ];
                if let Some(s) = &self.session {
                    let turns = s.messages.iter().filter(|m| m.role == "user").count();
                    lines.push(format!("turns:   {turns}"));
                }
                if let Some(busy) = &self.command_busy {
                    lines.push(format!("engine:  running {busy}"));
                }
                lines.push("Token/cost totals are kept by the engine — /cost reads its ledger.".into());
                self.note("Session", lines);
            }
            "compact" | "compress" => {
                self.note_error(
                    "Compaction",
                    vec![
                        "The harness does not compact: it sends the conversation as it stands.".into(),
                        "Use /new to start a fresh context.".into(),
                    ],
                );
            }
            "turns" | "turnson" | "turnsoff" => {
                self.note(
                    "Turn limit",
                    vec![
                        "The harness agent loop has no turn cap — it runs until the task ends or you press Stop.".into(),
                    ],
                );
            }
            _ => {
                self.run_on_engine(&parsed.raw);
            }
        }
        ctx.request_repaint();
        true
    }

    fn command_theme(&mut self, arg: &str) {
        let name = arg.trim().to_lowercase();
        if name.is_empty() || name == "list" || name == "themes" {
            let lines = ThemeChoice::all()
                .iter()
                .map(|t| {
                    let mark = if *t == self.config.theme { "▸" } else { " " };
                    format!("{mark} {:<10} {}", theme_key(*t), t.label())
                })
                .collect();
            self.note("Themes", lines);
            return;
        }
        let choice = if name == "reset" || name == "default" {
            Some(ThemeChoice::Nature)
        } else {
            theme_from_name(&name)
        };
        match choice {
            Some(choice) => {
                self.config.theme = choice;
                self.applied_theme = choice;
                if let Err(e) = self.config.save() {
                    self.note_error("Could not save the theme", vec![e.to_string()]);
                } else {
                    self.note("Theme", vec![choice.label().to_string()]);
                }
            }
            None => self.note_error(
                format!("Unknown theme “{}”", arg.trim()),
                vec![format!(
                    "Known: {}",
                    ThemeChoice::all()
                        .iter()
                        .map(|t| theme_key(*t).to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )],
            ),
        }
    }

    fn command_root(&mut self, arg: &str) {
        if arg.trim().is_empty() {
            self.note(
                "Project folder",
                vec![
                    self.file_tree.root.display().to_string(),
                    "Usage: /root <path>".into(),
                ],
            );
            return;
        }
        let path = crate::resolve_path(arg.trim());
        if !path.is_dir() {
            self.note_error(
                format!("Not a folder: {}", path.display()),
                vec!["Check the path and try again.".into()],
            );
            return;
        }
        self.file_tree.root = path.clone();
        self.config.project_root = path.display().to_string();
        let _ = self.config.save();
        self.preview.force_rescan(&path);
        // The engine session is scoped to its project root; start a new one.
        self.engine = None;
        self.note("Project folder", vec![path.display().to_string()]);
    }

    /// Send a command to `aura sidecar`, starting it on first use.
    fn run_on_engine(&mut self, raw: &str) {
        if self.command_busy.is_some() {
            self.note_error(
                "One command at a time",
                vec![format!(
                    "Still running {}. The engine has one session.",
                    self.command_busy.clone().unwrap_or_default()
                )],
            );
            return;
        }
        if self.engine.is_none() {
            if crate::engine_bridge::find_aura_binary().is_none() {
                self.note_error(
                    "Aura Code is not installed",
                    vec![
                        "This command is implemented by the Aura engine.".into(),
                        "Install aura-code (or set AURA_BIN) and try again.".into(),
                    ],
                );
                return;
            }
            let root = crate::absolute_dir(&self.file_tree.root)
                .display()
                .to_string();
            let model = self.current_model_id();
            let key = harness_core::provider::get_api_key_for_model(&model)
                .or_else(|| self.api_key.clone());
            let base = harness_core::provider::base_url_for(&model);
            self.engine = Some(crate::engine_bridge::EngineBridge::start(
                root, model, key, base,
            ));
        }
        if let Some(bridge) = &self.engine {
            bridge.run(raw.to_string());
            self.command_busy = Some(raw.to_string());
            self.status = Some(format!("Running {raw} on the Aura engine…"));
        }
    }

    /// Drain finished engine commands into transcript notes.
    pub(crate) fn pump_engine(&mut self, ctx: &egui::Context) {
        let mut finished = Vec::new();
        if let Some(bridge) = &self.engine {
            while let Some(result) = bridge.try_recv() {
                finished.push(result);
            }
        }
        if finished.is_empty() {
            return;
        }
        for result in finished {
            self.command_busy = None;
            self.status = None;
            match result {
                EngineResult::Done(out) => {
                    let command = out
                        .reason
                        .clone()
                        .filter(|_| out.terminal_only)
                        .map(|r| format!("terminal only — {r}"))
                        .unwrap_or_else(|| "done".into());
                    let lines: Vec<String> = out
                        .lines
                        .iter()
                        .map(|l| strip_ansi(l))
                        .flat_map(|l| l.split('\n').map(str::to_string).collect::<Vec<_>>())
                        .collect();
                    if out.terminal_only {
                        self.note_error(
                            "Needs the terminal",
                            vec![
                                command,
                                "This command drives the TUI's own stdin; the harness has its own UI for it.".into(),
                            ],
                        );
                    } else if !out.handled {
                        self.note_error(
                            "Not handled by the engine",
                            vec!["The Aura engine did not recognise this command.".into()],
                        );
                    } else {
                        self.note("Command output", clean_lines(lines));
                    }
                }
                EngineResult::Failed(err) => {
                    self.note_error("Engine error", vec![err]);
                }
            }
        }
        ctx.request_repaint();
    }

    pub(crate) fn note(&mut self, title: impl Into<String>, lines: Vec<String>) {
        self.push_note(CommandNote {
            title: title.into(),
            lines: clean_lines(lines),
            error: false,
        });
    }

    pub(crate) fn note_error(&mut self, title: impl Into<String>, lines: Vec<String>) {
        self.push_note(CommandNote {
            title: title.into(),
            lines: clean_lines(lines),
            error: true,
        });
    }

    fn push_note(&mut self, note: CommandNote) {
        self.notes.push(note);
        if self.notes.len() > 30 {
            self.notes.remove(0);
        }
        self.focus_composer = true;
    }

    fn session_id_line(&self) -> String {
        match &self.session {
            Some(s) => format!("{}  ·  “{}”", s.header.id, s.header.title),
            None => "(no session yet — the next prompt creates one)".into(),
        }
    }
}

fn clean_lines(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = lines
        .into_iter()
        .flat_map(|line| {
            line.split('\n')
                .map(|l| l.trim_end().to_string())
                .collect::<Vec<_>>()
        })
        .collect();
    while out.first().is_some_and(|l| l.is_empty()) {
        out.remove(0);
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn effort_label(effort: EffortChoice) -> &'static str {
    match effort {
        EffortChoice::None => "off",
        EffortChoice::Low => "low",
        EffortChoice::High => "high",
        EffortChoice::Max => "max",
    }
}

fn effort_from(arg: &str) -> Option<EffortChoice> {
    match arg.trim().to_lowercase().as_str() {
        "off" | "none" | "0" => Some(EffortChoice::None),
        "low" | "minimal" | "medium" => Some(EffortChoice::Low),
        "high" | "xhigh" => Some(EffortChoice::High),
        "max" | "maximum" => Some(EffortChoice::Max),
        _ => None,
    }
}

/// The stable id a theme is saved under (the serde name).
fn theme_key(theme: ThemeChoice) -> &'static str {
    match theme {
        ThemeChoice::Nature => "nature",
        ThemeChoice::Midnight => "midnight",
        ThemeChoice::Retro => "retro",
        ThemeChoice::Paper => "paper",
        ThemeChoice::Ember => "ember",
        ThemeChoice::Ocean => "ocean",
        ThemeChoice::Dracula => "dracula",
        ThemeChoice::Solarized => "solarized",
        ThemeChoice::Graphite => "graphite",
    }
}

fn theme_from_name(name: &str) -> Option<ThemeChoice> {
    match name.trim().to_lowercase().as_str() {
        "nature" | "forest" | "default" => Some(ThemeChoice::Nature),
        "midnight" | "blueprint" | "matrix" => Some(ThemeChoice::Midnight),
        "retro" => Some(ThemeChoice::Retro),
        "paper" | "cream" | "gruvbox" => Some(ThemeChoice::Paper),
        "ember" => Some(ThemeChoice::Ember),
        "ocean" | "cyan-grid" => Some(ThemeChoice::Ocean),
        "dracula" => Some(ThemeChoice::Dracula),
        "solarized" => Some(ThemeChoice::Solarized),
        "graphite" => Some(ThemeChoice::Graphite),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_not_prompts() {
        assert!(is_command_input("/help"));
        assert!(is_command_input(":dream full"));
        assert!(!is_command_input("/home/dusan/notes.md"));
        assert!(!is_command_input("tell me about /dream"));
        assert!(!is_command_input("   "));
    }

    #[test]
    fn quotes_survive_argument_splitting() {
        let p = parse(r#"/workflow ship "step one" 'step two'"#).unwrap();
        assert_eq!(p.head, "workflow");
        assert_eq!(p.args, vec!["ship", "step one", "step two"]);
    }

    #[test]
    fn colon_and_slash_are_the_same_command() {
        assert_eq!(parse(":dream full").unwrap().head, "dream");
        assert_eq!(parse("/dream full").unwrap().head, "dream");
    }

    #[test]
    fn every_advertised_command_parses_back_to_itself() {
        for spec in COMMANDS {
            let parsed = parse(spec.id)
                .unwrap_or_else(|| panic!("{} does not parse", spec.id));
            assert!(
                is_known(&parsed.head),
                "{} parses to unknown head {}",
                spec.id,
                parsed.head
            );
        }
    }

    #[test]
    fn the_full_tui_palette_is_present() {
        for id in [
            "/dream", "/mine --refine", "/research", "/designx", "/council",
            "/ecclesia", "/marathon", "/catchthis", "/lessons timeline",
            "/forget", "/machina", "/graph", "/viz", "/workflows", "/cost",
            "/doctor", "/archon", "/compon", "/btw", "/journey",
        ] {
            let present = COMMANDS.iter().any(|c| c.id == id)
                || COMMANDS.iter().any(|c| c.id.starts_with(&format!("{id} ")));
            assert!(present, "{id} missing from the imported palette");
        }
    }

    #[test]
    fn filtering_finds_by_label_and_category() {
        assert!(filter("dream").iter().any(|c| c.id == "/dream"));
        assert!(filter("Memory").iter().any(|c| c.category == "Memory"));
        assert!(filter("zzzzz").is_empty());
    }

    #[test]
    fn ansi_is_stripped_without_eating_text() {
        let raw = "\u{1b}[32m✓\u{1b}[0m done \u{1b}[1;31mnow\u{1b}[0m";
        assert_eq!(strip_ansi(raw), "✓ done now");
    }

    #[test]
    fn help_lists_every_command() {
        let text = help_lines().join("\n");
        for spec in COMMANDS {
            assert!(text.contains(spec.id), "{} is missing from /help", spec.id);
        }
    }
}
