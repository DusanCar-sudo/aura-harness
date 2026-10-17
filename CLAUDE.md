# Aura Harness — handoff (for Claude Code)

A native **egui/eframe desktop AI workstation** in Rust. One binary, no
browser, no Node server. Repo: `/mnt/bigdata/aura-harness` — public at
**github.com/DusanCar-sudo/aura-harness** (pushed 2026-10-17; history was
rewritten that day to purge `target/` blobs, so pre-rewrite hashes in older
docs are stale), master clean, v2 feature set complete and live-verified.

Read first: this file → `README.md` → `docs/SOLUTION.md` (decisions + traps).
The v2 plan/spec live locally in `.summer/plans/` and `.summer/specs/`
(that directory is gitignored).

## Shape

| crate | role |
|---|---|
| `crates/harness-core` | no GUI — API client (3 transports), agent/tool loop, provider routing, built-in tools, MCP, sessions, config, catalog, online discovery. Most tests. |
| `crates/harness-app` | egui shell — `main.rs` (layout, RightTab, app_id), `theme.rs` (9 palettes), `preview.rs` (artifact engine), `markdown.rs`, `ui/{chat,sidebar,input,settings,filetree,plugin_browser,preview}.rs`. All disk I/O. |

## Commands

```bash
cargo build --release                   # -> target/release/aura-harness
cargo test --workspace                  # 131 tests (43 app + 86 core + 2 mnemai)
cargo clippy --workspace --all-targets  # keep clean
cargo test -p harness-core --test live_deepseek -- --ignored --nocapture
                                        # 4 live API tests; need DEEPSEEK_API_KEY
                                        # (or ~/.config/aura-harness/credentials)
```

Run / kill (always through the compositor):

```bash
swaymsg exec -- "/home/dusan/.local/bin/aura-harness-launch"
pkill -f "release/aura-harness$"
```

## Machine rules (Aura OS / sway — these bit previous agents)

1. **Never launch from a bare terminal.** Go through sway's exec (above). The
   app forces the **wgpu** backend (v2 fix) because EGL/GL init can hang.
2. The window carries Wayland `app_id` `aura-harness` (set with
   `.with_app_id()` in `main.rs` — eframe's `run_native` sets only the title,
   so no sway rule could match it before). sway floats + centers it via
   `~/.config/sway/config`:
   `for_window [app_id="aura-harness"] floating enable, move position center`.
   **Do not fullscreen it — ever.** A previous agent fullscreened it for
   screenshots and Dusan read that as "the app always maximises". To test
   invisibly, move it *after* it maps:
   `swaymsg '[app_id="aura-harness"] move container to workspace "hidden"'`.
3. Capture without stealing focus: read the rect from `swaymsg -t get_tree`,
   then `grim -g "x,y WxH"`. Screenshots go to `.artifacts/` (gitignored).
4. Single display (eDP-1) and Dusan is usually working on it — avoid opening
   test windows on his workspace; use a hidden workspace.
5. **Never write `~/.config/aura-harness/config.toml` while an instance is
   running** — the app auto-saves within a frame and your edit is silently
   overwritten.
6. `scripts/install.sh` is v1-era: it would install the stale Oct-12 ELF to
   `~/.local/bin/aura-harness` and overwrite the hand-edited v2 desktop entry
   (the one that execs `aura-harness-launch`) with a v1 one. Don't run it
   blindly. The live path is: desktop entry → `~/.local/bin/aura-harness-launch`
   → `target/release/aura-harness`.

## Config

- `~/.config/aura-harness/config.toml` — theme, `right_panel_tab`,
  `project_root`, `show_hidden_files`, `auto_approve`, `background_image`,
  `background_opacity`, `system_prompt`, `model_id`, `reasoning_effort`, …
- `~/.config/aura-harness/plugins.toml` — MCP servers + command tools.
- Universal Aura config: `~/.aura/.env` (API keys) + `~/.aura/config.json`;
  `~/.aura/mcp.json` is shared with Aura Code. `AURA_HOME` overrides.
- All of the above honor `AURA_HARNESS_HOME` and the usual `XDG_*` variables.
- The live config is user-tuned (theme `ember`, model `deepseek-v4-pro`) —
  don't clobber it casually.

## Architecture notes / contracts

- **Providers** (`harness-core/src/provider/descriptors.rs`): one descriptor
  table (`KNOWN_PROVIDERS`) drives routing, API-key env names, base URLs and
  transport selection. 15 families (anthropic, openai, google, deepseek, xai,
  openrouter, ollama, lmstudio, qwen, groq, mistral, cerebras, nvidia,
  minimax, generic catch-all), 37 model prefixes. Three transports:
  OpenAI-compatible, Anthropic, Google.
- **Built-in tools** (`builtins.rs`): `shell`, `read_file`, `write_file`,
  `edit_file`, `list_files`, `grep`, `glob`. Registered in
  `ToolRegistry::new()` and **cannot be shadowed** by plugins/MCP. `shell`
  runs via `/bin/sh` (pipes/globs work), 120 s cap.
- **Approval**: mutating tools are marked needs-approval, but `auto_approve`
  defaults **true** (full access) — that is what Dusan wants by default. Turn
  it off in Settings to get Approve/Reject cards.
- **Agent loop** (`agent.rs`): `MAX_TOOL_ROUNDS = 20`; child processes run
  under `child_env()` (PATH/HOME only — they never inherit our secrets).
- **Catalog** (`catalog.rs`): 35 curated entries, 10 categories, tag-based
  semantic search ("gaming", "social media", "3d game", "summertime" are
  tested cases). `online.rs` discovers more from the public MCP registry,
  GitHub search, raw READMEs and agenttool.sh.
- **Preview** (`harness-app/src/preview.rs`): detection order = built APK →
  `index.html` (root, `dist/`, `build/`) → game (Godot project / binary /
  manifest). Web serves a local `StaticServer` and snapshots with
  Playwright's headless chromium (`~/.cache/ms-playwright`, `AURA_HEADLESS`
  override; chromium switches must use the `=` form). Android uses `adb`
  (needs an attached device); the game path is best-effort.
- **Themes**: 9 — `ThemeChoice` in core config.rs ↔ palettes in app
  theme.rs.

## State

v2 complete as of commit `597aa68` (universal config, provider routing,
catalog + search, right panel Files/Preview/Plugins, interactive file
manager, background image, 9 themes, built-in tools, preview tab,
floating-window fix). 131 tests + clippy clean; 4 ignored live API tests all
pass against the real API.

Open items (priority order):

1. DONE 2026-10-17: API streams now carry a 90s idle timeout in all three
   transports (`api.rs` + `provider/{anthropic,google}.rs`) — a stalled
   response errors out instead of hanging the turn.
2. Game + Android preview paths are best-effort (only web is verified live;
   Android needs a device attached).
3. Provider dispatch in `descriptors.rs` is hand-rolled — a cleanup pass
   would reduce duplication across the three transports.
4. No UI-level end-to-end test of a chat→tool turn (loop level is covered by
   `live_deepseek.rs`).

Repo is public: **github.com/DusanCar-sudo/aura-harness** (published
2026-10-17 with Dusan's approval). Secrets are kept out of the repo —
credentials live in `~/.config/aura-harness/` only.

Cross-agent context lives in `/mnt/agentmem/shared/` (bulletin:
`agent-bulletin.txt`; project note: `aura-harness-project.md`). Keep this
file current when commands, contracts, or open items change.
