//! Built-in tools every harness has, no plugins required.
//!
//! `shell` is the star: it runs through `/bin/sh` so pipes, globs and
//! redirects work, and like every mutating tool it waits for user approval.
//! Read-only tools (`read_file`, `list_files`, `grep`, `glob`) run free.

use crate::api::ToolDef;
use crate::error::{Error, Result};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

/// Shell commands can legitimately be long-running; still capped.
pub const SHELL_TIMEOUT: Duration = Duration::from_secs(120);

const LIST_CAP: usize = 500;
const GREP_CAP: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinTool {
    Shell,
    ReadFile,
    WriteFile,
    EditFile,
    ListFiles,
    Grep,
    Glob,
}

impl BuiltinTool {
    pub fn name(&self) -> &'static str {
        match self {
            BuiltinTool::Shell => "shell",
            BuiltinTool::ReadFile => "read_file",
            BuiltinTool::WriteFile => "write_file",
            BuiltinTool::EditFile => "edit_file",
            BuiltinTool::ListFiles => "list_files",
            BuiltinTool::Grep => "grep",
            BuiltinTool::Glob => "glob",
        }
    }

    /// Mutating tools pause for the user's verdict, like command tools.
    pub fn needs_approval(&self) -> bool {
        matches!(
            self,
            BuiltinTool::Shell | BuiltinTool::WriteFile | BuiltinTool::EditFile
        )
    }

    pub fn all() -> [BuiltinTool; 7] {
        [
            BuiltinTool::Shell,
            BuiltinTool::ReadFile,
            BuiltinTool::WriteFile,
            BuiltinTool::EditFile,
            BuiltinTool::ListFiles,
            BuiltinTool::Grep,
            BuiltinTool::Glob,
        ]
    }

    pub fn def(&self) -> ToolDef {
        ToolDef {
            name: self.name().into(),
            description: self.description().into(),
            parameters: self.schema(),
        }
    }

    fn description(&self) -> &'static str {
        match self {
            BuiltinTool::Shell => "Run a shell command via /bin/sh (pipes, redirects, globs). Requires approval.",
            BuiltinTool::ReadFile => "Read a text file, with optional 1-based line offset and limit.",
            BuiltinTool::WriteFile => "Create or overwrite a file with exact content. Requires approval.",
            BuiltinTool::EditFile => "Replace exact text inside a file. Requires approval.",
            BuiltinTool::ListFiles => "List files and directories (optionally recursive).",
            BuiltinTool::Grep => "Search file contents for a regex pattern.",
            BuiltinTool::Glob => "Find files whose path matches a glob pattern (*, ?, **).",
        }
    }

    fn schema(&self) -> Value {
        let str_prop = |desc: &str| {
            serde_json::json!({"type": "string", "description": desc})
        };
        match self {
            BuiltinTool::Shell => serde_json::json!({
                "type": "object",
                "properties": {"command": str_prop("Shell command to run")},
                "required": ["command"],
            }),
            BuiltinTool::ReadFile => serde_json::json!({
                "type": "object",
                "properties": {
                    "path": str_prop("File path to read"),
                    "offset": {"type": "integer", "minimum": 1, "description": "Skip lines before this 1-based line"},
                    "limit": {"type": "integer", "minimum": 1, "description": "Return at most this many lines"},
                },
                "required": ["path"],
            }),
            BuiltinTool::WriteFile => serde_json::json!({
                "type": "object",
                "properties": {
                    "path": str_prop("File path to write"),
                    "content": str_prop("Exact file content"),
                },
                "required": ["path", "content"],
            }),
            BuiltinTool::EditFile => serde_json::json!({
                "type": "object",
                "properties": {
                    "path": str_prop("File path to edit"),
                    "old_string": str_prop("Exact text to find"),
                    "new_string": str_prop("Replacement text"),
                    "replace_all": {"type": "boolean", "description": "Replace every occurrence (default false)"},
                },
                "required": ["path", "old_string", "new_string"],
            }),
            BuiltinTool::ListFiles => serde_json::json!({
                "type": "object",
                "properties": {
                    "path": str_prop("Directory to list (default: current directory)"),
                    "recursive": {"type": "boolean", "description": "List subdirectories too (default false)"},
                },
                "required": [],
            }),
            BuiltinTool::Grep => serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": str_prop("Regex pattern to search for"),
                    "path": str_prop("File or directory to search (default: current directory)"),
                    "glob": str_prop("Optional glob filter, e.g. *.rs"),
                },
                "required": ["pattern"],
            }),
            BuiltinTool::Glob => serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": str_prop("Glob pattern, e.g. src/**/*.rs"),
                    "path": str_prop("Directory to search (default: current directory)"),
                },
                "required": ["pattern"],
            }),
        }
    }

    /// Execute with the raw JSON arguments the model produced. Relative paths
    /// and the shell resolve against `cwd`; an empty `cwd` means the process's.
    pub async fn run(&self, arguments: &str, cwd: &Path) -> Result<String> {
        let args: Value = serde_json::from_str(arguments).unwrap_or(Value::Null);
        let get = |key: &str| args.get(key).and_then(Value::as_str);
        match self {
            BuiltinTool::Shell => shell(get("command").unwrap_or(""), cwd).await,
            BuiltinTool::ReadFile => {
                read_file(
                    cwd,
                    get("path").unwrap_or("."),
                    args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize,
                    args.get("limit").and_then(Value::as_u64).unwrap_or(usize::MAX as u64) as usize,
                )
                .await
            }
            BuiltinTool::WriteFile => {
                write_file(
                    cwd,
                    get("path").unwrap_or(""),
                    get("content").unwrap_or(""),
                )
                .await
            }
            BuiltinTool::EditFile => {
                edit_file(
                    cwd,
                    get("path").unwrap_or(""),
                    get("old_string").unwrap_or(""),
                    get("new_string").unwrap_or(""),
                    args.get("replace_all").and_then(Value::as_bool).unwrap_or(false),
                )
                .await
            }
            BuiltinTool::ListFiles => {
                list_files(
                    cwd,
                    get("path").unwrap_or("."),
                    args.get("recursive").and_then(Value::as_bool).unwrap_or(false),
                )
                .await
            }
            BuiltinTool::Grep => {
                grep(
                    cwd,
                    get("pattern").unwrap_or(""),
                    get("path").unwrap_or("."),
                    get("glob"),
                )
                .await
            }
            BuiltinTool::Glob => {
                glob_files(cwd, get("pattern").unwrap_or(""), get("path").unwrap_or(".")).await
            }
        }
    }
}

fn child_command(prog: &str, args: &[&str]) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(prog);
    cmd.args(args)
        .env_clear()
        .envs(crate::child_env().iter().map(|(k, v)| (k.clone(), v.clone())));
    cmd
}

/// Resolve a model-supplied path: relative paths hang off `cwd`, absolute ones win.
fn resolve(cwd: &Path, path: &str) -> std::path::PathBuf {
    // `components()` drops interior `.`, so "." names the folder itself, not `<cwd>/.`.
    cwd.join(path).components().collect()
}

/// Run a shell command through /bin/sh so pipes and redirects behave.
pub async fn shell(command: &str, cwd: &Path) -> Result<String> {
    let mut cmd = child_command("/bin/sh", &["-c", command]);
    if !cwd.as_os_str().is_empty() {
        cmd.current_dir(cwd);
    }
    let output = tokio::time::timeout(SHELL_TIMEOUT, cmd.output())
        .await
        .map_err(|_| Error::Invalid("shell command timed out".into()))?
        .map_err(|e| Error::Invalid(format!("shell (in {}): {e}", cwd.display())))?;

    let mut out = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.stderr.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("[stderr]\n");
        out.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    if !output.status.success() {
        out.push_str(&format!(
            "\n[exit code: {}]",
            output.status.code().unwrap_or(-1)
        ));
    }
    if out.is_empty() {
        out.push_str("[no output]");
    }
    Ok(out)
}

async fn read_file(cwd: &Path, path: &str, offset: usize, limit: usize) -> Result<String> {
    let p = resolve(cwd, path);
    let p = p.as_path();
    if p.is_dir() {
        return Err(Error::Invalid(format!("{path} is a directory")));
    }
    let bytes = tokio::fs::read(p)
        .await
        .map_err(|e| Error::Invalid(format!("read {path}: {e}")))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| Error::Invalid(format!("{path} is not valid UTF-8 text")))?;
    let lines: Vec<&str> = text.lines().collect();
    let start = offset.saturating_sub(1).min(lines.len());
    let end = (start + limit).min(lines.len());
    let mut out = lines[start..end].join("\n");
    out.push_str(&format!("\n[{} lines total]", lines.len()));
    Ok(out)
}

async fn write_file(cwd: &Path, path: &str, content: &str) -> Result<String> {
    if path.is_empty() {
        return Err(Error::Invalid("write_file needs a path".into()));
    }
    let p = resolve(cwd, path);
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
    }
    tokio::fs::write(&p, content)
        .await
        .map_err(|e| Error::Invalid(format!("write {path}: {e}")))?;
    Ok(format!("wrote {} bytes to {}", content.len(), path))
}

async fn edit_file(cwd: &Path, path: &str, old: &str, new: &str, replace_all: bool) -> Result<String> {
    if path.is_empty() {
        return Err(Error::Invalid("edit_file needs a path".into()));
    }
    if old.is_empty() {
        return Err(Error::Invalid("old_string must not be empty".into()));
    }
    let full = resolve(cwd, path);
    let bytes = tokio::fs::read(&full)
        .await
        .map_err(|e| Error::Invalid(format!("read {path}: {e}")))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| Error::Invalid(format!("{path} is not valid UTF-8 text")))?;
    let count = text.matches(old).count();
    if count == 0 {
        return Err(Error::Invalid(format!("old_string not found in {path}")));
    }
    let replaced = if replace_all {
        text.replace(old, new)
    } else {
        text.replacen(old, new, 1)
    };
    tokio::fs::write(&full, replaced)
        .await
        .map_err(|e| Error::Invalid(format!("write {path}: {e}")))?;
    Ok(format!("edited {path}: {count} occurrence(s) replaced"))
}

async fn list_files(cwd: &Path, path: &str, recursive: bool) -> Result<String> {
    let root = resolve(cwd, path);
    let root = root.as_path();
    if !root.exists() {
        return Err(Error::Invalid(format!("{path} does not exist")));
    }
    let mut lines: Vec<String> = Vec::new();
    if recursive {
        for entry in walkdir::WalkDir::new(root)
            .sort_by_file_name()
            .into_iter()
            .filter_map(|e| e.ok())
            .take(LIST_CAP)
        {
            let md = entry.metadata().ok();
            let kind = if entry.file_type().is_dir() {
                "dir "
            } else {
                "file"
            };
            let size = md.map(|m| m.len()).unwrap_or(0);
            let name = entry.file_name().to_string_lossy();
            lines.push(format!("{kind} {size:>10} {name}"));
        }
    } else {
        let mut rd = tokio::fs::read_dir(root)
            .await
            .map_err(|e| Error::Invalid(format!("list {path}: {e}")))?;
        let mut n = 0;
        while let Ok(Some(entry)) = rd.next_entry().await {
            if n >= LIST_CAP {
                break;
            }
            n += 1;
            let name = entry.file_name().to_string_lossy().to_string();
            let kind = match entry.file_type().await {
                Ok(ft) if ft.is_dir() => "dir ",
                Ok(_) => "file",
                Err(_) => "unk ",
            };
            let size = entry.metadata().await.map(|m| m.len()).unwrap_or(0);
            lines.push(format!("{kind} {size:>10} {name}"));
        }
    }
    if lines.is_empty() {
        lines.push(format!("{path} is empty"));
    } else {
        lines.push(format!("[{} entries shown]", lines.len()));
    }
    Ok(lines.join("\n"))
}

/// Convert a glob pattern to an anchored regex: `**` crosses `/`, `*` and `?` do not.
fn glob_to_regex(pattern: &str) -> Result<regex::Regex> {
    let mut re = String::from("^");
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => {
                if chars.peek() == Some(&'*') {
                    chars.next();
                    if chars.peek() == Some(&'/') {
                        // `**/` spans zero or more directories.
                        chars.next();
                        re.push_str("(?:.*/)?");
                    } else {
                        re.push_str(".*");
                    }
                } else {
                    re.push_str("[^/]*");
                }
            }
            '?' => re.push_str("[^/]"),
            '.' | '+' | '(' | ')' | '|' | '^' | '$' | '{' | '}' | '[' | ']' | '\\' => {
                re.push('\\');
                re.push(c);
            }
            _ => re.push(c),
        }
    }
    re.push('$');
    regex::Regex::new(&re).map_err(|e| Error::Invalid(format!("glob {pattern}: {e}")))
}

async fn glob_files(cwd: &Path, pattern: &str, path: &str) -> Result<String> {
    let re = glob_to_regex(pattern)?;
    let root = resolve(cwd, path);
    let root = root.as_path();
    let mut hits: Vec<String> = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
        .take(LIST_CAP)
    {
        // Match the pattern against the path relative to the search root, so
        // `src/*.rs` finds `<root>/src/main.rs` the way a shell would.
        let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());
        if re.is_match(&rel.to_string_lossy()) {
            hits.push(entry.path().to_string_lossy().into_owned());
        }
    }
    if hits.is_empty() {
        hits.push(format!("no matches for {pattern} under {path}"));
    } else {
        hits.push(format!("[{} matches]", hits.len()));
    }
    Ok(hits.join("\n"))
}

async fn grep(cwd: &Path, pattern: &str, path: &str, glob: Option<&str>) -> Result<String> {
    if pattern.is_empty() {
        return Err(Error::Invalid("grep needs a pattern".into()));
    }
    let re = regex::Regex::new(pattern)
        .map_err(|e| Error::Invalid(format!("grep pattern: {e}")))?;
    let glob_re = match glob {
        Some(g) => Some(glob_to_regex(g)?),
        None => None,
    };
    let mut hits: Vec<String> = Vec::new();
    let root = resolve(cwd, path);
    for entry in walkdir::WalkDir::new(&root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let s = entry.path().to_string_lossy();
        if let Some(g) = &glob_re {
            // Same rule as `glob`: the filter sees the path relative to the search root.
            let rel = entry.path().strip_prefix(&root).unwrap_or(entry.path());
            if !g.is_match(&rel.to_string_lossy()) {
                continue;
            }
        }
        let Ok(bytes) = tokio::fs::read(entry.path()).await else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            if re.is_match(line) {
                hits.push(format!("{}:{}:{}", s, i + 1, line.trim()));
                if hits.len() >= GREP_CAP {
                    break;
                }
            }
        }
        if hits.len() >= GREP_CAP {
            break;
        }
    }
    if hits.is_empty() {
        hits.push(format!("no matches for /{pattern}/ under {path}"));
    } else {
        hits.push(format!("[{} matches shown]", hits.len()));
    }
    Ok(hits.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn shell_runs_and_reports_exit() {
        let out = shell("echo hello | tr a-z A-Z", Path::new("")).await.unwrap();
        assert_eq!(out.trim(), "HELLO");
        let bad = shell("exit 3", Path::new("")).await.unwrap();
        assert!(bad.contains("[exit code: 3]"));
    }

    #[tokio::test]
    async fn shell_runs_in_the_given_folder() {
        let dir = std::env::temp_dir().join("builtin-cwd-shell-test");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let out = shell("pwd", &dir).await.unwrap();
        assert_eq!(out.trim(), dir.canonicalize().unwrap().to_str().unwrap());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn file_tools_resolve_relative_paths_against_the_folder() {
        let dir = std::env::temp_dir().join("builtin-cwd-files-test");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();

        BuiltinTool::WriteFile
            .run(r#"{"path":"site/index.html","content":"<h1>hi</h1>"}"#, &dir)
            .await
            .unwrap();
        assert!(dir.join("site/index.html").is_file());

        let read = BuiltinTool::ReadFile
            .run(r#"{"path":"site/index.html"}"#, &dir)
            .await
            .unwrap();
        assert!(read.contains("<h1>hi</h1>"), "{read}");

        // Absolute paths still win over the folder.
        let abs = dir.join("site/index.html");
        let read_abs = BuiltinTool::ReadFile
            .run(&format!(r#"{{"path":"{}"}}"#, abs.display()), Path::new("/nonexistent"))
            .await
            .unwrap();
        assert!(read_abs.contains("<h1>hi</h1>"), "{read_abs}");

        // An empty path must still be rejected, not silently become the folder.
        let err = BuiltinTool::WriteFile.run(r#"{"content":"x"}"#, &dir).await;
        assert!(err.is_err());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn shell_sees_no_parent_secrets() {
        let _lock = crate::env_lock::guard();
        std::env::set_var("AURA_FAKE_SECRET", "sk-nope");
        let out = shell("env", Path::new("")).await.unwrap();
        assert!(!out.contains("AURA_FAKE_SECRET"), "leaked:\n{out}");
        std::env::remove_var("AURA_FAKE_SECRET");
    }

    #[tokio::test]
    async fn read_write_edit_roundtrip() {
        let dir = std::env::temp_dir().join("builtin-tools-test");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let file = dir.join("a.txt");

        write_file(Path::new(""), file.to_str().unwrap(), "one\ntwo\nthree\n").await.unwrap();
        let out = read_file(Path::new(""), file.to_str().unwrap(), 0, 2).await.unwrap();
        assert!(out.contains("one\ntwo"), "{out}");

        edit_file(Path::new(""), file.to_str().unwrap(), "two", "TWO", false).await.unwrap();
        let after = read_file(Path::new(""), file.to_str().unwrap(), 0, 10).await.unwrap();
        assert!(after.contains("TWO"), "{after}");

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn edit_reports_missing_old_string() {
        let dir = std::env::temp_dir().join("builtin-edit-test");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let file = dir.join("a.txt");
        write_file(Path::new(""), file.to_str().unwrap(), "alpha").await.unwrap();
        let err = edit_file(Path::new(""), file.to_str().unwrap(), "zzz", "y", false).await;
        assert!(err.is_err());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn list_and_glob_and_grep_find_things() {
        let dir = std::env::temp_dir().join("builtin-list-test");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(dir.join("src")).await.unwrap();
        write_file(Path::new(""), dir.join("src/main.rs").to_str().unwrap(), "fn main() {}\n").await.unwrap();
        write_file(Path::new(""), dir.join("README.md").to_str().unwrap(), "# hi\n").await.unwrap();

        let listing = list_files(Path::new(""), dir.to_str().unwrap(), true).await.unwrap();
        assert!(listing.contains("main.rs") && listing.contains("README.md"), "{listing}");

        let g = glob_files(Path::new(""), "src/*.rs", dir.to_str().unwrap()).await.unwrap();
        assert!(g.contains("main.rs"), "{g}");

        let hits = grep(Path::new(""), "fn main", dir.to_str().unwrap(), None).await.unwrap();
        assert!(hits.contains("main.rs"), "{hits}");
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn search_tools_work_from_the_project_folder() {
        let dir = std::env::temp_dir().join(format!("builtin-cwd-search-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(dir.join("src")).await.unwrap();
        write_file(&dir, "src/main.rs", "fn main() {}\n").await.unwrap();

        // "." is the folder itself: no `/./` in what the model sees.
        let by_dot = grep(&dir, "fn main", ".", None).await.unwrap();
        assert!(by_dot.contains("main.rs") && !by_dot.contains("/./"), "{by_dot}");
        let globbed = glob_files(&dir, "src/*.rs", ".").await.unwrap();
        assert!(globbed.contains("main.rs") && !globbed.contains("/./"), "{globbed}");

        // The glob filter sees paths relative to the search root, as `glob` does.
        let in_src = grep(&dir, "fn main", "src", Some("*.rs")).await.unwrap();
        assert!(in_src.contains("main.rs"), "{in_src}");
        let from_top = grep(&dir, "fn main", ".", Some("src/*.rs")).await.unwrap();
        assert!(from_top.contains("main.rs"), "{from_top}");
        let other_ext = grep(&dir, "fn main", ".", Some("**/*.md")).await.unwrap();
        assert!(other_ext.contains("no matches"), "{other_ext}");
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[test]
    fn glob_semantics() {
        let re = glob_to_regex("src/**/*.rs").unwrap();
        assert!(re.is_match("src/a.rs"));
        assert!(re.is_match("src/deep/a.rs"));
        assert!(!re.is_match("src/a.md"));
        let re = glob_to_regex("*.toml").unwrap();
        assert!(re.is_match("Cargo.toml"));
        assert!(!re.is_match("src/Cargo.toml"));
    }

    #[test]
    fn every_builtin_has_name_schema_and_policy() {
        for tool in BuiltinTool::all() {
            let def = tool.def();
            assert!(!def.name.is_empty());
            assert_eq!(def.parameters["type"], "object");
            assert!(def.parameters["properties"].is_object());
            // Mutating tools must be gated; readers must not.
            match tool {
                BuiltinTool::Shell | BuiltinTool::WriteFile | BuiltinTool::EditFile => {
                    assert!(tool.needs_approval())
                }
                _ => assert!(!tool.needs_approval()),
            }
        }
    }
}
