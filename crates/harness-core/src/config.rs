//! App config + credential storage.
//!
//! `~/.config/aura-harness/config.toml`  — model defaults, system prompt
//! `~/.config/aura-harness/credentials`  — `api_key=...`, mode 0600
//! `~/.config/aura-harness/plugins.toml` — MCP servers + local command tools

use crate::api::{Model, ReasoningEffort};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub model: ModelChoice,
    /// Universal multi-provider selector: full model id (e.g. "claude-sonnet-4").
    #[serde(default)]
    pub model_id: String,
    /// Base URL auto-filled from the selected model's provider.
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub reasoning_effort: EffortChoice,
    #[serde(default)]
    pub theme: ThemeChoice,
    /// Optional full-app background image path.
    #[serde(default)]
    pub background_image: String,
    /// How much of the background image shows through the panels (0..=1).
    #[serde(default = "default_background_opacity")]
    pub background_opacity: f32,
    /// Full access: mutating tools (shell, write_file, edit_file, cmd_*)
    /// run without asking for approval first.
    #[serde(default = "default_auto_approve")]
    pub auto_approve: bool,
    #[serde(default)]
    pub system_prompt: String,
    /// Mirror session summaries to the shared AGENTMEM store (Aura memory layer).
    #[serde(default)]
    pub agentmem_mirror: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ModelChoice {
    #[default]
    #[serde(rename = "flash")]
    Flash,
    #[serde(rename = "v4pro")]
    V4Pro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EffortChoice {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "low")]
    Low,
    #[default]
    #[serde(rename = "high")]
    High,
    #[serde(rename = "max")]
    Max,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: ModelChoice::Flash,
            model_id: "deepseek-flash".to_string(),
            base_url: Some("https://api.deepseek.com/v1".to_string()),
            reasoning_effort: EffortChoice::High,
            theme: ThemeChoice::Nature,
            background_image: String::new(),
            background_opacity: default_background_opacity(),
            auto_approve: default_auto_approve(),
            system_prompt: String::new(),
            agentmem_mirror: true,
        }
    }
}

fn default_background_opacity() -> f32 {
    0.35
}

/// Default to full access: the agent runs tools without an approval prompt.
/// Users who want the gate back can turn this off in Settings.
fn default_auto_approve() -> bool {
    true
}

/// Which look to wear. The palettes themselves live in the app crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeChoice {
    #[default]
    #[serde(rename = "nature")]
    Nature,
    #[serde(rename = "midnight")]
    Midnight,
    #[serde(rename = "retro")]
    Retro,
    #[serde(rename = "paper")]
    Paper,
}

impl ThemeChoice {
    /// Every theme, in the order the picker shows them.
    pub fn all() -> [ThemeChoice; 4] {
        [
            ThemeChoice::Nature,
            ThemeChoice::Midnight,
            ThemeChoice::Retro,
            ThemeChoice::Paper,
        ]
    }

    /// Label for the theme picker.
    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::Nature => "Aura Nature",
            ThemeChoice::Midnight => "Midnight",
            ThemeChoice::Retro => "Retro",
            ThemeChoice::Paper => "Paper",
        }
    }
}

impl From<ModelChoice> for Model {
    fn from(c: ModelChoice) -> Self {
        match c {
            ModelChoice::Flash => Model::Flash,
            ModelChoice::V4Pro => Model::V4Pro,
        }
    }
}

impl From<EffortChoice> for ReasoningEffort {
    fn from(c: EffortChoice) -> Self {
        match c {
            EffortChoice::None => ReasoningEffort::None,
            EffortChoice::Low => ReasoningEffort::Low,
            EffortChoice::High => ReasoningEffort::High,
            EffortChoice::Max => ReasoningEffort::Max,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandTool {
    /// Program to execute. Args are an argv array — never a shell string.
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub description: String,
    #[serde(default)]
    pub schema: toml::Table,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Plugins {
    #[serde(default)]
    pub mcp: std::collections::BTreeMap<String, McpServer>,
    #[serde(default)]
    pub cmd: std::collections::BTreeMap<String, CommandTool>,
}

fn default_true() -> bool {
    true
}

/// Universal Aura home directory — shared across all Aura tools (aura-code,
/// aura-harness, aura-dsh, etc.). Defaults to `~/.aura`, overridable via
/// `AURA_HOME`.
pub fn aura_home() -> PathBuf {
    if let Ok(dir) = std::env::var("AURA_HOME") {
        return PathBuf::from(dir);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".aura")
}

/// Harness-specific config dir — for persona, session storage, etc.
pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("AURA_HARNESS_HOME") {
        return PathBuf::from(dir);
    }
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config")
        });
    base.join("aura-harness")
}

fn read_toml<T: for<'de> Deserialize<'de>>(path: &PathBuf) -> std::io::Result<Option<T>> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text)
            .map(Some)
            .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

impl Config {
    pub fn load() -> std::io::Result<Self> {
        Ok(read_toml(&config_dir().join("config.toml"))?.unwrap_or_default())
    }

    pub fn save(&self) -> std::io::Result<()> {
        let dir = config_dir();
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            dir.join("config.toml"),
            toml::to_string_pretty(self).map_err(|e| std::io::Error::other(e.to_string()))?,
        )
    }
}

impl Plugins {
    pub fn load() -> std::io::Result<Self> {
        Ok(read_toml(&config_dir().join("plugins.toml"))?.unwrap_or_default())
    }

    /// Write the config back, always keeping an explanatory header so the file
    /// still teaches you how to add a plugin by hand afterwards.
    pub fn save(&self) -> std::io::Result<()> {
        let dir = config_dir();
        std::fs::create_dir_all(&dir)?;
        let body = toml::to_string_pretty(self).map_err(|e| std::io::Error::other(e.to_string()))?;
        std::fs::write(dir.join("plugins.toml"), format!("{PLUGINS_HEADER}{body}"))
    }
}

/// Rewritten above the tables every time plugins are saved.
const PLUGINS_HEADER: &str = "\
# Aura Harness plugins - MCP servers and local command tools.
#
#   [mcp.<name>]  the server's tools appear to the model as mcp_<name>_<tool>
#   [cmd.<name>]  appears as cmd_<name>; every run asks for your approval
#
# Add servers from Settings -> Plugins, or edit this file by hand.

";

pub fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Normalize a typed plugin name into a TOML-safe key.
pub fn normalize_name(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if c == '-' || c == '_' || c == '.' || c.is_whitespace() {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "plugin".to_string()
    } else {
        trimmed
    }
}

/// Derive a plugin key from a launch line.
///
/// Wrapper programs (`npx`, `uvx`, `node`, …) name nothing themselves, so the
/// first non-flag argument after them is used instead:
/// `npx -y rea-agents mcp` → `rea_agents`, `/opt/bin/unity mcp` → `unity`.
pub fn derive_name(command_line: &str) -> String {
    const WRAPPERS: [&str; 9] = [
        "npx", "bunx", "uvx", "pnpm", "yarn", "npm", "node", "python", "python3",
    ];

    fn basename(token: &str) -> &str {
        token.rsplit('/').next().unwrap_or(token)
    }

    let tokens = split_args(command_line);
    let Some(first) = tokens.first() else {
        return "plugin".to_string();
    };

    let program = basename(first);
    if WRAPPERS.contains(&program) {
        if let Some(meaningful) = tokens.iter().skip(1).find(|t| !t.starts_with('-')) {
            return normalize_name(basename(meaningful));
        }
    }
    normalize_name(program)
}

/// Split typed arguments into argv; quotes group words.
pub fn split_args(raw: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for c in raw.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => current.push(c),
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c.is_whitespace() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            None => current.push(c),
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

/// Shape of Aura Code's `~/.aura/mcp.json`.
#[derive(serde::Deserialize)]
struct AuraMcpJson {
    #[serde(rename = "mcpServers", default)]
    servers: std::collections::BTreeMap<String, AuraMcpServer>,
}

#[derive(serde::Deserialize)]
struct AuraMcpServer {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: std::collections::BTreeMap<String, String>,
}

/// MCP servers already declared for Aura Code, so they need not be retyped.
///
/// Empty (not an error) when the file is absent — the normal case on a machine
/// that never ran Aura Code.
pub fn aura_code_servers() -> Vec<(String, McpServer)> {
    let mut out = Vec::new();
    // Legacy: ~/.aura/mcp.json (same file aura-code uses)
    out.extend(aura_code_servers_in(&aura_home()));
    // Also: home dir (legacy fallback)
    let home = std::env::var("HOME").ok().map(PathBuf::from);
    if let Some(home) = home {
        out.extend(aura_code_servers_in(&home));
    }
    out
}

/// [`aura_code_servers`] against an explicit home, so it can be tested without
/// touching the process environment.
pub fn aura_code_servers_in(home: &Path) -> Vec<(String, McpServer)> {
    let path = home.join(".aura/mcp.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(parsed) = serde_json::from_str::<AuraMcpJson>(&text) else {
        return Vec::new();
    };
    parsed
        .servers
        .into_iter()
        .map(|(name, s)| {
            (
                normalize_name(&name),
                McpServer {
                    command: s.command,
                    args: s.args,
                    env: s.env,
                    enabled: true,
                },
            )
        })
        .collect()
}

/// Load all keys from the universal Aura .env file. Returns a map of
/// `KEY_NAME -> value` for every `KEY=value` line (ignoring comments/empty).
pub fn load_dotenv() -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    let path = aura_home().join(".env");
    let Ok(text) = std::fs::read_to_string(path) else { return out; };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') { continue; }
        let Some((key, value)) = line.split_once('=') else { continue; };
        let key = key.trim();
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
        if !key.is_empty() && !value.is_empty() {
            out.insert(key.to_string(), value);
        }
    }
    out
}

/// Universal API key lookup: tries process env → Aura .env → legacy credentials.
/// The `key_name` argument can be any `*_API_KEY` or `DEEPSEEK_API_KEY`.
pub fn load_api_key_named(key_name: &str) -> Option<String> {
    if let Ok(v) = std::env::var(key_name) {
        if !v.trim().is_empty() { return Some(v.trim().to_string()); }
    }
    if let Some(v) = load_dotenv().get(key_name) {
        if !v.trim().is_empty() { return Some(v.clone()); }
    }
    // Legacy path: ~/.config/aura-harness/credentials (uses api_key= prefix)
    let path = config_dir().join("credentials");
    if let Ok(text) = std::fs::read_to_string(path) {
        for line in text.lines() {
            // Match "api_key=" (legacy) OR "{key_name}=" (newer convention).
            let legacy = line.strip_prefix("api_key=").map(|v| v.trim());
            let named = line.strip_prefix(&format!("{}=", key_name)).map(|v| v.trim());
            for v in [legacy, named].into_iter().flatten() {
                if !v.is_empty() { return Some(v.to_string()); }
            }
        }
    }
    None
}

/// Backward-compatible alias for load_api_key_named("DEEPSEEK_API_KEY").
pub fn load_api_key() -> Option<String> {
    load_api_key_named("DEEPSEEK_API_KEY")
}

/// Universal config.json loading from ~/.aura/config.json.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuraConfigJson {
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub effort: Option<EffortChoice>,
    #[serde(default)]
    pub theme: Option<ThemeChoice>,
    #[serde(default)]
    pub active_plugins: Vec<String>,
    #[serde(default)]
    pub fallbacks: Vec<String>,
}

pub fn load_config_json() -> AuraConfigJson {
    let path = aura_home().join("config.json");
    if let Ok(text) = std::fs::read_to_string(path) {
        if let Ok(parsed) = serde_json::from_str(&text) {
            return parsed;
        }
    }
    AuraConfigJson::default()
}

pub fn save_config_json(cfg: &AuraConfigJson) -> std::io::Result<()> {
    let dir = aura_home();
    std::fs::create_dir_all(&dir)?;
    let text = serde_json::to_string_pretty(cfg)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::write(dir.join("config.json"), text)
}

/// Store the API key with 0600 permissions.
pub fn store_api_key(key: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("credentials");
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)?;
    writeln!(f, "api_key={key}")?;
    f.flush()
}

/// Starter `plugins.toml`, written once so there is something to edit.
pub const PLUGINS_TEMPLATE: &str = r#"# Aura Harness plugins — MCP servers and local command tools.
#
# Everything declared here is offered to the model as a callable tool:
#   [mcp.<name>]  ->  tools appear as mcp_<name>_<tool>
#   [cmd.<name>]  ->  appears as cmd_<name>, and each run asks you to approve
#
# After editing, use Settings -> Reconnect (or restart the app).

# --- MCP servers (stdio) ---------------------------------------------------
# Any MCP server works: give the command that launches it and its args.
#
# The Aura memory layer (shared across agents) is added automatically when
# its server file exists. To pin it explicitly, uncomment:
#
# [mcp.mnemai]
# command = "node"
# args = ["/mnt/bigdata/aura-mnemai/mcp-server.mjs"]
# enabled = true
#
# REA (https://github.com/morluto/rea) is an MCP server for reverse
# engineering. Install it once with `npx rea-agents setup`, then declare the
# command that setup registers so this app can call it:
#
# [mcp.rea]
# command = "<command printed by rea-agents setup>"
# args = ["<args from that setup>"]
# enabled = false

# --- Local command tools ---------------------------------------------------
# `command` is a program; `args` is an argv list — never a shell string.
# `{{key}}` placeholders are filled from the model's JSON arguments.
#
# [cmd.today]
# command = "date"
# args = ["+%Y-%m-%d"]
# description = "Return today's date"
# enabled = true
"#;

/// Create `~/.config/aura-harness/` with a starter config and plugins file.
///
/// Existing files are never touched.
pub fn ensure_templates() -> std::io::Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let plugins_path = dir.join("plugins.toml");
    if !plugins_path.exists() {
        std::fs::write(&plugins_path, PLUGINS_TEMPLATE)?;
    }
    if !dir.join("config.toml").exists() {
        Config::default().save()?;
    }
    Ok(())
}

/// Best-effort default plugin set: the Aura Mnemai memory server when present.
///
/// Mnemai is how this app reaches the Aura memory layer — the same server the
/// TUI, web client and Droid attach, so one memory is shared across agents.
pub fn with_default_plugins(mut plugins: Plugins) -> Plugins {
    if plugins.mcp.contains_key("mnemai") {
        return plugins;
    }

    // Preferred: the launcher the other Aura agents use.
    if let Some(launcher) = mnemai_launcher() {
        plugins.mcp.insert(
            "mnemai".into(),
            McpServer {
                command: launcher,
                args: vec!["serve".into()],
                env: Default::default(),
                enabled: true,
            },
        );
        return plugins;
    }

    // Fallback: a node-based server, if one was ever installed.
    for cmd in [
        "/mnt/bigdata/aura-mnemai/mcp-server.mjs",
        "/mnt/agentmem/mnemai/mcp-server.mjs",
        "/mnt/agentmem/tools/aura-mnemai/mcp-server.mjs",
    ] {
        if PathBuf::from(cmd).exists() {
            plugins.mcp.insert(
                "mnemai".into(),
                McpServer {
                    command: "node".into(),
                    args: vec![cmd.to_string()],
                    env: Default::default(),
                    enabled: true,
                },
            );
            break;
        }
    }
    plugins
}

/// Absolute path to the `mnemai` launcher, when it is installed.
fn mnemai_launcher() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".local/bin/mnemai");
    path.exists().then(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trip() {
        let _lock = crate::env_lock::guard();
        let dir = std::env::temp_dir().join(format!("aura-harness-cfg-{}", uuid::Uuid::new_v4()));
        std::env::set_var("AURA_HARNESS_HOME", &dir);
        let mut c = Config::default();
        c.model = ModelChoice::V4Pro;
        c.system_prompt = "be brief".into();
        c.save().unwrap();
        let loaded = Config::load().unwrap();
        assert_eq!(loaded.model, ModelChoice::V4Pro);
        assert_eq!(loaded.system_prompt, "be brief");
        std::env::remove_var("AURA_HARNESS_HOME");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn plugins_round_trip_with_cmd_tool() {
        let _lock = crate::env_lock::guard();
        let dir = std::env::temp_dir().join(format!("aura-harness-plg-{}", uuid::Uuid::new_v4()));
        std::env::set_var("AURA_HARNESS_HOME", &dir);
        let mut p = Plugins::default();
        p.cmd.insert(
            "echo".into(),
            CommandTool {
                command: "/bin/echo".into(),
                args: vec![],
                description: "echo back".into(),
                schema: toml::Table::new(),
                enabled: true,
            },
        );
        p.save().unwrap();
        let loaded = Plugins::load().unwrap();
        assert_eq!(loaded.cmd["echo"].command, "/bin/echo");
        std::env::remove_var("AURA_HARNESS_HOME");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn plugin_names_are_normalized() {
        assert_eq!(normalize_name("My Server"), "my_server");
        assert_eq!(normalize_name("  Rea!!  "), "rea");
        assert_eq!(normalize_name("unity-editor-mcp"), "unity_editor_mcp");
        assert_eq!(normalize_name("!!!"), "plugin");
    }

    #[test]
    fn plugin_names_derive_from_the_launch_line() {
        // Wrappers name nothing themselves.
        assert_eq!(derive_name("npx -y rea-agents mcp"), "rea_agents");
        assert_eq!(derive_name("uvx mcp-server-fetch"), "mcp_server_fetch");
        assert_eq!(derive_name("node /opt/tools/server.js"), "server_js");
        // A direct program names itself.
        assert_eq!(derive_name("unity mcp"), "unity");
        assert_eq!(derive_name("/opt/bin/unity  mcp"), "unity");
        assert_eq!(derive_name("  "), "plugin");
        // Quoted paths survive.
        assert_eq!(derive_name("\"/opt/my tools/serve\" --x"), "serve");
    }

    #[test]
    fn args_split_on_whitespace_and_respect_quotes() {
        assert_eq!(split_args("-y rea-agents mcp"), ["-y", "rea-agents", "mcp"]);
        assert_eq!(split_args("--path \"/a b/c\""), ["--path", "/a b/c"]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn saving_plugins_keeps_the_explanatory_header() {
        let _lock = crate::env_lock::guard();
        let dir = std::env::temp_dir().join(format!("aura-harness-hdr-{}", uuid::Uuid::new_v4()));
        std::env::set_var("AURA_HARNESS_HOME", &dir);

        let mut plugins = Plugins::default();
        plugins.mcp.insert(
            "mnemai".into(),
            McpServer {
                command: "/bin/mnemai".into(),
                args: vec!["serve".into()],
                env: Default::default(),
                enabled: true,
            },
        );
        plugins.save().unwrap();

        let text = std::fs::read_to_string(dir.join("plugins.toml")).unwrap();
        assert!(text.starts_with("# Aura Harness plugins"));
        assert!(text.contains("[mcp.mnemai]"));

        // And the file still parses back into the same set.
        let loaded = Plugins::load().unwrap();
        assert_eq!(loaded.mcp["mnemai"].command, "/bin/mnemai");

        std::env::remove_var("AURA_HARNESS_HOME");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn aura_code_servers_are_imported_and_missing_config_is_fine() {
        let dir = std::env::temp_dir().join(format!("aura-harness-aura-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join(".aura")).unwrap();

        // Absent file: empty, not an error.
        assert!(aura_code_servers_in(&dir).is_empty());

        std::fs::write(
            dir.join(".aura/mcp.json"),
            r#"{"mcpServers":{
                "my-unity": {"command":"unity","args":["mcp"]},
                "plain": {"command":"/bin/tool","args":[],"env":{"K":"v"}}
            }}"#,
        )
        .unwrap();

        let mut found = aura_code_servers_in(&dir);
        found.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, "my_unity");
        assert_eq!(found[0].1.command, "unity");
        assert_eq!(found[0].1.args, ["mcp"]);
        assert_eq!(found[1].1.env["K"], "v");
        assert!(found.iter().all(|(_, s)| s.enabled));

        // Garbage is ignored rather than panicking.
        std::fs::write(dir.join(".aura/mcp.json"), "not json").unwrap();
        assert!(aura_code_servers_in(&dir).is_empty());

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn default_plugins_never_override_explicit_config() {
        let mut plugins = Plugins::default();
        plugins.mcp.insert(
            "mnemai".into(),
            McpServer {
                command: "/custom/mnemai".into(),
                args: vec!["serve".into()],
                env: Default::default(),
                enabled: true,
            },
        );
        let out = with_default_plugins(plugins);
        assert_eq!(out.mcp["mnemai"].command, "/custom/mnemai");
        assert_eq!(out.mcp.len(), 1);
    }

    #[test]
    fn default_plugins_attach_a_memory_server_when_present() {
        // Only asserts the shape: whichever branch fires on this machine, an
        // attached server must be a runnable command, not a bare name.
        let out = with_default_plugins(Plugins::default());
        if let Some(server) = out.mcp.get("mnemai") {
            assert!(!server.command.trim().is_empty());
            assert!(server.enabled);
        }
    }

    #[test]
    fn ensure_templates_writes_once_and_never_clobbers() {
        let _lock = crate::env_lock::guard();
        let dir = std::env::temp_dir().join(format!("aura-harness-tpl-{}", uuid::Uuid::new_v4()));
        std::env::set_var("AURA_HARNESS_HOME", &dir);

        ensure_templates().unwrap();
        assert!(dir.join("plugins.toml").exists());
        assert!(dir.join("config.toml").exists());

        // A user edit must survive a second run.
        std::fs::write(dir.join("plugins.toml"), "# mine\n").unwrap();
        ensure_templates().unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("plugins.toml")).unwrap(),
            "# mine\n"
        );

        // The template must parse as a valid (empty) plugin set.
        let parsed: Plugins = toml::from_str(PLUGINS_TEMPLATE).unwrap();
        assert!(parsed.mcp.is_empty() && parsed.cmd.is_empty());

        std::env::remove_var("AURA_HARNESS_HOME");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn credentials_round_trip() {
        let _lock = crate::env_lock::guard();
        let dir = std::env::temp_dir().join(format!("aura-harness-cred-{}", uuid::Uuid::new_v4()));
        std::env::set_var("AURA_HARNESS_HOME", &dir);
        std::env::remove_var("DEEPSEEK_API_KEY");
        store_api_key("sk-test-000").unwrap();
        let meta = std::fs::metadata(dir.join("credentials")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        assert_eq!(load_api_key().as_deref(), Some("sk-test-000"));
        std::env::remove_var("AURA_HARNESS_HOME");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
