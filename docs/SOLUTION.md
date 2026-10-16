# Aura Harness — how it was built, and what bit back

Notes from turning `npx @deepseek-ai/dsh web` into a standalone Rust desktop
app. Written for whoever touches this next.

## Shape

```
crates/harness-core   # no GUI: api, agent loop, mcp, cmdtool, tools, session, config
crates/harness-app    # egui/eframe shell + all disk I/O
```

The split matters: `harness-core` has no rendering dependency, so the agent loop,
SSE parser, MCP client and session format are all unit-testable. `harness-app` is
the only crate that writes to `~/.config` or `~/.local/share`.

## The DeepSeek API, as this app uses it

```
POST https://api.deepseek.com/chat/completions
Authorization: Bearer <key>
{ model: "deepseek-flash" | "deepseek-v4-pro",
  messages, stream: true, stream_options: {include_usage: true},
  thinking: {type: "enabled" | "disabled"},
  reasoning_effort: "none" | "low" | "high" | "max",
  tools: [{type: "function", function: {name, description, parameters}}] }
```

SSE frames are `data: {...}` lines terminated by `data: [DONE]`; deltas carry
`content` and/or `reasoning_content`; tool-call arguments arrive as fragments
that must be accumulated **by `index`** across frames. A 401 with a bad key
returns an OpenAI-shaped error body, which is what `Error::Api { status,
message }` surfaces.

## Decisions worth keeping

**Reasoning is never echoed back.** `reasoning_content` is display-only; the
app keeps it in the session file so the UI can show "Thought for Ns", but
`Message::to_api()` strips both it and the session-local `ts` before any
request. Without this the transcript sent to the API drifts from what the model
produced.

**MCP tool names are namespaced, but never parsed back.** Tools are exposed as
`mcp_<server>_<tool>`. Splitting that string to recover the server name breaks
the moment a server is called `my_server`. `mcp::collect_registered()` therefore
builds each `RegisteredTool` from the *map key* plus the server's own
`(original_name, ToolDef)` pairs via `list_tools_raw()`. `split_namespaced()`
is kept for display only.

**Command tools are argv, never a shell string.** `cmdtool::run` executes a
program plus an argv array with `{{key}}` substitution from the model's JSON
arguments, under a 60 s timeout. No shell means no quoting bugs and no
injection surface. Every invocation also waits on an Approve/Reject card before
it runs; MCP tools run straight through, matching the owner-responsibility
posture used elsewhere in Aura.

**MCP connections are shared and long-lived.** They live in
`Arc<tokio::sync::Mutex<BTreeMap<String, McpConnection>>>` owned by the app and
borrowed by the agent loop, so a server is spawned once, not once per turn.
`kill_on_drop(true)` means dropping the map reaps the children.

**Sessions are JSONL, not zstd.** One header line plus one line per message,
written atomically (tmp + rename). Small conversations, greppable, and a crash
mid-stream leaves a readable file. This deliberately does not reuse dsh's own
session format.

**The UI thread never blocks on the network.** The prompt is sent by spawning
`agent::run` on a tokio runtime owned by the app; events come back over an
`mpsc` channel drained with `try_recv` each frame. Dropping the receiver is the
cancel path — the agent's next `send` fails and the loop unwinds.

## Traps hit along the way

- **The API is strict in two places the OpenAI shape does not advertise.**
  (1) Every entry of an assistant message's `tool_calls` must carry
  `"type": "function"`; without it the request dies with
  `messages[N]: missing field 'type'`. `ToolCall` therefore serializes `kind`
  as `type` and never puts its local `index` on the wire.
  (2) A tool's `parameters` must be a JSON Schema of `type: "object"` — an
  empty `[cmd.x] schema` table, or an MCP server that omits `type`, is
  rejected with `schema must be a JSON Schema of 'type: "object"'`.
  `api::normalize_parameters()` covers both providers.
  Neither was caught by fixture tests; both were caught by
  `tests/live_deepseek.rs`, which is why that file runs against the real API.
- **egui cannot load woff2.** The Aura brand font ships as `geist.woff2`, which
  `epaint` will not parse, so v1 uses egui's bundled fonts. A `.ttf` would fix
  it; nothing else needs to change.
- **`rfd`'s default features pull `async-std`.** With tokio already present,
  build it as `default-features = false, features = ["xdg-portal", "tokio"]`.
- **The release profile must live at the workspace root.** Cargo silently
  ignores `[profile.release]` in a workspace member.
- **Tests that mutate `AURA_HARNESS_HOME` race.** `DEEPSEEK_API_KEY` and
  `AURA_HARNESS_HOME` are process-wide, and cargo runs test threads in parallel;
  `harness_core::env_lock::guard()` serializes the ones that touch them.
- **The `mnemai` launcher was broken.** It hard-coded
  `PYTHONPATH=$HOME/Projects/aura-mnemai`, but the checkout lives at
  `~/Downloads/Projects/aura-mnemai`, so Aura Code, Hermes *and* this app all
  failed with `No module named mnemai`. The launcher now searches
  `~/Downloads/Projects`, `~/Projects`, then `/mnt/agentmem/tools`.
- **The window "maximised over the full screen".** Two causes. Under sway a
  new window is a full-screen tile until something floats it — and the window
  had no Wayland `app_id` for a rule to match, because eframe's
  `run_native(app_name, …)` sets only the *title*. `main()` now sets
  `.with_app_id("aura-harness")` (same string as the `.desktop` file's
  `StartupWMClass`) and the desktop floats it with
  `for_window [app_id="aura-harness"] floating enable, move position center`.
  Verified live: it maps as `floating_con`, centered, at its own 1320x820.
  Corollary for whoever verifies the UI next: capture the window region
  (`grim -g "x,y WxH"`, rect from `swaymsg -t get_tree`), never fullscreen it
  for a screenshot — that is what made it look maximised in the first place.

## Verifying

```bash
cargo test --workspace     # 58 tests; the mnemai integration test skips if absent
```

The mnemonics worth remembering when something looks broken:

- no answer at all → the API key (Settings writes it 0600 to `credentials`)
- answer but no reasoning → `reasoning_effort` is `none`, i.e. thinking off
- no `cmd_*` / `mcp_*` tools in the composer → Settings → Reconnect, or check
  `~/.config/aura-harness/plugins.toml`
