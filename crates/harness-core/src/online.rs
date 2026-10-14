//! Online discovery: find connectable plugins beyond the local catalogue.
//!
//! Four read-only public sources, no API keys:
//! - the official **MCP registry** (searchable; entries carry installable
//!   packages with runtime hints)
//! - **GitHub repository search** (skill packs, plugin collections, MCP repos)
//! - a repo's **README** on raw.githubusercontent, scanned for a launch line
//!   so a pasted GitHub link can be installed directly
//! - the **agenttool.sh** directory of agent-ready tools (browse + link out)
//!
//! Security: launch lines derived from an internet README are *proposals* —
//! the caller decides whether to add them. Once added, the child still runs
//! under `child_env()` (PATH/HOME only), never with our secrets.

use crate::config;
use serde::Deserialize;
use std::time::Duration;

pub const USER_AGENT: &str = "aura-harness";
const REGISTRY_URL: &str = "https://registry.modelcontextprotocol.io/v0/servers";
const GITHUB_SEARCH_URL: &str = "https://api.github.com/search/repositories";
const AGENTTOOL_URL: &str = "https://agenttool.sh/api/tools";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Registry,
    Github,
    /// agenttool.sh service directory — browse + link, never spawnable.
    Directory,
}

/// One search hit from any online source.
#[derive(Debug, Clone)]
pub struct OnlineEntry {
    pub name: String,
    pub description: String,
    pub url: String,
    pub source: Source,
    pub stars: Option<u32>,
    /// Ready-to-paste launch line, when one can be derived.
    pub launch_line: Option<String>,
    /// Required environment variables the plugin asks for.
    pub needs_env: Vec<String>,
    /// agenttool.sh letter grade ("A", "B+"), when rated.
    pub grade: Option<String>,
    /// agenttool.sh category ("Developer Tools"), when known.
    pub category: Option<String>,
}

impl OnlineEntry {
    /// One-line detail shown next to the name in lists.
    pub fn detail(&self) -> String {
        let mut bits: Vec<String> = Vec::new();
        if let Some(grade) = &self.grade {
            let cat = self.category.clone().unwrap_or_default();
            if cat.is_empty() {
                bits.push(format!("grade {grade}"));
            } else {
                bits.push(format!("grade {grade} · {cat}"));
            }
        } else if let Some(stars) = self.stars {
            bits.push(format!("★{stars}"));
        }
        if let Some(line) = &self.launch_line {
            bits.push(line.clone());
        }
        bits.join("  ·  ")
    }
}

/// A launch line recovered from a GitHub repo's README.
#[derive(Debug, Clone)]
pub struct DerivedPlugin {
    pub name: String,
    pub launch_line: String,
    pub description: String,
    pub repo_url: String,
}

// ---------------------------------------------------------------------------
// URL parsing
// ---------------------------------------------------------------------------

/// Extract `(owner, repo)` from any GitHub repository URL.
///
/// Accepts `github.com/o/r`, trailing `/`, `.git`, `http(s)://`, `www.`,
/// and subpaths (`/tree/…`, `/blob/…`, `/releases`).
pub fn parse_github_repo_url(url: &str) -> Option<(String, String)> {
    let trimmed = url.trim().trim_end_matches('/');
    let rest = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
        .or_else(|| trimmed.strip_prefix("www.github.com/"))
        .or_else(|| trimmed.strip_prefix("github.com/"))
        .or_else(|| trimmed.strip_prefix("git@github.com:"))?;

    let mut parts = rest.split('/');
    let owner = parts.next()?.trim().to_string();
    let repo = parts.next()?.trim().trim_end_matches(".git").to_string();
    if owner.is_empty() || repo.is_empty() || owner.contains(' ') || repo.contains(' ') {
        return None;
    }
    Some((owner, repo))
}

/// True when the manual field holds something URL-shaped.
pub fn looks_like_github_url(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    (t.starts_with("http://")
        || t.starts_with("https://")
        || t.starts_with("www.")
        || t.starts_with("github.com")
        || t.starts_with("git@"))
        && t.contains("github.com")
}

// ---------------------------------------------------------------------------
// Registry search
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
struct RegResp {
    #[serde(default)]
    servers: Vec<RegItem>,
}

#[derive(Deserialize, Default)]
struct RegItem {
    #[serde(default)]
    server: RegServer,
}

#[derive(Deserialize, Default)]
struct RegServer {
    name: Option<String>,
    description: Option<String>,
    repository: Option<RegRepo>,
    #[serde(default)]
    packages: Vec<RegPkg>,
    #[serde(default)]
    remotes: Vec<RegRemote>,
}

#[derive(Deserialize)]
struct RegRepo {
    url: Option<String>,
}

#[derive(Deserialize)]
struct RegPkg {
    identifier: Option<String>,
    #[serde(rename = "runtimeHint")]
    runtime_hint: Option<String>,
    #[serde(rename = "runtimeArguments")]
    runtime_args: Option<Vec<RegArg>>,
    transport: Option<RegTransport>,
    #[serde(rename = "environmentVariables")]
    env_vars: Option<Vec<RegEnv>>,
}

#[derive(Deserialize)]
struct RegArg {
    value: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Deserialize)]
struct RegTransport {
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Deserialize)]
struct RegEnv {
    name: Option<String>,
    #[serde(rename = "isRequired")]
    required: Option<bool>,
}

#[derive(Deserialize)]
struct RegRemote {
    url: Option<String>,
}

/// Parse a registry search response body into entries.
pub fn parse_registry_json(body: &str) -> Vec<OnlineEntry> {
    let Ok(parsed) = serde_json::from_str::<RegResp>(body) else {
        return Vec::new();
    };
    parsed
        .servers
        .into_iter()
        .map(|item| {
            let s = item.server;
            let repo_url = s
                .repository
                .as_ref()
                .and_then(|r| r.url.clone())
                .or_else(|| s.remotes.first().and_then(|r| r.url.clone()))
                .unwrap_or_default();

            // Prefer a stdio package — that is what this harness can spawn.
            let stdio = s.packages.iter().find(|p| {
                p.transport
                    .as_ref()
                    .map(|t| t.kind.as_deref() != Some("streamable-http"))
                    .unwrap_or(true)
            });

            let launch_line = stdio.and_then(package_launch_line);
            let needs_env = stdio
                .map(|p| {
                    p.env_vars
                        .iter()
                        .flatten()
                        .filter(|e| e.required.unwrap_or(false))
                        .filter_map(|e| e.name.clone())
                        .collect()
                })
                .unwrap_or_default();

            OnlineEntry {
                name: s.name.unwrap_or_default(),
                description: s.description.unwrap_or_default(),
                url: repo_url,
                source: Source::Registry,
                stars: None,
                launch_line,
                needs_env,
                grade: None,
                category: None,
            }
        })
        .collect()
}

/// `runtimeHint` + positional args + identifier, e.g. `npx -y some-server`.
fn package_launch_line(pkg: &RegPkg) -> Option<String> {
    let hint = pkg.runtime_hint.as_deref()?;
    let identifier = pkg.identifier.as_deref()?;
    let mut parts = vec![hint.to_string()];
    for arg in pkg.runtime_args.iter().flatten() {
        if arg.kind.as_deref().unwrap_or("positional") == "positional" {
            if let Some(v) = &arg.value {
                parts.push(v.clone());
            }
        }
    }
    parts.push(identifier.to_string());
    Some(parts.join(" "))
}

// ---------------------------------------------------------------------------
// GitHub search
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
struct GhResp {
    #[serde(default)]
    items: Vec<GhItem>,
}

#[derive(Deserialize)]
struct GhItem {
    full_name: Option<String>,
    html_url: Option<String>,
    description: Option<String>,
    #[serde(rename = "stargazers_count")]
    stars: Option<u32>,
}

pub fn parse_github_search_json(body: &str) -> Vec<OnlineEntry> {
    let Ok(parsed) = serde_json::from_str::<GhResp>(body) else {
        return Vec::new();
    };
    parsed
        .items
        .into_iter()
        .map(|i| OnlineEntry {
            name: i.full_name.unwrap_or_default(),
            description: i.description.unwrap_or_default(),
            url: i.html_url.unwrap_or_default(),
            source: Source::Github,
            stars: i.stars,
            // GitHub repos are links, not spawn lines — the paste-analyze
            // path derives the line when the user actually wants to add one.
            launch_line: None,
            needs_env: Vec::new(),
            grade: None,
            category: None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Combined search
// ---------------------------------------------------------------------------

/// Search registry + GitHub in parallel. Returns `(entries, warning)`.
///
/// The warning carries rate-limit or connectivity trouble; entries from the
/// healthy source are still returned.
pub async fn search(query: &str, client: &reqwest::Client) -> (Vec<OnlineEntry>, Option<String>) {
    let q = query.trim();
    if q.is_empty() {
        return (Vec::new(), None);
    }

    let reg_fut = client
        .get(REGISTRY_URL)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .timeout(REQUEST_TIMEOUT)
        .query(&[("search", q), ("limit", "10")])
        .send();
    // GitHub search is ANDed across words; adding "claude" keeps results in
    // the agent-plugin space instead of generic repos.
    let gh_query = format!("{q} claude in:name,description");
    let gh_fut = client
        .get(GITHUB_SEARCH_URL)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .timeout(REQUEST_TIMEOUT)
        .query(&[("q", gh_query.as_str()), ("sort", "stars"), ("per_page", "10")])
        .send();

    let (reg, gh) = tokio::join!(reg_fut, gh_fut);
    let mut warnings = Vec::new();
    let mut entries = Vec::new();

    match reg {
        Ok(resp) if resp.status().is_success() => match resp.text().await {
            Ok(body) => entries.extend(parse_registry_json(&body)),
            Err(e) => warnings.push(format!("registry unreadable: {e}")),
        },
        Ok(resp) => warnings.push(format!("registry answered {}", resp.status())),
        Err(e) => warnings.push(format!("registry unreachable: {e}")),
    }

    let mut gh_body = None;
    match gh {
        Ok(resp) if resp.status().is_success() => match resp.text().await {
            Ok(body) => gh_body = Some(body),
            Err(e) => warnings.push(format!("github unreadable: {e}")),
        },
        Ok(resp) if resp.status().as_u16() == 403 || resp.status().as_u16() == 429 => {
            warnings.push("GitHub rate limit — try again in a minute".to_string());
        }
        Ok(resp) => warnings.push(format!("github answered {}", resp.status())),
        Err(e) => warnings.push(format!("github unreachable: {e}")),
    }

    if let Some(body) = gh_body {
        let mut gh_entries = parse_github_search_json(&body);
        if gh_entries.is_empty() {
            // Retry without the "claude" bias so narrow queries still hit.
            let plain = format!("{q} in:name,description");
            if let Ok(resp) = client
                .get(GITHUB_SEARCH_URL)
                .header(reqwest::header::USER_AGENT, USER_AGENT)
                .timeout(REQUEST_TIMEOUT)
                .query(&[("q", plain.as_str()), ("sort", "stars"), ("per_page", "10")])
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(body) = resp.text().await {
                        gh_entries = parse_github_search_json(&body);
                    }
                }
            }
        }
        entries.extend(gh_entries);
    }

    let warning = (!warnings.is_empty()).then(|| warnings.join(" · "));
    (entries, warning)
}

// ---------------------------------------------------------------------------
// agenttool.sh directory
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
struct AtResp {
    #[serde(default)]
    data: Vec<AtItem>,
    meta: Option<AtMeta>,
}

#[derive(Deserialize)]
struct AtMeta {
    page: Option<u32>,
    pages: Option<u32>,
}

#[derive(Deserialize)]
struct AtItem {
    name: Option<String>,
    url: Option<String>,
    description: Option<String>,
    category: Option<String>,
    #[serde(rename = "agentGrade")]
    grade: Option<String>,
}

/// Parse one agenttool.sh `/api/tools` page.
pub fn parse_agenttool_page(body: &str) -> (Vec<OnlineEntry>, Option<u32>) {
    let Ok(parsed) = serde_json::from_str::<AtResp>(body) else {
        return (Vec::new(), None);
    };
    let next_page = parsed.meta.as_ref().and_then(|m| {
        let page = m.page.unwrap_or(1);
        let pages = m.pages.unwrap_or(1);
        (page < pages).then_some(page + 1)
    });
    let entries = parsed
        .data
        .into_iter()
        .map(|i| OnlineEntry {
            name: i.name.unwrap_or_default(),
            description: i.description.unwrap_or_default(),
            url: i.url.unwrap_or_default(),
            source: Source::Directory,
            stars: None,
            // These are hosted services, not spawnable servers — browse and
            // link out; the user decides what to use them for.
            launch_line: None,
            needs_env: Vec::new(),
            grade: i.grade,
            category: i.category,
        })
        .collect();
    (entries, next_page)
}

/// Download the full agenttool.sh directory (paged, capped for sanity).
///
/// ~194 tools today; three pages of 100 covers growth for a long while.
pub async fn fetch_directory(client: &reqwest::Client) -> Vec<OnlineEntry> {
    let mut all = Vec::new();
    let mut page = 1u32;
    loop {
        let resp = client
            .get(AGENTTOOL_URL)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .timeout(REQUEST_TIMEOUT)
            .query(&[("limit", "100"), ("page", &page.to_string())])
            .send()
            .await;
        let Ok(resp) = resp else { break };
        if !resp.status().is_success() {
            break;
        }
        let Ok(body) = resp.text().await else { break };
        let (entries, next) = parse_agenttool_page(&body);
        let got = entries.len();
        all.extend(entries);
        match next {
            Some(n) if got > 0 && page < 3 => page = n,
            _ => break,
        }
    }
    all
}

/// Case-insensitive substring filter across name/description/category/grade.
pub fn filter_directory<'a>(entries: &'a [OnlineEntry], query: &str) -> Vec<&'a OnlineEntry> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    entries
        .iter()
        .filter(|e| {
            e.name.to_lowercase().contains(&q)
                || e.description.to_lowercase().contains(&q)
                || e.category.as_deref().unwrap_or_default().to_lowercase().contains(&q)
                || e.grade.as_deref().unwrap_or_default().to_lowercase().contains(&q)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// README analysis (paste-a-link install)
// ---------------------------------------------------------------------------

/// Derive an installable plugin from a GitHub repository link.
///
/// Fetches the README and looks, in order, for: a `mcpServers` JSON block, a
/// `claude mcp add … -- cmd` line, or a bare `npx`/`uvx`/`pipx run` line.
pub async fn analyze_github(url: &str, client: &reqwest::Client) -> Result<DerivedPlugin, String> {
    let (owner, repo) =
        parse_github_repo_url(url).ok_or("that doesn't look like a GitHub repository link")?;

    let mut readme = None;
    for name in ["README.md", "readme.md"] {
        let raw = format!("https://raw.githubusercontent.com/{owner}/{repo}/HEAD/{name}");
        match client
            .get(&raw)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(text) = resp.text().await {
                    readme = Some(text);
                    break;
                }
            }
            _ => continue,
        }
    }
    let readme = readme.ok_or("couldn't fetch that repository's README")?;

    let launch_line = extract_launch_line(&readme).ok_or(
        "found the repo, but its README shows no launch command (no mcpServers JSON, \
         no npx/uvx line). Copy the install command from the repo and paste it here.",
    )?;

    let name = config::derive_name(&launch_line);
    Ok(DerivedPlugin {
        name,
        launch_line,
        description: first_heading(&readme),
        repo_url: format!("https://github.com/{owner}/{repo}"),
    })
}

/// Find a launch command in README text. Pure — fixture-tested.
pub fn extract_launch_line(readme: &str) -> Option<String> {
    // 1. `mcpServers`-style JSON: "command" plus its "args" array.
    if let Some(line) = extract_json_launch(readme) {
        return Some(line);
    }
    // 2. `claude mcp add <name> -- <command…>`
    for raw in readme.lines() {
        if raw.contains("mcp add") {
            if let Some(after) = raw.split("--").nth(1) {
                if let Some(line) = take_launch_prefix(after) {
                    return Some(line);
                }
            }
        }
    }
    // 3. Bare or backticked launch lines.
    for candidate in code_spans(readme) {
        if let Some(line) = take_launch_prefix(&candidate) {
            return Some(line);
        }
    }
    None
}

/// Recognize `npx` / `uvx` / `pipx run` / `uv run` invocations only —
/// deliberately not `npm install` (that is setup, not a launch line).
fn take_launch_prefix(text: &str) -> Option<String> {
    // Cut at a closing backtick so "`npx -y x` and go." → "npx -y x".
    let lead_stripped = text.trim().trim_matches('`');
    let upto_backtick = lead_stripped.split('`').next().unwrap_or(lead_stripped);
    let trimmed = upto_backtick.trim();
    for prefix in ["npx ", "uvx ", "pipx run ", "uv run "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            // Must name a package: first non-flag token.
            let argv = config::split_args(rest);
            if argv.iter().any(|t| !t.starts_with('-')) {
                return Some(config::split_args(trimmed)
                    .iter()
                    .map(|t| t.as_str())
                    .collect::<Vec<_>>()
                    .join(" "));
            }
        }
    }
    None
}

/// Lines and backtick spans — install sections are usually fenced or inline.
fn code_spans(readme: &str) -> Vec<String> {
    let mut out: Vec<String> = readme.lines().map(|l| l.to_string()).collect();
    for part in readme.split('`') {
        let t = part.trim();
        if !t.is_empty() && t.lines().count() <= 3 {
            out.push(t.to_string());
        }
    }
    out
}

/// Extract `"command": "npx", "args": ["-y", "pkg"]` from JSON-ish text.
fn extract_json_launch(readme: &str) -> Option<String> {
    let mut search_from = 0usize;
    while let Some(rel) = readme[search_from..].find("\"command\"") {
        let at = search_from + rel;
        search_from = at + 1;
        let window_end = (at + 600).min(readme.len());
        let window = &readme[at..window_end];

        let Some((_, after)) = window.split_once(':') else {
            continue;
        };
        let after = after.trim_start();
        let Some(rest) = after.strip_prefix('"') else {
            continue;
        };
        let Some(command) = rest.split('"').next() else {
            continue;
        };
        if command.is_empty() {
            continue;
        }
        let command = command.to_string();

        let Some(args_rel) = window.find("\"args\"") else {
            continue;
        };
        let args_region = &window[args_rel..];
        let Some(open) = args_region.find('[') else {
            continue;
        };
        let Some(close) = bracket_match(&args_region[open..]) else {
            continue;
        };
        let args_json = &args_region[open..=open + close];
        let Ok(args) = serde_json::from_str::<Vec<String>>(args_json) else {
            continue;
        };

        let mut parts = vec![command];
        parts.extend(args);
        return Some(parts.join(" "));
    }
    None
}

/// Index of the `]` closing the `[` at position 0 of `text`.
fn bracket_match(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, c) in text.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match c {
            '\\' if in_string => escape = true,
            '"' => in_string = !in_string,
            '[' if !in_string => depth += 1,
            ']' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// First `# …` heading, else first non-empty line; capped for UI rows.
fn first_heading(readme: &str) -> String {
    let text = readme
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let cleaned = text.trim_start_matches('#').trim();
    let mut out: String = cleaned.chars().take(100).collect();
    if cleaned.chars().count() > 100 {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGISTRY_FIXTURE: &str = r#"{"servers":[
        {"server":{
            "name":"com.example/fs",
            "description":"file access",
            "repository":{"url":"https://github.com/example/fs"},
            "packages":[{
                "registryType":"npm",
                "identifier":"fs-mcp-server",
                "runtimeHint":"npx",
                "runtimeArguments":[{"value":"-y","type":"positional"}],
                "transport":{"type":"stdio"},
                "environmentVariables":[{"name":"FS_TOKEN","isRequired":true}]
            }]
        }},
        {"server":{
            "name":"example/remote-only",
            "description":"http only",
            "remotes":[{"type":"streamable-http","url":"https://x.example/mcp"}]
        }}
    ]}"#;

    const GITHUB_FIXTURE: &str = r#"{"items":[
        {"full_name":"sponticelli/gamedev-claude-plugins",
         "html_url":"https://github.com/sponticelli/gamedev-claude-plugins",
         "description":"a comprehensive plugin marketplace",
         "stargazers_count":16}
    ]}"#;

    const AGENTTOOL_FIXTURE: &str = r#"{"data":[
        {"name":"GitHub","url":"https://github.com","description":"Code hosting",
         "category":"Developer Tools","agentGrade":"A"},
        {"name":"Stripe","url":"https://stripe.com","description":"Payments",
         "category":"Payments","agentGrade":"B+"}
    ],"meta":{"total":194,"page":1,"pages":4}}"#;

    #[test]
    fn parses_registry_entries() {
        let entries = parse_registry_json(REGISTRY_FIXTURE);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "com.example/fs");
        assert_eq!(entries[0].launch_line.as_deref(), Some("npx -y fs-mcp-server"));
        assert_eq!(entries[0].needs_env, ["FS_TOKEN"]);
        assert_eq!(entries[0].source, Source::Registry);
        // Remote-only: linkable but not spawnable.
        assert_eq!(entries[1].launch_line, None);
        assert_eq!(entries[1].url, "https://x.example/mcp");
    }

    #[test]
    fn parses_github_entries() {
        let entries = parse_github_search_json(GITHUB_FIXTURE);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "sponticelli/gamedev-claude-plugins");
        assert_eq!(entries[0].stars, Some(16));
        assert_eq!(entries[0].source, Source::Github);
        assert!(entries[0].launch_line.is_none());
    }

    #[test]
    fn parses_agenttool_pages() {
        let (entries, next) = parse_agenttool_page(AGENTTOOL_FIXTURE);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "GitHub");
        assert_eq!(entries[0].grade.as_deref(), Some("A"));
        assert_eq!(entries[0].category.as_deref(), Some("Developer Tools"));
        assert_eq!(entries[0].source, Source::Directory);
        assert!(entries[0].launch_line.is_none());
        assert_eq!(next, Some(2), "page 1 of 4 must point onward");
    }

    #[test]
    fn last_page_has_no_next() {
        let body = r#"{"data":[{"name":"X","url":"u","agentGrade":"C"}],
                       "meta":{"total":1,"page":4,"pages":4}}"#;
        let (_, next) = parse_agenttool_page(body);
        assert_eq!(next, None);
    }

    #[test]
    fn directory_filter_matches_across_fields() {
        let (entries, _) = parse_agenttool_page(AGENTTOOL_FIXTURE);
        assert_eq!(filter_directory(&entries, "git").len(), 1);
        assert_eq!(filter_directory(&entries, "payments").len(), 1);
        assert_eq!(filter_directory(&entries, "b+").len(), 1);
        assert!(filter_directory(&entries, "").is_empty());
        assert!(filter_directory(&entries, "zzz").is_empty());
    }

    #[test]
    fn garbage_becomes_empty_not_panic() {
        assert!(parse_registry_json("not json").is_empty());
        assert!(parse_github_search_json("{}").is_empty());
        assert!(parse_agenttool_page("nope").0.is_empty());
    }

    #[test]
    fn parses_github_urls() {
        assert_eq!(
            parse_github_repo_url("https://github.com/AnonyJcy/dsh-j-space"),
            Some(("AnonyJcy".into(), "dsh-j-space".into()))
        );
        assert_eq!(
            parse_github_repo_url("https://github.com/a/b/"),
            Some(("a".into(), "b".into()))
        );
        assert_eq!(
            parse_github_repo_url("https://github.com/a/b.git"),
            Some(("a".into(), "b".into()))
        );
        assert_eq!(
            parse_github_repo_url("https://github.com/a/b/tree/main/crates"),
            Some(("a".into(), "b".into()))
        );
        assert_eq!(
            parse_github_repo_url("git@github.com:a/b.git"),
            Some(("a".into(), "b".into()))
        );
        assert_eq!(parse_github_repo_url("https://gitlab.com/a/b"), None);
        assert_eq!(parse_github_repo_url("https://github.com/only-owner"), None);
    }

    #[test]
    fn detects_url_shaped_manual_input() {
        assert!(looks_like_github_url("https://github.com/AnonyJcy/dsh-j-space"));
        assert!(looks_like_github_url("github.com/a/b"));
        assert!(!looks_like_github_url("npx -y something"));
        assert!(!looks_like_github_url("gitlab.com/a/b"));
    }

    #[test]
    fn extracts_mcp_json_block() {
        let readme = r#"
# My server
```json
{
  "mcpServers": {
    "demo": {
      "command": "npx",
      "args": ["-y", "@scope/demo-mcp", "--port", "3000"]
    }
  }
}
```
"#;
        assert_eq!(
            extract_launch_line(readme).as_deref(),
            Some("npx -y @scope/demo-mcp --port 3000")
        );
    }

    #[test]
    fn extracts_claude_mcp_add_line() {
        let readme = r#"Install: `claude mcp add weather -- npx -y weather-mcp` and go."#;
        assert_eq!(
            extract_launch_line(readme).as_deref(),
            Some("npx -y weather-mcp")
        );
    }

    #[test]
    fn extracts_bare_npx_line() {
        let readme = "Run it with:\n\n    npx -y some-pkg --verbose\n";
        assert_eq!(
            extract_launch_line(readme).as_deref(),
            Some("npx -y some-pkg --verbose")
        );
    }

    #[test]
    fn ignores_install_instructions() {
        let readme = "First `npm install my-pkg`, then configure it.";
        assert_eq!(extract_launch_line(readme), None);
    }

    #[test]
    fn returns_none_when_nothing_found() {
        assert_eq!(extract_launch_line("# Just docs, no install"), None);
        assert_eq!(extract_launch_line(""), None);
    }

    #[test]
    fn first_heading_is_capped() {
        let long = format!("# {}\nbody", "x".repeat(300));
        let h = first_heading(&long);
        assert!(h.chars().count() <= 101);
        assert!(!h.starts_with('#'));
    }

    #[test]
    fn bracket_match_handles_nested_and_strings() {
        assert_eq!(bracket_match("[[1]]"), Some(4));
        assert_eq!(bracket_match(r#"["]"]"#), Some(4));
        assert_eq!(bracket_match("[1, 2"), None);
    }

    #[test]
    fn entry_detail_line_shows_grade_and_stars() {
        let (entries, _) = parse_agenttool_page(AGENTTOOL_FIXTURE);
        assert!(entries[0].detail().contains("grade A"));
        assert!(entries[0].detail().contains("Developer Tools"));
        let gh = parse_github_search_json(GITHUB_FIXTURE);
        assert!(gh[0].detail().contains("★16"));
    }
}
