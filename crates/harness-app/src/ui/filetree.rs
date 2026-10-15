//! File browser panel for the right-hand side of the harness UI.
//!
//! Browsable directory tree: expand/collapse folders, "hop in" to SSH into a
//! subfolder as the base view root, filter, and click a file to emit its path.
//! Directories are interactive (expand, navigate, hover hints).

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct FileTreeState {
    pub root: PathBuf,
    pub current: PathBuf,
    pub filter: String,
    pub expanded: std::collections::BTreeSet<PathBuf>,
    pub max_depth: usize,
}

impl FileTreeState {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let r: PathBuf = root.into();
        Self {
            root: r.clone(),
            current: r,
            filter: String::new(),
            expanded: std::collections::BTreeSet::new(),
            max_depth: 4,
        }
    }

    /// Ensures a walkable base directory (falls back to the program's cwd).
    fn base(&self) -> PathBuf {
        if self.current.as_os_str().is_empty() {
            self.root.clone()
        } else {
            self.current.clone()
        }
    }

    /// Returns a clicked file path, if any.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<PathBuf> {
        let mut clicked: Option<PathBuf> = None;
        if self.current.as_os_str().is_empty() {
            self.current = self.root.clone();
        }
        if self.root.as_os_str().is_empty() {
            self.root = PathBuf::from(".");
            self.current = self.root.clone();
        }

        ui.horizontal(|ui| {
            ui.small("Location:");
            ui.small(self.current.display().to_string());
            if ui.small_button("root").clicked() && self.current != self.root {
                self.current = self.root.clone();
                self.expanded.clear();
            }
            if let Some(parent) = self.current.parent() {
                if self.current != self.root {
                    if ui.small_button("up").clicked() {
                        self.current = parent.to_path_buf();
                        self.expanded.clear();
                    }
                }
            }
        });
        ui.separator();
        ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .hint_text("filter (name or path)"),
        );

        self.show_dir(ui, self.current.clone(), 0, &self.filter.to_string().to_lowercase(), &mut clicked);

        clicked
    }

    fn show_dir(
        &mut self,
        ui: &mut egui::Ui,
        dir: PathBuf,
        depth: usize,
        filter: &str,
        clicked: &mut Option<PathBuf>,
    ) {
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
                    // still descend into dirs, so matches deeper show up
                    if is_dir { self.show_dir(ui, p, depth + 1, filter, clicked); }
                    continue;
                }
            }
            ui.horizontal(|ui| {
                // indentation via per-line spacer
                for _ in 0..depth {
                    ui.add_space(14.0);
                }
                if is_dir {
                    let expanded = self.expanded.contains(&p);
                    let icon = if expanded { "📂" } else { "📁" };
                    if ui.button(format!("{} {}", icon, name)).clicked() {
                        if expanded { self.expanded.remove(&p); } else { self.expanded.insert(p.clone()); }
                    }
                    if ui.small_button("in").on_hover_text("Hop into folder").clicked() {
                        self.current = p.clone();
                        self.expanded.clear();
                    }
                    if expanded {
                        self.show_dir(ui, p, depth + 1, filter, clicked);
                    }
                } else {
                    let label = format!("📄 {}", name);
                    if ui
                        .button(&label)
                        .on_hover_text(p.display().to_string())
                        .clicked()
                    {
                        *clicked = Some(p.clone());
                    }
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
