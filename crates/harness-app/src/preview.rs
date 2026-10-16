//! Artifact preview engine: work out what the project built, serve it on a
//! loopback port, and take a picture of it.
//!
//! Three kinds of artifact are recognised:
//!
//! * **web** — an `index.html` in the project or in a known output dir. It is
//!   served by [`StaticServer`] and screenshotted with the headless Chromium
//!   that ships with Playwright (`~/.cache/ms-playwright`), falling back to a
//!   system `chromium`/`google-chrome`.
//! * **android** — an `.apk`, or a Gradle project with a manifest. With a
//!   device or emulator attached the screen comes back through
//!   `adb exec-out screencap -p`.
//! * **game** — a Godot project or a runnable binary. The binary is spawned
//!   and its window is grabbed through sway + `grim`.
//!
//! Nothing in here touches egui: every job returns PNG bytes (or an error)
//! and the UI decides what to do with them.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// What a project produced. Detection order is web, then android, then game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Nothing recognisable yet.
    None,
    /// A static web build: serve this directory.
    Web { root: PathBuf },
    /// An APK (or a Gradle project without one built yet).
    Android { apk: Option<PathBuf> },
    /// Something runnable.
    Game { run: PathBuf, label: String },
}

impl Target {
    /// Short human label for the preview header.
    pub fn label(&self) -> &'static str {
        match self {
            Target::None => "nothing to preview",
            Target::Web { .. } => "web build",
            Target::Android { .. } => "android build",
            Target::Game { .. } => "game build",
        }
    }
}

/// Directories that never hold a build output worth scanning.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "venv",
    ".venv",
    "__pycache__",
    ".gradle",
    ".cargo",
    "Pods",
    ".idea",
];

/// Which kind of artifact to look for. `Auto` walks them in order of how
/// finished they look: a built APK beats a checked-in `index.html`, and both
/// beat a manifest that only says what the project *will* be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Auto,
    Web,
    Android,
    Game,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Auto => "Auto",
            Kind::Web => "Web",
            Kind::Android => "APK",
            Kind::Game => "Game",
        }
    }
}

/// Recognise what `root` contains. Bounded: it never walks more than a few
/// thousand entries, so a huge tree cannot stall the UI thread.
pub fn detect(root: &Path) -> Target {
    if !root.is_dir() {
        return Target::None;
    }
    // A built APK is an unambiguous artifact; an index.html at the root may
    // just be a source file, so web comes second.
    if let Some(t) = android_target(root) {
        return t;
    }
    if let Some(t) = web_target(root) {
        return t;
    }
    if let Some(t) = game_target(root) {
        return t;
    }
    Target::None
}

/// Same, but only consider one kind — the manual override in the preview tab.
pub fn detect_as(root: &Path, kind: Kind) -> Target {
    if !root.is_dir() {
        return Target::None;
    }
    match kind {
        Kind::Web => web_target(root).unwrap_or(Target::None),
        Kind::Android => android_target(root).unwrap_or(Target::None),
        Kind::Game => game_target(root).unwrap_or(Target::None),
        // Auto is the documented default entry point: one ordering, one place.
        Kind::Auto => detect(root),
    }
}

fn web_target(root: &Path) -> Option<Target> {
    if !root.is_dir() {
        return None;
    }
    {
    let mut web_dirs: Vec<PathBuf> = vec![
        root.to_path_buf(),
        root.join("dist"),
        root.join("build").join("web"),
        root.join("build").join("www"),
        root.join("out"),
        root.join("public"),
        root.join("target").join("www"),
        root.join("site"),
        root.join("docs"),
        root.join("_site"),
        root.join("build").join("webapp"),
    ];
    for d in &web_dirs {
        if d.join("index.html").is_file() {
            return Some(Target::Web { root: d.clone() });
        }
    }
    // A shallow sweep for an index.html in an unconventional spot.
    if let Some(found) = find_file(root, "index.html", 3) {
        return Some(Target::Web { root: found });
    }
    web_dirs.clear();
    None
    }
}

fn android_target(root: &Path) -> Option<Target> {
    if !root.is_dir() {
        return None;
    }
    if let Some(apk) = find_ext(root, "apk", 5) {
        return Some(Target::Android { apk: Some(apk) });
    }
    if is_gradle_project(root) {
        return Some(Target::Android { apk: None });
    }
    None
}

fn game_target(root: &Path) -> Option<Target> {
    if !root.is_dir() {
        return None;
    }
    if root.join("project.godot").is_file() {
        return Some(Target::Game {
            run: root.to_path_buf(),
            label: "Godot project".into(),
        });
    }
    if let Some((bin, label)) = find_game_binary(root) {
        return Some(Target::Game { run: bin, label });
    }
    if let Some(name) = game_from_manifest(root) {
        return Some(Target::Game {
            run: root.to_path_buf(),
            label: name,
        });
    }
    None
}

/// Bounded breadth-first walk looking for a file name. Returns the directory
/// that contains it (so a web root can be served from there).
fn find_file(root: &Path, name: &str, max_depth: usize) -> Option<PathBuf> {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut seen = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        seen += 1;
        if seen > 3000 {
            return None;
        }
        if dir.join(name).is_file() {
            return Some(dir);
        }
        if depth >= max_depth {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && !skipped(&e.file_name().to_string_lossy()) {
                stack.push((p, depth + 1));
            }
        }
    }
    None
}

/// Bounded walk for a file with the given extension; returns the full path.
fn find_ext(root: &Path, ext: &str, max_depth: usize) -> Option<PathBuf> {
    let ext = format!(".{ext}");
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut seen = 0usize;
    let mut best: Option<(usize, PathBuf)> = None;
    while let Some((dir, depth)) = stack.pop() {
        seen += 1;
        if seen > 3000 {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if depth < max_depth && !skipped(&e.file_name().to_string_lossy()) {
                    stack.push((p, depth + 1));
                }
            } else if p.extension().map(|x| x.eq_ignore_ascii_case(&ext[1..])) == Some(true) {
                // Prefer the shortest path: `app/build/outputs/apk/debug/...`
                // beats a stray fixture apk deeper in the tree.
                let score = p.components().count();
                if best.as_ref().map(|(s, _)| score < *s).unwrap_or(true) {
                    best = Some((score, p));
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

fn skipped(name: &str) -> bool {
    SKIP_DIRS.iter().any(|s| s.eq_ignore_ascii_case(name))
}

fn is_gradle_project(root: &Path) -> bool {
    const MARKERS: &[&str] = &[
        "settings.gradle",
        "settings.gradle.kts",
        "gradlew",
        "build.gradle",
        "build.gradle.kts",
    ];
    MARKERS.iter().any(|m| root.join(m).is_file())
        && (root.join("app").is_dir() || find_file(root, "AndroidManifest.xml", 4).is_some())
}

/// Look for something executable that could be the game.
fn find_game_binary(root: &Path) -> Option<(PathBuf, String)> {
    const DIRS: &[&str] = &[".", "target/release", "target/debug", "bin", "build", "dist", "out"];
    for d in DIRS {
        let dir = if *d == "." {
            root.to_path_buf()
        } else {
            root.join(d)
        };
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_file() {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            // Ignore source files, scripts, archives and the usual clutter.
            if name.contains('.') && !is_likely_binary_name(&name) {
                continue;
            }
            if !is_executable(&p) {
                continue;
            }
            if name.starts_with('.') || name.ends_with(".sh") {
                continue;
            }
            let label = format!("executable `{name}`");
            return Some((p, label));
        }
    }
    None
}

/// Windows/game binaries keep an extension; Unix ones usually do not.
fn is_likely_binary_name(name: &str) -> bool {
    matches!(
        name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str(),
        "exe" | "appimage" | "bin" | "x86_64" | "elf"
    )
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p)
            .map(|m| m.permissions().mode() & 0o111 != 0 && m.len() > 4096)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// A manifest that names a known game engine tells us what this project is
/// even before anything has been built.
fn game_from_manifest(root: &Path) -> Option<String> {
    let engines = [
        ("Cargo.toml", &["bevy", "ggez", "macroquad", "miniquad", "raylib", "tetra"][..]),
        ("package.json", &["phaser", "\"three\"", "pixi.js", "playcanvas", "babylonjs"][..]),
    ];
    for (file, needles) in engines {
        let Ok(text) = std::fs::read_to_string(root.join(file)) else {
            continue;
        };
        if needles.iter().any(|n| text.contains(n)) {
            let kind = if file == "Cargo.toml" { "Rust game" } else { "JS game" };
            return Some(format!("{kind} ({file})"));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Static file server
// ---------------------------------------------------------------------------

/// A loopback HTTP server for one directory. Dropping it stops the thread.
pub struct StaticServer {
    pub port: u16,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl StaticServer {
    /// Bind to 127.0.0.1 on an ephemeral port and serve `root` until dropped.
    pub fn start(root: PathBuf) -> std::io::Result<Self> {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let canonical = root.canonicalize().unwrap_or(root);

        let handle = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut sock, _)) => {
                        let root = canonical.clone();
                        std::thread::spawn(move || {
                            let _ = sock.set_read_timeout(Some(Duration::from_secs(5)));
                            let _ = serve(&mut sock, &root);
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(15));
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            port,
            stop,
            handle: Some(handle),
        })
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for StaticServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            // The accept loop wakes within ~15 ms; don't block the UI on it.
            drop(std::thread::spawn(move || {
                let _ = h.join();
            }));
        }
    }
}

/// Read one request and write one response. Error paths still answer, so a
/// broken request never leaves Chromium hanging.
fn serve<S: std::io::Read + std::io::Write>(sock: &mut S, root: &Path) -> std::io::Result<()> {
    let mut buf = [0u8; 8192];
    let n = sock.read(&mut buf)?;
    if n == 0 {
        return Ok(());
    }
    let head = String::from_utf8_lossy(&buf[..n]);
    let mut parts = head.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();

    if method != "GET" && method != "HEAD" {
        return write_response(sock, 405, "text/plain", b"method not allowed", &method);
    }

    let rel = url_path(&target);
    let rel = rel.trim_start_matches('/');
    let candidate = root.join(rel);
    // Canonicalise and confine: `../../etc/passwd` must never escape `root`.
    let resolved = match candidate.canonicalize() {
        Ok(p) if p.starts_with(root) => p,
        _ => return write_response(sock, 404, "text/plain", b"not found", &method),
    };

    let path = if resolved.is_dir() {
        resolved.join("index.html")
    } else {
        resolved
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return write_response(sock, 404, "text/plain", b"not found", &method);
    };
    write_response(sock, 200, mime_for(&path), &bytes, &method)
}

/// Strip query/fragment and percent-decode the path.
fn url_path(target: &str) -> String {
    let bare = target
        .split(['?', '#'])
        .next()
        .unwrap_or("/")
        .to_string();
    percent_decode(&bare)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &s[i + 1..i + 3];
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn write_response<S: std::io::Write>(
    sock: &mut S,
    code: u16,
    mime: &str,
    body: &[u8],
    method: &str,
) -> std::io::Result<()> {
    let status = match code {
        200 => "OK",
        405 => "Method Not Allowed",
        _ => "Not Found",
    };
    let head = format!(
        "HTTP/1.1 {code} {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\n\
         Cache-Control: no-cache\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
        body.len()
    );
    sock.write_all(head.as_bytes())?;
    if method != "HEAD" {
        sock.write_all(body)?;
    }
    sock.flush()
}

/// Content type for the handful of extensions a web build actually uses.
pub fn mime_for(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.to_str().unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" | "md" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        "wasm" => "application/wasm",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

// ---------------------------------------------------------------------------
// Capture jobs
// ---------------------------------------------------------------------------

/// One picture to take. Everything runs off the UI thread and comes back as
/// PNG bytes through a channel.
#[derive(Debug, Clone)]
pub enum Job {
    /// Screenshot `url` at `w` x `h` with headless Chromium.
    Web { url: String, w: u16, h: u16 },
    /// `adb exec-out screencap -p` from the first attached device.
    Adb,
    /// Grab the window whose title/class mentions `hint` (sway + grim).
    Window { hint: String },
}

/// Run one job to completion. Blocking — spawn it.
pub fn run(job: &Job) -> Result<Vec<u8>, String> {
    match job {
        Job::Web { url, w, h } => screenshot_web(url, *w, *h),
        Job::Adb => adb_screenshot(),
        Job::Window { hint } => window_screenshot(hint),
    }
}

/// Locate a headless Chromium: Playwright's bundled copy first (it is
/// already on disk and always the right version), then the system browser.
pub fn headless_shell() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AURA_HEADLESS") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let home = std::env::var("HOME").ok()?;
    let base = PathBuf::from(home).join(".cache/ms-playwright");
    if let Ok(rd) = std::fs::read_dir(&base) {
        for e in rd.flatten() {
            let d = e.path();
            for cand in [
                d.join("chrome-headless-shell-linux64").join("chrome-headless-shell"),
                d.join("chrome-linux64").join("chrome"),
                d.join("chrome-linux").join("chrome"),
            ] {
                if cand.is_file() {
                    return Some(cand);
                }
            }
        }
    }
    which(&["chromium", "chromium-browser", "google-chrome", "google-chrome-stable"])
}

/// Resolve a program name against `$PATH`.
pub fn which(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for name in names {
        for dir in std::env::split_paths(&path) {
            let cand = dir.join(name);
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

fn screenshot_web(url: &str, w: u16, h: u16) -> Result<Vec<u8>, String> {
    let bin = headless_shell().ok_or_else(|| {
        "no headless browser found (install chromium or set AURA_HEADLESS)".to_string()
    })?;
    let out = std::env::temp_dir().join(format!("aura-preview-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&out);

    let mut cmd = std::process::Command::new(&bin);
    cmd.args([
        "--headless",
        "--disable-gpu",
        "--no-sandbox",
        "--hide-scrollbars",
        "--disable-dev-shm-usage",
        "--force-device-scale-factor=1",
    ])
    // Chromium switches must use `=`; a space-separated `--window-size 640,400`
    // is read as an empty switch plus a URL, and no screenshot is written.
    .arg(format!("--window-size={w},{h}"))
    // Let timers, fonts and XHR settle before the shutter opens.
    .arg("--virtual-time-budget=5000")
    .arg(format!("--screenshot={}", out.display()))
    .arg(url);

    // Keep stderr: "no screenshot" is useless without the browser's reason.
    cmd.stderr(std::process::Stdio::piped());
    let mut child = cmd
        .stdout(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn {}: {e}", bin.display()))?;
    let mut err_pipe = child.stderr.take();

    // Hard stop: a page with an open websocket would otherwise never exit.
    let deadline = Duration::from_secs(30);
    let start = SystemTime::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed().unwrap_or_default() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("preview render timed out after 30s".into());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("wait: {e}")),
        }
    }

    let bytes = std::fs::read(&out).map_err(|e| {
        let mut why = String::new();
        if let Some(pipe) = err_pipe.as_mut() {
            use std::io::Read as _;
            let _ = pipe.read_to_string(&mut why);
        }
        let why = why
            .lines()
            .filter(|l| !l.is_empty())
            .take(3)
            .collect::<Vec<_>>()
            .join(" | ");
        format!(
            "no screenshot: {e}{}{why}",
            if why.is_empty() { "" } else { ": " }
        )
    })?;
    let _ = std::fs::remove_file(&out);
    if bytes.len() < 64 {
        return Err("the browser produced an empty screenshot".into());
    }
    Ok(bytes)
}

/// First device/emulator in the `adb` list, if any.
pub fn adb_device() -> Option<String> {
    let adb = which(&["adb"])?;
    let out = std::process::Command::new(adb)
        .args(["devices"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip(1)
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let serial = it.next()?;
            (it.next() == Some("device")).then(|| serial.to_string())
        })
        .next()
}

fn adb_screenshot() -> Result<Vec<u8>, String> {
    let serial = adb_device().ok_or_else(|| "no device attached".to_string())?;
    let adb = which(&["adb"]).ok_or_else(|| "adb not found on PATH".to_string())?;
    let out = std::process::Command::new(adb)
        .args(["exec-out", "-s", &serial, "screencap", "-p"])
        .output()
        .map_err(|e| format!("adb: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    if out.stdout.len() < 64 {
        return Err("the device returned an empty screen".into());
    }
    Ok(out.stdout)
}

/// Find a window by title/app id and capture exactly its rectangle.
fn window_screenshot(hint: &str) -> Result<Vec<u8>, String> {
    let rect = find_window_rect(hint).ok_or_else(|| format!("no window matching `{hint}`"))?;
    let geom = format!("{},{} {}x{}", rect.0, rect.1, rect.2, rect.3);
    let out = std::env::temp_dir().join(format!("aura-win-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let status = std::process::Command::new("grim")
        .args(["-g", &geom])
        .arg(&out)
        .status()
        .map_err(|e| format!("grim: {e}"))?;
    if !status.success() {
        return Err("grim failed".into());
    }
    let bytes = std::fs::read(&out).map_err(|e| format!("capture: {e}"))?;
    let _ = std::fs::remove_file(&out);
    Ok(bytes)
}

/// (x, y, w, h) of the first sway window whose title, app id or class
/// contains `hint`, preferring the focused one when several match.
///
/// Falls back to the focused window — unless that window is the harness
/// itself, because clicking "Capture window" gives the harness focus and a
/// self-portrait would be worse than an honest error.
pub fn find_window_rect(hint: &str) -> Option<(i32, i32, u32, u32)> {
    let out = std::process::Command::new("swaymsg")
        .args(["-t", "get_tree"])
        .output()
        .ok()?;
    let tree: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    pick_view(&tree, hint)
}

/// Pure part of [`find_window_rect`], so the choice logic is testable without
/// a compositor.
pub fn pick_view(tree: &serde_json::Value, hint: &str) -> Option<(i32, i32, u32, u32)> {
    struct Found {
        matched: Option<(i32, i32, u32, u32, bool)>,
        focused_other: Option<(i32, i32, u32, u32)>,
        focused_is_harness: bool,
    }
    let mut found = Found {
        matched: None,
        focused_other: None,
        focused_is_harness: false,
    };

    fn visit(node: &serde_json::Value, needle: &str, f: &mut Found) {
        let is_view = node.get("app_id").is_some()
            || node.get("window").map(|w| !w.is_null()).unwrap_or(false);
        if is_view {
            if let Some(r) = node.get("rect") {
                let x = r.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let y = r.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let w = r.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let h = r.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                if w > 2 && h > 2 {
                    let haystack = [
                        node.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                        node.get("app_id").and_then(|v| v.as_str()).unwrap_or(""),
                        node.get("window_properties")
                            .and_then(|p| p.get("class"))
                            .and_then(|v| v.as_str())
                            .unwrap_or(""),
                        node.get("window_properties")
                            .and_then(|p| p.get("instance"))
                            .and_then(|v| v.as_str())
                            .unwrap_or(""),
                    ]
                    .join(" ")
                    .to_lowercase();
                    let focused = node.get("focused").and_then(|v| v.as_bool()).unwrap_or(false);
                    let harness = haystack.contains("aura harness")
                        || haystack.contains("aura-harness");
                    if !harness {
                        if focused {
                            f.focused_other = Some((x, y, w, h));
                        }
                        if !needle.is_empty() && haystack.contains(needle) {
                            // A focused match always wins over a background one.
                            if focused || f.matched.is_none() {
                                f.matched = Some((x, y, w, h, focused));
                            }
                        }
                    } else if focused {
                        f.focused_is_harness = true;
                    }
                }
            }
        }
        for key in ["nodes", "floating_nodes"] {
            if let Some(list) = node.get(key).and_then(|v| v.as_array()) {
                for child in list {
                    visit(child, needle, f);
                }
            }
        }
    }

    let needle = hint.to_lowercase();
    visit(tree, &needle, &mut found);
    if let Some((x, y, w, h, _)) = found.matched {
        return Some((x, y, w, h));
    }
    if found.focused_is_harness {
        return None;
    }
    found.focused_other
}

/// True when any file under `root` changed after `since`. Bounded, so it can
/// run on a timer without the UI noticing.
pub fn changed_since(root: &Path, since: SystemTime) -> bool {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut seen = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        seen += 1;
        if seen > 4000 {
            return false;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if depth < 4 && !skipped(&e.file_name().to_string_lossy()) {
                    stack.push((p, depth + 1));
                }
            } else if let Ok(m) = e.metadata() {
                if let Ok(modified) = m.modified() {
                    if modified > since {
                        return true;
                    }
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("aura-pv-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn detects_a_web_build() {
        let d = tmp("web");
        std::fs::write(d.join("index.html"), "<html></html>").unwrap();
        assert_eq!(detect(&d), Target::Web { root: d.clone() });
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn detects_a_web_build_in_dist() {
        let d = tmp("dist");
        std::fs::create_dir_all(d.join("dist")).unwrap();
        std::fs::write(d.join("dist/index.html"), "<html></html>").unwrap();
        assert_eq!(
            detect(&d),
            Target::Web {
                root: d.join("dist")
            }
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn detects_an_apk_over_a_web_build() {
        let d = tmp("apk");
        std::fs::write(d.join("index.html"), "<html></html>").unwrap();
        let nested = d.join("app/build/outputs/apk/debug");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("app-debug.apk"), b"pk").unwrap();
        // Android wins: an APK is a finished artifact, index.html may be a source.
        match detect(&d) {
            Target::Android { apk: Some(apk) } => assert!(apk.ends_with("app-debug.apk")),
            other => panic!("expected android, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn detects_a_gradle_project_without_an_apk() {
        let d = tmp("gradle");
        std::fs::write(d.join("settings.gradle"), "rootProject.name='x'").unwrap();
        std::fs::create_dir_all(d.join("app")).unwrap();
        assert!(matches!(detect(&d), Target::Android { apk: None }));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn detects_a_godot_project() {
        let d = tmp("godot");
        std::fs::write(d.join("project.godot"), "[application]").unwrap();
        assert!(matches!(detect(&d), Target::Game { .. }));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn detects_a_rust_game_manifest() {
        let d = tmp("bevy");
        std::fs::write(d.join("Cargo.toml"), "[dependencies]\nbevy = \"0.15\"").unwrap();
        match detect(&d) {
            Target::Game { label, .. } => assert!(label.contains("Rust game"), "{label}"),
            other => panic!("expected a game, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn nothing_in_an_empty_dir() {
        let d = tmp("empty");
        assert_eq!(detect(&d), Target::None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_dir_is_none() {
        assert_eq!(detect(Path::new("/does/not/exist")), Target::None);
    }

    #[test]
    fn mime_types_cover_a_web_build() {
        assert!(mime_for(Path::new("a/b/index.html")).starts_with("text/html"));
        assert!(mime_for(Path::new("a/b/app.js")).starts_with("text/javascript"));
        assert_eq!(mime_for(Path::new("style.css")), "text/css; charset=utf-8");
        assert_eq!(mime_for(Path::new("logo.svg")), "image/svg+xml");
        assert_eq!(mime_for(Path::new("mod.wasm")), "application/wasm");
        assert_eq!(
            mime_for(Path::new("weird.zzz")),
            "application/octet-stream"
        );
    }

    #[test]
    fn url_paths_are_decoded_and_cleansed() {
        assert_eq!(url_path("/a%20b/index.html"), "/a b/index.html");
        assert_eq!(url_path("/x.css?v=3#frag"), "/x.css");
        assert_eq!(url_path("/plain"), "/plain");
    }

    #[test]
    fn percent_decode_tolerates_bad_escapes() {
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%41%42"), "AB");
    }

    #[test]
    fn server_serves_index_and_blocks_traversal() {
        let root = tmp("srv");
        std::fs::write(root.join("index.html"), "HELLO").unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/app.js"), "console.log(1)").unwrap();

        let server = StaticServer::start(root.clone()).expect("bind");
        let url = format!("http://127.0.0.1:{}", server.port);

        let body = get(&format!("{url}/"));
        assert!(body.contains("HELLO"), "{body}");
        let js = get(&format!("{url}/sub/app.js"));
        assert!(js.contains("console.log"), "{js}");

        // Traversal must not reach outside the served root.
        let escape = get(&format!("{url}/../../../etc/hostname"));
        assert!(!escape.contains("root:"), "traversal leaked: {escape}");

        let missing = get(&format!("{url}/nope.html"));
        assert!(missing.contains("not found"), "{missing}");

        drop(server);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn server_stops_on_drop() {
        let root = tmp("stop");
        std::fs::write(root.join("index.html"), "x").unwrap();
        let server = StaticServer::start(root.clone()).unwrap();
        let addr = ("127.0.0.1", server.port);
        drop(server);
        // After the drop the port must be free again.
        std::thread::sleep(Duration::from_millis(120));
        assert!(std::net::TcpListener::bind(addr).is_ok(), "port still held");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// End to end: detect a site, serve it, screenshot it, get real pixels.
    /// Skipped where no headless browser is installed.
    #[test]
    fn serves_and_screenshots_a_web_build() {
        if headless_shell().is_none() {
            eprintln!("skipped: no headless browser");
            return;
        }
        let d = tmp("e2e");
        std::fs::write(
            d.join("index.html"),
            "<!doctype html><html><body style=\"background:#123;margin:0\">\
             <div style=\"color:#0f8;font:40px sans-serif;padding:40px\">BUILD OK</div>\
             </body></html>",
        )
        .unwrap();

        let Target::Web { root } = detect(&d) else {
            panic!("detection missed the site");
        };
        let server = StaticServer::start(root).expect("serve");
        let png = run(&Job::Web {
            url: server.url() + "/",
            w: 640,
            h: 400,
        })
        .expect("screenshot");
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        // The page really rendered: decoding it must give us a full frame.
        let img = image::load_from_memory(&png).expect("decode");
        assert_eq!(img.width(), 640, "capture width");
        assert_eq!(img.height(), 400, "capture height");
        drop(server);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn detection_kinds_can_be_forced() {
        let d = tmp("kinds");
        std::fs::write(d.join("index.html"), "<html></html>").unwrap();
        std::fs::write(d.join("project.godot"), "[application]").unwrap();

        assert_eq!(detect_as(&d, Kind::Web), Target::Web { root: d.clone() });
        assert!(matches!(
            detect_as(&d, Kind::Game),
            Target::Game { .. }
        ));
        // Auto sees no APK and no gradle markers, so the web build wins over
        // the Godot marker by the ordering rule.
        assert_eq!(detect_as(&d, Kind::Auto), Target::Web { root: d.clone() });
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A miniature sway tree: two views, one of them the harness.
    fn sample_tree() -> serde_json::Value {
        serde_json::json!({
            "nodes": [
                {
                    "name": "root", "app_id": null,
                    "rect": {"x":0,"y":0,"width":3840,"height":2160},
                    "nodes": [
                        {
                            "name": "AURA harness", "app_id": "aura-harness",
                            "focused": true,
                            "rect": {"x":1800,"y":2192,"width":812,"height":1168},
                            "nodes": []
                        },
                        {
                            "name": "mygame v0.3", "app_id": "mygame",
                            "focused": false,
                            "rect": {"x":100,"y":200,"width":960,"height":540},
                            "nodes": []
                        }
                    ],
                    "floating_nodes": []
                }
            ],
            "floating_nodes": []
        })
    }

    #[test]
    fn window_pick_prefers_the_named_window() {
        let tree = sample_tree();
        assert_eq!(
            pick_view(&tree, "mygame"),
            Some((100, 200, 960, 540)),
            "the game window is found by name even though it is not focused"
        );
    }

    #[test]
    fn window_pick_never_captures_the_harness_itself() {
        let tree = sample_tree();
        // The harness holds focus (you just clicked the button), so there is no
        // honest fallback: report no window instead of a self-portrait.
        assert_eq!(pick_view(&tree, "not-a-real-game"), None);
        // An empty hint means "whatever is focused", which is also the harness.
        assert_eq!(pick_view(&tree, ""), None);
    }

    #[test]
    fn window_pick_falls_back_to_a_focused_foreign_window() {
        let mut tree = sample_tree();
        // Make the game the focused window and the harness background.
        tree["nodes"][0]["nodes"][0]["focused"] = serde_json::json!(false);
        tree["nodes"][0]["nodes"][1]["focused"] = serde_json::json!(true);
        assert_eq!(
            pick_view(&tree, "not-a-real-game"),
            Some((100, 200, 960, 540)),
            "the focused non-harness window is the sensible fallback"
        );
    }

    #[test]
    fn kind_labels_are_stable() {
        assert_eq!(Kind::Auto.label(), "Auto");
        assert_eq!(Kind::Web.label(), "Web");
        assert_eq!(Kind::Android.label(), "APK");
        assert_eq!(Kind::Game.label(), "Game");
        assert_eq!(Kind::default(), Kind::Auto);
    }

    #[test]
    fn changed_since_sees_new_files() {
        let d = tmp("mtime");
        std::fs::write(d.join("a.txt"), "1").unwrap();
        let before = SystemTime::now() - Duration::from_secs(60);
        assert!(changed_since(&d, before), "a file newer than the mark counts");
        let after = SystemTime::now() + Duration::from_secs(5);
        assert!(!changed_since(&d, after), "nothing is newer than the future");
        let _ = std::fs::remove_dir_all(&d);
    }

    fn get(url: &str) -> String {
        let resp = ureq_get(url);
        resp
    }

    /// A tiny blocking GET built on std only (the crate has no HTTP client).
    fn ureq_get(url: &str) -> String {
        let rest = url.trim_start_matches("http://");
        let (hostport, path) = rest.split_once('/').map(|(h, p)| (h, format!("/{p}"))).unwrap();
        let mut stream = std::net::TcpStream::connect(hostport).expect("connect");
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let req = format!("GET {path} HTTP/1.1\r\nHost: {hostport}\r\nConnection: close\r\n\r\n");
        std::io::Write::write_all(&mut stream, req.as_bytes()).unwrap();
        let mut out = String::new();
        let _ = std::io::Read::read_to_string(&mut stream, &mut out);
        out
    }
}
