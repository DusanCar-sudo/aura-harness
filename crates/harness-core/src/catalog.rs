//! Curated catalogue of MCP servers, skills, and workflows you can connect
//! to the harness. Each entry has a category + tags for semantic search:
//! typing "gaming" returns game-dev, 3D, and engine entries, typing
//! "social media" returns social Media MCPs, "3d game" returns 3D assets,
//! "summertime" returns the Summer Engine companion.
//!
//! Launch lines are unverified-at-runtime packages. Some need an API key or
//! path arg before they start — `needs_key` / `needs_args` flag those so the
//! browser can warn.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub launch_line: &'static str,
    /// Category for filter tabs in the UI: gaming, social, coding, etc.
    pub category: &'static str,
    /// Free-text tokens (lower-case) matched during search.
    pub tags: &'static str,
    /// Needs API key in environment.
    pub needs_key: bool,
    /// Needs a hand-supplied argument (path, connection string, …).
    pub needs_args: bool,
}

const fn entry(
    name: &'static str,
    description: &'static str,
    launch_line: &'static str,
    category: &'static str,
    tags: &'static str,
    needs_key: bool,
    needs_args: bool,
) -> CatalogEntry {
    CatalogEntry { name, description, launch_line, category, tags, needs_key, needs_args }
}

pub fn entries() -> &'static [CatalogEntry] {
    static ENTRIES: &[CatalogEntry] = &[
        // ── MCP servers (universal) ──────────────────────────────────────
        entry("memory", "Knowledge-graph memory read/write", "npx -y @modelcontextprotocol/server-memory", "core", "memory knowledge graph storage agent memory", false, false),
        entry("fetch", "Fetch a URL return clean text", "uvx mcp-server-fetch", "core", "http web fetch research", false, false),
        entry("sequential-thinking", "Step-by-step structured reasoning tool", "npx -y @modelcontextprotocol/server-sequential-thinking", "core", "reasoning think chain of thought planning", false, false),
        entry("time", "Current time and timezones", "npx -y @modelcontextprotocol/server-time", "core", "time date clock timezone", false, false),
        entry("everything", "Official reference MCP server exercising all features", "npx -y @modelcontextprotocol/server-everything", "core", "reference demo test", false, false),
        entry("filesystem", "Read and write files (append allowed dirs)", "npx -y @modelcontextprotocol/server-filesystem", "core", "files directories io storage", false, true),
        entry("git", "Read and manipulate a git repo (append path)", "npx -y @modelcontextprotocol/server-git", "core", "git version control repo github", false, true),
        entry("puppeteer", "Drive headless browser: screenshots clicks navigation", "npx -y @modelcontextprotocol/server-puppeteer", "web", "browser screenshot scrape click navigation web automation", false, false),
        entry("brave-search", "Web search via the Brave API", "npx -y @modelcontextprotocol/server-brave-search", "research", "search web google bing brave research", true, false),
        entry("github", "GitHub issues PRs and code", "npx -y @modelcontextprotocol/server-github", "coding", "github version control issues pull requests code", true, false),
        entry("postgres", "Query a Postgres database", "npx -y @modelcontextprotocol/server-postgres", "data", "postgres sql database query data", false, true),
        entry("google-maps", "Places directions geocoding", "npx -y @modelcontextprotocol/server-google-maps", "data", "maps location geo places address", true, false),
        // ── All Aura-defined MCPs ────────────────────────────────────────
        entry("mnemai", "Aura shared memory layer", "/home/dusan/.local/bin/mnemai serve", "memory", "memory shared aura mnemai knowledge context agentmem", false, false),
        // ── Game development ─────────────────────────────────────────────
        entry("gamedev-catalog", "Curated MCP for game-dev workflows (asset gen scene setup)", "npx -y gamedev-mcp", "gaming", "game gamedev 3d 2d unity godot unreal bevy phaser animation sprite texture scene", false, false),
        entry("summer-engine", "Summer Engine game creation tools (Godot-based)", "npx -y summer-engine-mcp", "gaming", "game 3d 2d scenes nodes script asset import export summer engine", false, false),
        entry("sketchfab", "Sketchfab 3D model search and download", "npx -y sketchfab-mcp", "gaming", "3d model asset sketchfab mesh texture game", true, false),
        // ── Social / communications ──────────────────────────────────────
        entry("twitter", "X/Twitter post and search", "npx -y twitter-mcp", "social", "social media twitter x post timeline tweet", true, true),
        entry("telegram", "Telegram bot messaging", "npx -y telegram-mcp", "social", "telegram messaging bot social chat", true, true),
        entry("slack", "Slack workspace integration", "npx -y slack-mcp", "social", "slack team chat messages channels", true, true),
        entry("discord", "Discord server integration", "npx -y discord-mcp", "social", "discord chat server messages guild", true, true),
        entry("reddit", "Reddit read and post", "npx -y reddit-mcp", "social", "reddit forum thread post social", true, false),
        entry("linkedin", "LinkedIn profile and connections", "npx -y linkedin-mcp", "social", "linkedin profile professional network", true, false),
        entry("instagram", "Instagram posts and insights", "npx -y instagram-mcp", "social", "instagram social photo post followers insights", true, false),
        // ── Productivity / office ────────────────────────────────────────
        entry("notion", "Notion pages databases and sync", "npx -y notion-mcp-server", "productivity", "notion notes documentation wiki database", true, false),
        entry("obsidian", "Obsidian vault read and write", "npx -y obsidian-mcp-server", "productivity", "obsidian notes markdown vault", false, true),
        entry("google-calendar", "Google Calendar events read create", "npx -y google-calendar-mcp", "productivity", "google calendar events schedule appointment meeting", true, true),
        entry("gmail", "Gmail read and send", "npx -y gmail-mcp", "productivity", "gmail email mail send read messages inbox", true, true),
        entry("dropbox", "Dropbox file storage sync", "npx -y dropbox-mcp", "storage", "dropbox storage cloud files sync", true, false),
        entry("s3", "Manage Amazon S3 buckets and objects", "npx -y s3-mcp-server", "storage", "s3 aws bucket storage object upload download", true, true),
        // ── Coding / devtools ────────────────────────────────────────────
        entry("docker", "Docker container and image management", "npx -y docker-mcp", "coding", "docker container image devops build", false, false),
        entry("kubernetes", "Kubernetes cluster management", "npx -y kubernetes-mcp", "coding", "kubernetes k8s cluster pod deployment devops", false, true),
        entry("npm", "Search npm packages info", "npx -y npm-mcp", "coding", "npm node javascript package library", false, false),
        entry("pypi", "Search python packages", "npx -y pypi-mcp", "coding", "pypi python package library pip", false, false),
        entry("shell-commands", "Allow the model to run shell commands", "npx -y mcp-server-shell", "coding", "shell command terminal bash CLI execute", false, false),
        entry("wallet", "Ethereum wallet tools: balance transfer nft", "npx -y ethereum-mcp", "web3", "wallet ethereum nft transfer balance web3 crypto", true, true),
    ];
    ENTRIES
}

pub fn categories() -> &'static [&'static str] {
    &["core", "gaming", "social", "productivity", "storage", "coding", "research", "web", "data", "web3"]
}

/// Normalize text for matching — lowercase, split on non-alphanumeric, ASCII.
fn normalize(text: &str) -> String {
    text.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect()
}

/// Tokenized keyword matching with category-aware boosting. Empty query lists
/// everything. Genuinely semantic enough for "gaming", "3d game", "social media",
/// "summertime" style queries.
pub fn search(query: &str) -> Vec<&'static CatalogEntry> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return entries().iter().collect();
    }
    // Expand query into individual tokens; full phrase also tested.
    let tokens: Vec<&str> = q.split_whitespace().collect();
    entries()
        .iter()
        .filter(|e| {
            // Match if name/description/category/tags contains the FULL phrase (spacing-insensitive)
            let haystack = format!("{} {} {} {}", e.name, e.description, e.category, e.tags);
            let normalized = normalize(&haystack);
            // Whole-phrase
            if normalized.contains(&q) { return true; }
            // Tokens overlap: at least one query token must be present.
            if tokens.iter().all(|t| t.len() < 3) { return false; }
            for t in &tokens {
                if t.len() < 3 { continue; }
                // Stem-lite: match if haystack contains token, or haystack token starts with token
                if normalized.contains(t) { return true; }
                // Common expansions
                let expanded = map_alias(t);
                if normalized.contains(expanded) { return true; }
            }
            false
        })
        .collect()
}

/// Map common user adjectives to their catalogue concept.
fn map_alias(tok: &str) -> &str {
    match tok {
        "gaming" | "games" | "game" => "game",
        "social" | "media" => "social",
        "3d" => "3d",
        "summertime" | "summer" => "summer",
        "assets" | "models" | "mesh" => "asset",
        "comms" | "communication" => "chat",
        "db" | "database" => "database",
        "web" => "web",
        "crypto" | "blockchain" | "nft" => "web3",
        "art" | "graphics" => "graphics",
        _ => tok,
    }
}

pub fn filter_by_category(category: &str) -> Vec<&'static CatalogEntry> {
    entries().iter().filter(|e| e.category == category).collect()
}

pub fn plugin_key(entry: &CatalogEntry) -> String {
    crate::config::normalize_name(entry.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entries_are_unique_and_complete() {
        let all = entries();
        assert!(!all.is_empty());
        assert!(all.len() >= 30, "catalog should have 30+ entries");
        for e in all {
            assert!(!e.name.is_empty());
            assert!(!e.description.is_empty());
            assert!(!e.launch_line.is_empty());
            assert!(!e.category.is_empty());
            assert!(!e.tags.is_empty());
            let argv = crate::config::split_args(e.launch_line);
            assert!(!argv.is_empty(), "{} has no argv", e.name);
        }
        let mut names: Vec<_> = all.iter().map(|e| e.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), all.len(), "duplicate catalogue names");
    }

    #[test]
    fn semantic_search_matches_examples() {
        // Gaming returns game, summer, 3d entries
        let gaming = search("gaming");
        assert!(!gaming.is_empty());
        assert!(gaming.iter().any(|e| e.category == "gaming"), "gaming should return gaming category");

        // Social media returns social
        let social = search("social media");
        assert!(!social.is_empty());
        assert!(social.iter().any(|e| e.category == "social"));

        // 3D game — 3D game entries
        let three = search("3d game");
        assert!(!three.is_empty());

        // Summertime — summer engine
        let summer = search("summertime");
        assert!(summer.iter().any(|e| e.name.contains("summer")), "summertime should match summer-engine");

        // Workflows-related (storage/productivity)
        let work = search("cloud storage");
        assert!(!work.is_empty());

        // Empty query lists everything
        assert_eq!(search("").len(), entries().len());
        assert_eq!(search("   ").len(), entries().len());
        // No match
        assert!(search("zzz-nothing").is_empty());
    }

    #[test]
    fn category_filter_works() {
        let gaming = filter_by_category("gaming");
        assert!(!gaming.is_empty());
        for e in gaming { assert_eq!(e.category, "gaming"); }
    }

    #[test]
    fn plugin_keys_are_safe() {
        for e in entries() {
            let k = plugin_key(e);
            assert!(!k.is_empty());
            assert!(k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        }
    }
}