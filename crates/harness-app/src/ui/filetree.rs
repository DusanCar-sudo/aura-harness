//! File browser panel for the right-hand side of the harness UI.
//!
//! Fully interactive: expand/collapse, hop into folders, filter, click to
//! attach a file, copy a path, open a file/folder externally, rename, create
//! files and folders, and delete. The tree grows downward (one row per line)
//! so deep folders never push sideways.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct FileTreeState {
    pub root: PathBuf,
    pub current: PathBuf,
    pub filter: String,
    pub expanded: std::collections::BTreeSet<PathBuf>,
    pub max_depth: usize,

    /// Inline editors: (path, new name).
    pub editing: Option<(PathBuf, String)>,
    /// Inline creator: (parent dir, new name, is_dir).
    pub creating: Option<(PathBuf, String, bool)>,
    /// Inline delete confirmation.
    pub deleting: Option<PathBuf>,
    /// Last filesystem error, shown under the tree.
    pub error: Option<String>,
    /// Set when the user points the root at the current folder; the app
    /// persists it and the Preview tab re-scans against it.
    pub root_changed: bool,
    /// List dotfiles and heavy folders too (target, node_modules). Build
    /// outputs (dist, build) are always listed either way.
    pub show_hidden: bool,
}

impl FileTreeState {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let r: PathBuf = root.into();
        Self {
            root: r.clone(),
            current: r,
            filter: String::new(),
            expanded: std::collections::BTreeSet::new(),
            max_depth: 6,
            editing: None,
            creating: None,
            deleting: None,
            error: None,
            root_changed: false,
            show_hidden: false,
        }
    }

    /// Restore the persisted hidden-files preference.
    pub fn with_hidden(mut self, show_hidden: bool) -> Self {
        self.show_hidden = show_hidden;
        self
    }

    fn base(&self) -> PathBuf {
        if self.current.as_os_str().is_empty() {
            self.root.clone()
        } else {
            self.current.clone()
        }
    }

    fn fail(&mut self, what: &str, path: &Path, e: std::io::Error) {
        self.error = Some(format!("{what} {}: {e}", path.display()));
    }

    fn copy_path(&mut self, ui: &egui::Ui, path: &Path) {
        ui.ctx().copy_text(path.display().to_string());
        self.error = None;
    }

    fn open_externally(path: &Path) -> Result<(), std::io::Error> {
        std::process::Command::new("xdg-open")
            .arg(path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
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
        let _ = self.base();

        ui.horizontal(|ui| {
            ui.small("Location:");
            ui.small(self.current.display().to_string());
            if ui.small_button("root").on_hover_text("Go to workspace root").clicked() && self.current != self.root
            {
                self.current = self.root.clone();
                self.expanded.clear();
            }
            if self.current != self.root {
                if let Some(parent) = self.current.parent() {
                    if ui.small_button("up").on_hover_text("One level up").clicked() {
                        self.current = parent.to_path_buf();
                        self.expanded.clear();
                    }
                }
            }
            if ui.small_button("new file").on_hover_text("Create a file here").clicked() {
                self.creating = Some((self.current.clone(), String::new(), false));
            }
            if ui.small_button("new dir").on_hover_text("Create a folder here").clicked() {
                self.creating = Some((self.current.clone(), String::new(), true));
            }
            if self.current != self.root
                && ui
                    .small_button("set root")
                    .on_hover_text("Make this folder the project root — the Preview tab looks here for a build")
                    .clicked()
            {
                self.root = self.current.clone();
                self.root_changed = true;
                self.expanded.clear();
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            crate::ui::text_edit(
                ui,
                &mut self.filter,
                crate::ui::TextEditSpec::singleline().hint("filter (name or path)"),
            );
            if ui
                .small_button(if self.show_hidden {
                    "hidden: on"
                } else {
                    "hidden: off"
                })
                .on_hover_text("Also list dotfiles and heavy folders (target, node_modules)")
                .clicked()
            {
                self.show_hidden = !self.show_hidden;
                self.expanded.clear();
            }
        });

        self.show_dir(ui, self.current.clone(), 0, &self.filter.to_lowercase(), &mut clicked);
        self.show_editors(ui);

        if let Some(err) = &self.error {
            ui.colored_label(crate::theme::danger(), err);
        }
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
        let entries = list_dir(&dir, self.show_hidden);

        for (p, is_dir) in entries {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if !filter.is_empty() {
                let haystack = p.to_string_lossy().to_lowercase();
                if !haystack.contains(filter) {
                    if is_dir { self.show_dir(ui, p, depth + 1, filter, clicked); }
                    continue;
                }
            }

            // One row per line; children render below, never beside.
            ui.horizontal(|ui| {
                for _ in 0..depth {
                    ui.add_space(12.0);
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
                } else {
                    if ui
                        .button(format!("📄 {}", name))
                        .on_hover_text(p.display().to_string())
                        .clicked()
                    {
                        *clicked = Some(p.clone());
                    }
                }
                if ui
                    .small_button("📋")
                    .on_hover_text("Copy full path to clipboard")
                    .clicked()
                {
                    self.copy_path(ui, &p);
                }
                ui.menu_button("•••", |ui| {
                    ui.small("Actions for this entry");
                    if ui.button("Copy path").clicked() {
                        self.copy_path(ui, &p);
                    }
                    if ui.button("Open").clicked() {
                        if let Err(e) = Self::open_externally(&p) {
                            self.fail("open", &p, e);
                        }
                    }
                    if ui.button("Open containing folder").clicked() {
                        let dir = p.parent().unwrap_or(&p);
                        if let Err(e) = Self::open_externally(dir) {
                            self.fail("open", dir, e);
                        }
                    }
                    ui.separator();
                    if ui.button("Rename…").clicked() {
                        self.editing = Some((p.clone(), name.clone()));
                    }
                    if is_dir {
                        if ui.button("New file here").clicked() {
                            self.creating = Some((p.clone(), String::new(), false));
                        }
                        if ui.button("New folder here").clicked() {
                            self.creating = Some((p.clone(), String::new(), true));
                        }
                    }
                    ui.separator();
                    if ui.button("Delete…").clicked() {
                        self.deleting = Some(p.clone());
                    }
                });
            });

            if is_dir && self.expanded.contains(&p) {
                self.show_dir(ui, p, depth + 1, filter, clicked);
            }
        }
    }

    /// Inline rename / create / delete rows, drawn under the tree.
    fn show_editors(&mut self, ui: &mut egui::Ui) {
        if let Some((path, name)) = self.editing.take() {
            ui.horizontal(|ui| {
                ui.small("Rename:");
                let mut draft = name.clone();
                let resp = crate::ui::text_edit(
                    ui,
                    &mut draft,
                    crate::ui::TextEditSpec::singleline(),
                );
                resp.request_focus();
                if ui.small_button("save").clicked()
                    || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    self.rename(&path, &draft);
                    self.editing = None;
                }
                if ui.small_button("cancel").clicked() {
                    self.editing = None;
                }
            });
        }

        if let Some((parent, name, is_dir)) = self.creating.take() {
            let mut draft = name.clone();
            ui.horizontal(|ui| {
                ui.small(if is_dir { "New folder:" } else { "New file:" });
                let resp = crate::ui::text_edit(
                    ui,
                    &mut draft,
                    crate::ui::TextEditSpec::singleline(),
                );
                resp.request_focus();
                if ui.small_button("create").clicked()
                    || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    self.create(&parent, &draft, is_dir);
                    self.creating = None;
                }
                if ui.small_button("cancel").clicked() {
                    self.creating = None;
                }
            });
        }

        if let Some(path) = self.deleting.clone() {
            ui.horizontal(|ui| {
                ui.colored_label(
                    crate::theme::danger(),
                    format!("Delete {}?", path.display()),
                );
                if ui.small_button("delete").clicked() {
                    self.delete(&path);
                    self.deleting = None;
                }
                if ui.small_button("cancel").clicked() {
                    self.deleting = None;
                }
            });
        }
    }

    fn rename(&mut self, path: &Path, new_name: &str) {
        let new_name = new_name.trim();
        if new_name.is_empty() || new_name.contains('/') {
            self.error = Some("rename needs a plain name (no slashes)".into());
            return;
        }
        let target = path.with_file_name(new_name);
        match fs::rename(path, &target) {
            Ok(()) => {
                self.expanded.remove(path);
                self.error = None;
            }
            Err(e) => self.fail("rename", path, e),
        }
    }

    fn create(&mut self, parent: &Path, name: &str, is_dir: bool) {
        let name = name.trim();
        if name.is_empty() || name.contains('/') {
            self.error = Some("new name needs to be a plain name (no slashes)".into());
            return;
        }
        let target = parent.join(name);
        let res = if is_dir {
            fs::create_dir(&target)
        } else {
            fs::File::create(&target).map(|_| ())
        };
        match res {
            Ok(()) => self.error = None,
            Err(e) => self.fail("create", &target, e),
        }
    }

    fn delete(&mut self, path: &Path) {
        let res = if path.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        };
        match res {
            Ok(()) => {
                self.expanded.remove(path);
                self.error = None;
            }
            Err(e) => self.fail("delete", path, e),
        }
    }
}

fn should_skip(name: &str, show_hidden: bool) -> bool {
    if show_hidden {
        return false;
    }
    // `dist` and `build` are build outputs: the artifacts this workstation
    // previews. Hiding them would hide exactly what the user came for.
    name.starts_with('.') || name == "target" || name == "node_modules"
}

/// One level of `dir` as (path, is_dir): noise skipped, folders first.
pub fn list_dir(dir: &Path, show_hidden: bool) -> Vec<(PathBuf, bool)> {
    let mut out = Vec::new();
    if let Ok(it) = fs::read_dir(dir) {
        for e in it.flatten() {
            let p = e.path();
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if should_skip(&name, show_hidden) { continue; }
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
        // Build outputs are artifacts, so they must never be skipped.
        std::fs::create_dir(&tmp.join("dist")).unwrap();
        std::fs::create_dir(&tmp.join("build")).unwrap();
        let listed = list_dir(&tmp, false);
        assert!(listed.iter().any(|(p, _)| p.ends_with("src")));
        assert!(listed.iter().any(|(p, _)| p.ends_with("dist")));
        assert!(listed.iter().any(|(p, _)| p.ends_with("build")));
        assert!(!listed.iter().any(|(p, _)| p.ends_with(".git")));
        assert!(!listed.iter().any(|(p, _)| p.ends_with("target")));
        // Hidden mode lists everything, so no entry is ever unreachable.
        let all = list_dir(&tmp, true);
        assert!(all.iter().any(|(p, _)| p.ends_with(".git")));
        assert!(all.iter().any(|(p, _)| p.ends_with("target")));
        assert!(all.len() > listed.len());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn create_rename_and_delete_roundtrip() {
        let tmp = std::env::temp_dir().join("filetree-ops-test");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let mut st = FileTreeState::new(&tmp);

        st.create(&tmp, "notes.md", false);
        let file = tmp.join("notes.md");
        assert!(file.exists(), "create failed: {:?}", st.error);

        st.rename(&file, "readme.md");
        let renamed = tmp.join("readme.md");
        assert!(renamed.exists() && !file.exists(), "rename failed");

        st.delete(&renamed);
        assert!(!renamed.exists(), "delete failed");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rejects_path_traversal_names() {
        let tmp = std::env::temp_dir().join("filetree-guard-test");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let mut st = FileTreeState::new(&tmp);
        st.create(&tmp, "../evil", false);
        assert!(st.error.is_some(), "slash names must be refused");
        st.rename(&tmp, "..");
        assert!(st.error.is_some(), "dot-dot names must be refused");
        let _ = fs::remove_dir_all(&tmp);
    }
}
