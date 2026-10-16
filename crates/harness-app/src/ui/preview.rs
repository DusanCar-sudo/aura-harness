//! The Preview tab: shows what the harness just built.
//!
//! The engine (`crate::preview`) does the finding, serving and capturing off
//! the UI thread; this module owns the state, turns the returned PNG bytes
//! into a texture, and draws the pane.

use crate::preview::{self, Job, Kind, StaticServer, Target};
use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;
use std::time::{Duration, Instant, SystemTime};

/// A finished capture: generation tag, then the PNG or the reason it failed.
type Shot = (u64, Result<Vec<u8>, String>);

/// Everything the Preview tab needs between frames.
pub struct PreviewState {
    /// What detection found under the current root.
    pub target: Target,
    /// The root the detection ran against, so we know when to re-run it.
    pub scanned: PathBuf,
    /// Loopback server for a web build.
    pub server: Option<StaticServer>,
    /// The last successful capture.
    pub shot: Option<egui::TextureHandle>,
    /// Native pixel size of `shot`, for aspect-correct layout.
    pub shot_size: [f32; 2],
    pub shot_at: Option<Instant>,
    /// A capture is in flight.
    pub busy: bool,
    /// Re-render automatically when the build changes.
    pub auto: bool,
    /// Captures still arriving from an older target are dropped.
    pub generation: u64,
    pub error: Option<String>,
    /// Attached android device, refreshed on demand.
    pub device: Option<String>,
    /// Manual override: look for one kind of artifact instead of guessing.
    pub kind: Kind,
    rx: Option<std_mpsc::Receiver<Shot>>,
    last_check: Instant,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self::new()
    }
}

impl PreviewState {
    pub fn new() -> Self {
        Self {
            target: Target::None,
            scanned: PathBuf::new(),
            server: None,
            shot: None,
            shot_size: [0.0, 0.0],
            shot_at: None,
            busy: false,
            auto: true,
            generation: 0,
            error: None,
            device: preview::adb_device(),
            kind: Kind::Auto,
            rx: None,
            last_check: Instant::now(),
        }
    }

    /// Re-detect what `root` produced. Restarts the static server when the
    /// web root moves and drops any capture that belonged to the old target.
    pub fn rescan(&mut self, root: &Path) {
        if self.scanned == root {
            return;
        }
        let target = preview::detect_as(root, self.kind);
        if matches!((&self.target, &target), (Target::Web { root: a }, Target::Web { root: b }) if a == b) {
            self.scanned = root.to_path_buf();
            return;
        }
        self.scanned = root.to_path_buf();
        self.target = target;
        self.bump();
        self.server = None;
        self.shot = None;
        self.shot_size = [0.0, 0.0];
        self.error = None;
        if let Target::Web { root: web_root } = &self.target {
            match StaticServer::start(web_root.clone()) {
                Ok(s) => self.server = Some(s),
                Err(e) => self.error = Some(format!("cannot serve the build: {e}")),
            }
        }
        if matches!(self.target, Target::Android { .. }) {
            self.device = preview::adb_device();
        }
    }

    /// Forget the current target and look again — the "Rescan" button and the
    /// kind override both come through here.
    pub fn force_rescan(&mut self, root: &Path) {
        self.scanned = PathBuf::new();
        self.target = Target::None;
        self.rescan(root);
    }

    /// Switch what kind of artifact to look for.
    pub fn set_kind(&mut self, kind: Kind, root: &Path) {
        if self.kind == kind {
            return;
        }
        self.kind = kind;
        self.force_rescan(root);
    }

    fn bump(&mut self) {
        self.generation += 1;
        self.busy = false;
        self.rx = None;
    }

    /// Kick off a capture for the current target. Ignored while one runs.
    pub fn refresh(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        let job = match &self.target {
            Target::Web { .. } => {
                let port = match &self.server {
                    Some(s) => s.port,
                    None => return,
                };
                let size = self.capture_size(ctx);
                Job::Web {
                    url: format!("http://127.0.0.1:{port}/"),
                    w: size.0,
                    h: size.1,
                }
            }
            Target::Android { apk } => {
                if apk.is_none() && self.device.is_none() {
                    self.error = Some("no APK built and no device attached".into());
                    return;
                }
                match self.device {
                    Some(_) => Job::Adb,
                    None => {
                        self.error = Some("build the APK, then plug in a device to see it".into());
                        return;
                    }
                }
            }
            Target::Game { run, .. } => Job::Window {
                hint: run
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
            },
            Target::None => return,
        };
        self.spawn(ctx, job);
    }

    /// Run a specific job (used by "Run game" then "Capture").
    pub fn spawn(&mut self, ctx: &egui::Context, job: Job) {
        if self.busy {
            return;
        }
        self.generation += 1;
        self.busy = true;
        self.error = None;
        let gen = self.generation;
        let ctx = ctx.clone();
        let (tx, rx) = std_mpsc::channel();
        self.rx = Some(rx);
        std::thread::spawn(move || {
            let result = preview::run(&job);
            // Wake egui the moment the picture is ready.
            ctx.request_repaint();
            let _ = tx.send((gen, result));
        });
    }

    /// Pull in any finished capture. Call once per frame, whatever tab is up.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.rx else { return };
        let Ok((gen, result)) = rx.try_recv() else { return };
        if gen != self.generation {
            // Stale: the target changed while this capture was running.
            return;
        }
        self.busy = false;
        self.rx = None;
        match result {
            Ok(bytes) => match image::load_from_memory(&bytes) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
                    if w == 0 || h == 0 {
                        self.error = Some("empty capture".into());
                        return;
                    }
                    let color = egui::ColorImage::from_rgba_unmultiplied([w, h], rgba.as_raw());
                    self.shot = Some(ctx.load_texture(
                        "aura-preview",
                        color,
                        egui::TextureOptions::LINEAR,
                    ));
                    self.shot_size = [w as f32, h as f32];
                    self.shot_at = Some(Instant::now());
                    self.error = None;
                }
                Err(e) => self.error = Some(format!("cannot decode the capture: {e}")),
            },
            Err(e) => self.error = Some(e),
        }
    }

    /// Capture size for the next web render: keep it close to the panel so
    /// the picture is 1:1 instead of an upscaled thumbnail.
    fn capture_size(&self, ctx: &egui::Context) -> (u16, u16) {
        let s = ctx.screen_rect();
        let w = (s.width() * 1.4).clamp(640.0, 1600.0) as u16;
        let h = (s.height() * 1.0).clamp(480.0, 1200.0) as u16;
        (w, h)
    }

    /// Auto mode: re-render when the served tree changes, at most twice a
    /// second, and only while the tab is actually on screen.
    pub fn tick(&mut self, ctx: &egui::Context) {
        if !self.auto || self.busy {
            return;
        }
        if self.last_check.elapsed() < Duration::from_millis(600) {
            return;
        }
        self.last_check = Instant::now();
        let since = self
            .shot_at
            .map(|t| SystemTime::now() - t.elapsed())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        match &self.target {
            Target::Web { root } => {
                if preview::changed_since(root, since) {
                    self.refresh(ctx);
                }
            }
            // Nothing recognised yet: keep looking while the tree changes, so
            // the first successful build appears without a manual rescan.
            Target::None => {
                let root = self.scanned.clone();
                if !root.as_os_str().is_empty() && preview::changed_since(&root, since) {
                    self.force_rescan(&root);
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

/// Draw the Preview tab. `root` is the file-tree root (the project).
pub fn show(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut PreviewState,
    root: &Path,
) {
    state.rescan(root);
    state.tick(ctx);
    state.poll(ctx);

    // ---- toolbar -----------------------------------------------------------
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(!state.busy, egui::Button::new("⟳ Refresh"))
            .on_hover_text("Re-capture the build now")
            .clicked()
        {
            state.refresh(ctx);
        }
        ui.checkbox(&mut state.auto, "auto")
            .on_hover_text("Re-render whenever the build changes");
        if ui.small_button("↻ Rescan").clicked() {
            state.force_rescan(root);
        }
        let can_open = matches!(&state.target, Target::Web { .. });
        if ui
            .add_enabled(can_open, egui::Button::new("↗ Open"))
            .on_hover_text("Open this build in your default browser")
            .clicked()
        {
            if let Some(s) = &state.server {
                let url = s.url();
                let _ = std::process::Command::new("xdg-open")
                    .arg(&url)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
        }
        if state.busy {
            ui.label(egui::RichText::new("rendering…").weak());
        }
    });

    // ---- what kind of artifact to look for --------------------------------
    ui.horizontal(|ui| {
        ui.small("look for");
        for kind in [Kind::Auto, Kind::Web, Kind::Android, Kind::Game] {
            if ui
                .selectable_label(state.kind == kind, kind.label())
                .on_hover_text("Which kind of artifact to look for (Auto guesses)")
                .clicked()
            {
                state.set_kind(kind, root);
            }
        }
    });

    // ---- target line -------------------------------------------------------
    let (label, detail) = match &state.target {
        Target::None => (state.target.label().to_string(), String::new()),
        Target::Web { root: w } => (
            state.target.label().to_string(),
            w.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| w.display().to_string()),
        ),
        Target::Android { apk } => (
            state.target.label().to_string(),
            match apk {
                Some(a) => a.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                None => "gradle project (no APK built yet)".into(),
            },
        ),
        Target::Game { label: kind, .. } => (state.target.label().to_string(), kind.clone()),
    };
    let target_line = if detail.is_empty() {
        label
    } else {
        format!("{label} · {detail}")
    };
    ui.label(egui::RichText::new(target_line).strong().color(theme_accent()));
    if let Target::Web { .. } = &state.target {
        if let Some(s) = &state.server {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(s.url()).small().color(theme_dim()));
                if ui.small_button("📋").on_hover_text("Copy the preview URL").clicked() {
                    ui.ctx().copy_text(s.url());
                }
            });
        }
    }
    ui.separator();

    // ---- status ------------------------------------------------------------
    if let Some(err) = &state.error {
        ui.colored_label(theme_danger(), err);
        ui.add_space(4.0);
    }

    // ---- the picture, or an explanation of why there isn't one -------------
    if state.shot.is_some() {
        paint_shot(ui, state);
    } else {
        empty_state(ui, ctx, state, root);
    }
}

/// Aspect-correct placement of the captured frame inside the panel.
fn paint_shot(ui: &mut egui::Ui, state: &PreviewState) {
    let Some(tex) = &state.shot else { return };
    let (nw, nh) = (state.shot_size[0].max(1.0), state.shot_size[1].max(1.0));
    let avail = ui.available_size_before_wrap();
    let scale = (avail.x / nw).min(avail.y.max(200.0) / nh);
    let scale = if scale.is_finite() && scale > 0.0 {
        scale.min(2.0)
    } else {
        1.0
    };
    let size = egui::vec2(nw * scale, nh * scale);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(rect, 6.0, egui::Color32::BLACK);
    ui.put(
        rect,
        egui::Image::new((tex.id(), size)).rounding(6.0),
    );
    if let Some(at) = state.shot_at {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
            ui.label(
                egui::RichText::new(format!(
                    "captured {}s ago · {}×{}",
                    at.elapsed().as_secs(),
                    state.shot_size[0] as u32,
                    state.shot_size[1] as u32
                ))
                .small()
                .weak(),
            );
        });
    }
}

/// No picture yet: explain what will appear and why it is missing.
fn empty_state(ui: &mut egui::Ui, ctx: &egui::Context, state: &mut PreviewState, root: &Path) {
    // Clone the target out of `state`: the branches below need `state` mutably.
    match state.target.clone() {
        Target::None => {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Nothing to preview yet.")
                    .strong()
                    .color(theme_accent()),
            );
            ui.label(
                "The harness looks under the file-tree root for a build it recognises:",
            );
            ui.add_space(2.0);
            bullet(ui, "web — an `index.html` at the root or in dist/, build/web/, out/, public/");
            bullet(ui, "android — an .apk, or a Gradle project (settings.gradle + app/)");
            bullet(ui, "game — project.godot, a runnable binary, or bevy/phaser in the manifest");
            ui.add_space(6.0);
            if ui.button("↻ Look again").clicked() {
                state.force_rescan(root);
            }
        }
        Target::Web { root: w } => {
            ui.add_space(6.0);
            ui.label(format!("Serving {} on the loopback port.", w.display()));
            if let Some(err) = &state.error {
                ui.label(egui::RichText::new(err.as_str()).color(theme_danger()));
            }
            ui.add_space(6.0);
            if ui.button("▶ Render the page").clicked() {
                state.refresh(ctx);
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("With auto on, this repaints itself every time a build file changes.")
                    .small()
                    .weak(),
            );
        }
        Target::Android { apk } => {
            ui.add_space(6.0);
            match apk {
                Some(a) => {
                    let meta = std::fs::metadata(&a).ok();
                    ui.label(format!("APK: {}", a.display()));
                    if let Some(m) = meta {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} · modified {}",
                                human_bytes(m.len()),
                                m.modified()
                                    .ok()
                                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                                    .map(|d| chrono_format(d.as_secs()))
                                    .unwrap_or_else(|| "unknown".into())
                            ))
                            .small()
                            .weak(),
                        );
                    }
                }
                None => {
                    ui.label("No APK yet — build the project (gradlew assembleDebug).");
                }
            }
            ui.add_space(4.0);
            match &state.device {
                Some(d) => {
                    ui.label(format!(
                        "Device: {d} · capturing its screen.",
                        d = d
                    ));
                    if ui.button("📸 Capture screen").clicked() {
                        state.spawn(ctx, Job::Adb);
                    }
                }
                None => {
                    ui.label(
                        egui::RichText::new("No device or emulator attached (`adb devices` is empty).")
                            .weak(),
                    );
                    if ui.button("↻ Check for a device").clicked() {
                        state.device = preview::adb_device();
                        if state.device.is_some() {
                            state.refresh(ctx);
                        }
                    }
                    ui.small("With a device attached the preview is the device's own screen.");
                }
            }
        }
        Target::Game { run, label } => {
            ui.add_space(6.0);
            ui.label(format!("{label}: {}", run.display()));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("▶ Run").clicked() {
                    let _ = run_game(&run);
                    // The window appears a moment after spawn: drop any capture
                    // that was started against the old window list.
                    state.generation += 1;
                    state.busy = false;
                    state.rx = None;
                }
                if ui.button("📸 Capture window").clicked() {
                    state.spawn(ctx, Job::Window {
                        hint: run.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                    });
                }
            });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Run starts the binary; Capture grabs its window. A game that renders to a \
                     web page (index.html) previews under the web build path instead.",
                )
                .small()
                .weak(),
            );
            if let Some(err) = &state.error {
                ui.label(egui::RichText::new(err.as_str()).color(theme_danger()));
            }
        }
    }
}

fn run_game(run: &Path) -> Option<u32> {
    if run.is_dir() {
        // Godot project: launch the editor if godot is on PATH.
        let godot = preview::which(&["godot", "godot4", "godot4.2"])?;
        let child = std::process::Command::new(godot)
            .arg(run)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()?;
        return Some(child.id());
    }
    let mut cmd = std::process::Command::new(run);
    cmd.current_dir(run.parent().unwrap_or(Path::new(".")));
    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let child = cmd.spawn().ok()?;
    Some(child.id())
}

fn bullet(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(egui::RichText::new("•").color(theme_accent()));
        ui.label(egui::RichText::new(text).small());
    });
}

fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

fn chrono_format(secs: u64) -> String {
    let dt = chrono::DateTime::from_timestamp(secs as i64, 0)
        .map(|d| d.with_timezone(&chrono::Local))
        .map(|d| d.format("%H:%M").to_string());
    dt.unwrap_or_else(|| "unknown".into())
}

// Palette accessors: this module reads the app theme like every other panel.
fn theme_accent() -> egui::Color32 {
    crate::theme::accent()
}
fn theme_dim() -> egui::Color32 {
    crate::theme::dim()
}
fn theme_danger() -> egui::Color32 {
    crate::theme::danger()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_reads_naturally() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(1_572_864), "1.5 MB");
        assert_eq!(human_bytes(5 * 1024 * 1024 * 1024), "5.0 GB");
    }

    #[test]
    fn fresh_state_has_nothing_to_show() {
        let s = PreviewState::new();
        assert!(s.shot.is_none());
        assert!(!s.busy);
        assert!(s.auto, "auto-render is the default");
        assert_eq!(s.generation, 0);
        // A missing directory must not panic the constructor.
        assert_eq!(s.target, Target::None);
    }

    #[test]
    fn rescan_is_stable_for_the_same_root() {
        let dir = std::env::temp_dir().join(format!("aura-pvs-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut s = PreviewState::new();
        s.rescan(&dir);
        let first = s.generation;
        s.rescan(&dir);
        s.rescan(&dir);
        assert_eq!(s.generation, first, "rescanning the same root resets state");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
