//! A small curated catalogue of MCP servers people commonly connect.
//!
//! These are launch lines, not verified-at-runtime packages: some need an API
//! key or a path argument before they start, which each entry flags so the
//! browser can say so. The manual "paste a command" path stays for anything
//! not listed.

/// One connectable server in the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogEntry {
    /// Short name shown in the list.
    pub name: &'static str,
    /// One-line description.
    pub description: &'static str,
    /// The launch line, split-safe via `config::split_args`.
    pub launch_line: &'static str,
    /// Needs an API key in the environment before it will start.
    pub needs_key: bool,
    /// Needs an argument (a path, a query, …) appended by hand.
    pub needs_args: bool,
}

const fn entry(
    name: &'static str,
    description: &'static str,
    launch_line: &'static str,
    needs_key: bool,
    needs_args: bool,
) -> CatalogEntry {
    CatalogEntry {
        name,
        description,
        launch_line,
        needs_key,
        needs_args,
    }
}

/// The curated set, ordered by how often they are wanted.
pub fn entries() -> &'static [CatalogEntry] {
    static ENTRIES: &[CatalogEntry] = &[
        entry(
            "memory",
            "Knowledge-graph memory the model can read and write",
            "npx -y @modelcontextprotocol/server-memory",
            false,
            false,
        ),
        entry(
            "fetch",
            "Fetch a URL and return it as clean text",
            "uvx mcp-server-fetch",
            false,
            false,
        ),
        entry(
            "sequential-thinking",
            "Structured step-by-step reasoning tool",
            "npx -y @modelcontextprotocol/server-sequential-thinking",
            false,
            false,
        ),
        entry(
            "time",
            "Current time and timezones",
            "npx -y @modelcontextprotocol/server-time",
            false,
            false,
        ),
        entry(
            "everything",
            "Official reference server — exercises every MCP feature",
            "npx -y @modelcontextprotocol/server-everything",
            false,
            false,
        ),
        entry(
            "filesystem",
            "Read and write files (append the directories it may touch)",
            "npx -y @modelcontextprotocol/server-filesystem",
            false,
            true,
        ),
        entry(
            "git",
            "Read and manipulate a git repository (append its path)",
            "npx -y @modelcontextprotocol/server-git",
            false,
            true,
        ),
        entry(
            "puppeteer",
            "Drive a headless browser: screenshots, clicks, navigation",
            "npx -y @modelcontextprotocol/server-puppeteer",
            false,
            false,
        ),
        entry(
            "brave-search",
            "Web search via the Brave API (set BRAVE_API_KEY)",
            "npx -y @modelcontextprotocol/server-brave-search",
            true,
            false,
        ),
        entry(
            "github",
            "GitHub issues, PRs and code (set GITHUB_TOKEN)",
            "npx -y @modelcontextprotocol/server-github",
            true,
            false,
        ),
        entry(
            "postgres",
            "Query a Postgres database (append the connection string)",
            "npx -y @modelcontextprotocol/server-postgres",
            false,
            true,
        ),
    ];
    ENTRIES
}

/// Filter the catalogue by name or description; empty query lists everything.
pub fn search(query: &str) -> Vec<&'static CatalogEntry> {
    let q = query.trim().to_lowercase();
    entries()
        .iter()
        .filter(|e| {
            q.is_empty()
                || e.name.contains(&q)
                || e.description.to_lowercase().contains(&q)
        })
        .collect()
}

/// Name the plugin would get if added from this entry.
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
        for e in all {
            assert!(!e.name.is_empty());
            assert!(!e.description.is_empty());
            assert!(!e.launch_line.is_empty());
            // Every launch line must survive argv splitting with a program.
            let argv = crate::config::split_args(e.launch_line);
            assert!(!argv.is_empty(), "{} has no argv", e.name);
        }
        let mut names: Vec<_> = all.iter().map(|e| e.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), all.len(), "duplicate catalogue names");
    }

    #[test]
    fn search_matches_name_and_description() {
        assert!(search("git").iter().any(|e| e.name == "git"));
        assert!(search("GITHUB").iter().any(|e| e.name == "github"));
        assert!(search("memory").iter().any(|e| e.name == "memory"));
        assert_eq!(search("  ").len(), entries().len());
        assert!(search("zzz-nothing").is_empty());
    }

    #[test]
    fn keys_are_stable_and_safe() {
        let e = &entries()[0];
        let key = plugin_key(e);
        assert!(!key.is_empty());
        assert!(key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        assert_eq!(plugin_key(e), key, "keys must be deterministic");
    }
}
