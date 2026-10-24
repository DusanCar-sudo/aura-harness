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
cargo test --workspace                  # 139 tests (46 app + 91 core + 2 mnemai)
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
5. Editing `~/.config/aura-harness/config.toml` while an instance runs is now
   survivable: `Config::save` is atomic (tmp+rename) and if the file changed
   on disk since the app's last sync, the divergent copy is preserved as
   `config.toml.conflict.bak` before the app's write lands. The app's own
   values still win until restart.
6. `scripts/install.sh` (fixed 2026-10-18) now installs the binary, generates
   `~/.local/bin/aura-harness-launch` (Wayland env wrapper) and the v2 desktop
   entry — matching the live setup, safe to run. The live launcher still execs
   `target/release/aura-harness` directly; running install.sh switches it to
   the installed copy.

## Config

- `~/.config/aura-harness/config.toml` — theme, `right_panel_tab`,
  `project_root`, `show_hidden_files`, `auto_approve`, `background_image`,
  `background_opacity`, `system_prompt`, `model_id`, `reasoning_effort`,
  `tts_voice`, …
- `~/.config/aura-harness/plugins.toml` — MCP servers + command tools.
- Universal Aura config: `~/.aura/.env` (API keys) + `~/.aura/config.json`;
  `~/.aura/mcp.json` is shared with Aura Code. `AURA_HOME` overrides.
- All of the above honor `AURA_HARNESS_HOME` and the usual `XDG_*` variables.
- The live config is user-tuned (theme `ember`, model `deepseek-v4-pro`) —
  don't clobber it casually.

## Architecture notes / contracts

- **Providers** (`harness-core/src/provider/descriptors.rs`): one descriptor
  table (`KNOWN_PROVIDERS`) drives routing, API-key env names, base URLs and
  transport selection. 32 families ported from Aura Code (DeepSeek,
  OpenRouter, Anthropic, OpenAI, Google, xAI, Qwen, MiniMax, Kimi, Zhipu,
  Xiaomi, Groq, NVIDIA, FPT, BytePlus, Hugging Face, OpenCode, StepFun,
  Tencent, Fireworks, Arcee, GMI, Kilo, Upstage, Alibaba, Cerebras,
  SambaNova, Mistral, Together, Cohere, Ollama, LM Studio + generic), with
  resellers ordered before the vendors they carry. Three transports:
  OpenAI-compatible, Anthropic, Google. Chat bodies strip the factory routing
  prefix (`openrouter/`, `fpt/`, …) before the wire.
- **Model factory** (`harness-core/src/models.rs` + `ui/model_factory.rs`):
  the full provider → model selector, 157 curated models including
  `deepseek-flash` = **DeepSeek V4.1 Flash 1017**. Live `/models` refresh for
  OpenAI-compatible providers, Anthropic, Google, OpenRouter, Ollama and LM
  Studio; curated names are preferred for known live ids. Opened from the
  composer's model button or Settings → Choose model.
- **Voice** (`harness-core/src/voice.rs`): `Recorder` (`pw-record` → `parec`
  → `arecord`, raw → ffmpeg WAV), STT chain MiMo ASR → Groq Whisper → OpenAI
  Whisper → ElevenLabs, TTS chain MiMo TTS → ElevenLabs → OpenAI. Composer
  **Mic** button transcribes into the draft; **speak** on assistant messages
  plays the answer. TTS voice lives in `config.tts_voice` (default
  `mimo_default`). Keys come from the same universal lookup as the factory.
- **Chat titles**: new sessions title themselves from the first user prompt;
  `repair_untitled_sessions()` migrates old "New chat" sessions at launch.
  Rename via ✎, double-click, or right-click.
- **Switching chats stops the in-flight turn first** (`new_session`,
  `open_session`, deleting the open chat). `stop()` appends any streamed
  content as a partial assistant message to the old session before dropping
  the channels, so starting a new task cannot leak events into the new chat
  and cannot erase the old answer. A blank "New chat" is reused instead of
  stacking duplicates.
- **Built-in tools** (`builtins.rs`): `shell`, `read_file`, `write_file`,
  `edit_file`, `list_files`, `grep`, `glob`. Registered in
  `ToolRegistry::new()` and **cannot be shadowed** by plugins/MCP. `shell`
  runs via `/bin/sh` (pipes/globs work), 120 s cap.
- **Working directory**: `ToolRegistry` carries an explicit `cwd`
  (`set_cwd`/`cwd()`); `send()` sets it to the file tree's root (made
  absolute) and the built-ins resolve relative paths against it (`shell` via
  `.current_dir`). Deliberately **not** `std::env::set_current_dir` — that is
  process-global, races parallel tests and breaks a relative
  `AURA_HARNESS_HOME`. A saved root that no longer exists is ignored (empty
  cwd = the process's own folder). `cmdtool` command tools and MCP stdio
  children do **not** get the cwd. The model is told its folder by a system
  message rebuilt each send (`system_message` in `main.rs`, not stored in the
  session); `grep`'s glob filter, like `glob`, sees paths relative to the
  search root.
- **Attachments** (`harness-core/src/attach.rs`): text files inline as fenced
  prompt blocks; images are sniffed by magic bytes, base64-encoded, and sent
  as real multimodal parts (OpenAI `image_url`, Anthropic `image` source,
  Gemini `inlineData`). The composer shows thumbnails in the transcript.
  `models::supports_images` gates sending: DeepSeek V4 Pro answers 200 and
  silently ignores images, so it is blocked; V4.1 Flash and the vision route
  accept them. Attach works by picker **and** window drag-and-drop.
  **Trap:** `rfd` is built with its `tokio` backend, so never call the sync
  `FileDialog::pick_files()` — its `pollster` block panics with "there is no
  reactor running". Use `rt.block_on(AsyncFileDialog::pick_files())`.
- **Composer focus**: the text box takes focus once per `focus_composer`
  (launch, after Send/Stop, New chat, opening a session) via
  `ComposerState.request_focus`; the flag is consumed only when the composer
  renders, so it survives the Settings view.
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
floating-window fix), plus the 2026-10-18 real-use fixes (composer focus,
tool working directory) and the 2026-10-19 model-factory/voice pass.
105 unit/integration tests; clippy shows only older warnings in
tests/theme/filetree/preview. Live checks passed 2026-10-19: DeepSeek
`/models` returns `deepseek-flash` + `deepseek-v4-pro`; OpenRouter returns
400 models; MiMo TTS→ASR round trip is green.

Layout: the egui render pass lives in `src/layout.rs` (`impl eframe::App`);
`main.rs` holds state + event plumbing. Both under the 800-line ceiling;
`preview.rs` (836 non-test lines) is marginally over but mostly tests.

**2026-10-24 (transcript):** `ui/chat.rs` now derives the transcript from the
saved messages in chronological order (`transcript_rows`), so each tool call
renders under the assistant turn that asked for it and the final answer is
always last. Tool rows are one line (status, readable `$ command`, first
result line) and click to expand; non-zero exits are warnings, not green.
Completed turns end with `✓ Task finished · N tool calls · duration`; the top
bar shows `Working… / Waiting for you / Error / Ready`. DeepSeek/NIM's
`<｜DSML｜ calls>` sentinel is stripped from displayed (not saved) assistant
text. `AURA_HARNESS_OPEN_SESSION=<id>` opens a chat at launch. 60 app + 112
core tests green, including a headless egui paint-order test.

**2026-10-24 (request timeout):** the 90s stream idle timeout covered only
the response body. A server that accepted the connection and never sent
headers left a turn on `Working…` forever. The initial request is now wrapped
in the same `STREAM_IDLE_TIMEOUT` in `api.rs`,
`provider/anthropic.rs`, and `provider/google.rs`; it reports
`no response after 90s — server stalled` instead of hanging.

Open items (priority order):

1. DONE 2026-10-17: API streams now carry a 90s idle timeout in all three
   transports (`api.rs` + `provider/{anthropic,google}.rs`) — a stalled
   response errors out instead of hanging the turn.
2. Game + Android preview paths are best-effort (only web is verified live;
   Android needs a device attached).
3. Provider dispatch in `descriptors.rs` is hand-rolled — a cleanup pass
   would reduce duplication across the three transports.
4. No automated UI-level end-to-end test of a chat→tool turn (loop level is
   covered by `live_deepseek.rs`). Done by hand 2026-10-18: resume PDF →
   portfolio site in a nested headless sway, run twice (22 tool rounds before
   the cwd fix, 7 after; recipe in `docs/SOLUTION.md`).
5. A turn that needs more than `MAX_TOOL_ROUNDS = 20` stops abruptly while
   verifying; a "continue" affordance or a higher cap is undecided.

Repo is public: **github.com/DusanCar-sudo/aura-harness** (published
2026-10-17 with Dusan's approval). Secrets are kept out of the repo —
credentials live in `~/.config/aura-harness/` only.

Cross-agent context lives in `/mnt/agentmem/shared/` (bulletin:
`agent-bulletin.txt`; project note: `aura-harness-project.md`). Keep this
file current when commands, contracts, or open items change.
