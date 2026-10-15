//! File browser panel for the right-hand side of the harness UI.
//!
//! Lists a directory tree (max 3 levels deep) with a filter box; clicking a
//! file returns its path to the caller via the return value (`Some(PathBuf)`).
//! Directories are shown as collapsible headers.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct FileTreeState {
    pub root: PathBuf,
    pub filter: String,
    pub expanded: std::collections::BTreeSet<PathBuf>,
    pub max_depth: usize,
}

impl FileTreeState {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            filter: String::new(),
            expanded: std::collections::BTreeSet::new(),
            max_depth: 3,
        }
    }

    /// Returns a clicked file path, if any.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<PathBuf> {
        let mut clicked: Option<PathBuf> = None;
        ui.text_edit_singleline(&mut self.filter);
        let filter = self.filter.to_lowercase();
        self.show_dir(ui, self.root.clone(), 0, &filter, &mut clicked);
        clicked
    }

    fn show_dir(&mut self, ui: &mut egui::Ui, dir: PathBuf, depth: usize, filter: &str, clicked: &mut Option<PathBuf>) {
        if depth > self.max_depth {
            return;
        }
        let mut entries: Vec<(PathBuf, bool)> = Vec::new();
        if let Ok(it) = fs::read_dir(&dir) {
            for e in it.flatten() {
                let p = e.path();
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if should_skip(&name) { continue; }
                let is_dir = p.is_dir();
                entries.push((p, is_dir));
            }
        }
        // directories first, then files, alphabetically
        entries.sort_by(|a, b| match (a.1, b.1) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.0.cmp(&b.0),
        });

        for (p, is_dir) in entries {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if !filter.is_empty() {
                let haystack = p.to_string_lossy().to_lowercase();
                if !haystack.contains(filter) {
                    // still descend into dirs
                    if is_dir { self.show_dir(ui, p, depth + 1, filter, clicked); }
                    continue;
                }
            }
            let indent = depth as f32 * 14.0;
            ui.add_space(2.0);
            ui.indent(egui::Id::new(&p), |ui| {
                if is_dir {
                    let expanded = self.expanded.contains(&p);
                    let icon = if expanded { "📂" } else { "📁" };
                    if ui.button(format!("{} {}", icon, name)).clicked() {
                        if expanded { self.expanded.remove(&p); } else { self.expanded.insert(p.clone()); }
                    }
                    if expanded {
                        self.show_dir(ui, p, depth + 1, filter, clicked);
                    }
                } else {
                    let label = format!("📄 {}", name);
                    // highlight filter
                    if ui.button(&label).clicked() {
                        *clicked = Some(p.clone());
                    }
                    let _ = indent;
                }
            });
        }
    }
}

fn should_skip(name: &str) -> bool {
    name.starts_with('.')
        || name == "target"
        || name == "node_modules"
        || name == "dist"
        || name == "build"
        || name == ".git"
}

/// Convenience helper: return the list of (path, is_dir) one level deep (used by tests).
pub fn list_dir(dir: &Path) -> Vec<(PathBuf, bool)> {
    let mut out = Vec::new();
    if let Ok(it) = fs::read_dir(dir) {
        for e in it.flatten() {
            let p = e.path();
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if should_skip(&name) { continue; }
            out.push((p.clone(), p.is_dir()));
        }
    }
    out.sort_by(|a, b| match (a.1, b.1) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.0.cmp(&b.0),
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn list_dir_skips_noise() {
        let tmp = std::env::temp_dir().join("filetree-test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::create_dir(&tmp.join("src")).unwrap();
        std::fs::write(&tmp.join("src/main.rs"), "").unwrap();
        std::fs::create_dir(&tmp.join(".git")).unwrap();
        std::fs::create_dir(&tmp.join("target")).unwrap();
        let listed = list_dir(&tmp);
        assert!(listed.iter().any(|(p, _)| p.ends_with("src")));
        assert!(!listed.iter().any(|(p, _)| p.ends_with(".git")));
        assert!(!listed.iter().any(|(p, _)| p.ends_with("target")));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
