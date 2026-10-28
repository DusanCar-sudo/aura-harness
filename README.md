# Aura Harness

The DeepSeek harness as a standalone desktop application — one Rust binary, no
browser tab, no Node server.

Streaming chat with visible reasoning, rich markdown, file attachments, and
direct access to your plugins (MCP servers and local command tools).

## UI showcase

<p align="center">
  <img src="assets/readme/aura-harness-01-welcome.svg" alt="Aura Harness launch screen" width="100%" />
</p>

### 1. Welcome / launch view

A centered workspace with the Aura branding, floating side rail, and the minimal
conversation list that makes the app feel like a native workstation instead of a
browser tab.

<p align="center">
  <img src="assets/readme/aura-harness-02-chat.svg" alt="Aura Harness chat view" width="100%" />
</p>

### 2. Streaming assistant workspace

The main app shell keeps the conversation on the left, the active artifact preview
and tools on the right, and the composer pinned at the bottom without stealing
focus from the user.

<p align="center">
  <img src="assets/readme/aura-harness-03-settings.svg" alt="Aura Harness settings panel" width="100%" />
</p>

### 3. Settings and model controls

The settings sheet exposes the model selection, reasoning effort, system prompt,
voice stack, tool approval, and plugin configuration in a single dense but clean
layout.

<p align="center">
  <img src="assets/readme/aura-harness-04-tools.svg" alt="Aura Harness tools and plugins" width="100%" />
</p>

### 4. Built-in tools and plugin panel

The right panel surfaces built-in tools, preview modes, and plugin management so
MCP servers and local command tools are visible and usable without leaving the
app.

## What it is

`aura-harness` is a native `egui`/`eframe` client that talks straight to the
DeepSeek chat-completions API — and, since v2, to many more providers
(Anthropic, Google, xAI, OpenRouter, Qwen, Groq, Mistral, Cerebras, NVIDIA,
MiniMax, Ollama/LM Studio, and any OpenAI-compatible endpoint), routed by
model name. It is independent: its own binary, its own config, its own
sessions. It reaches the Aura memory layer the same way any
other agent does — by attaching the Mnemai MCP server as a plugin, not by
sharing code with the TUI, web client, or Droid.

## Features

- **Full model factory** — the same provider → model selector as the Aura Code
  terminal: 32 providers and 157 curated models (including **DeepSeek V4.1
  Flash 1017**), with live `/models` refresh for every provider that supports
  it. OpenRouter shows its whole catalogue, not just one Claude entry.
- **Streaming answers** with chain-of-thought shown live in a collapsible
  "Thought for Ns" block (thinking off/low/high/max)
- **Rich markdown** — syntax-highlighted code blocks with a copy button
- **Attachments** — pick text files (1 MiB each) and they ride along as context
- **Plugins, immediately available**
  - **MCP servers** over stdio; their tools appear to the model as
    `mcp_<server>_<tool>` and run automatically
  - **Local command tools** (`cmd_<name>`) — argv only, never a shell string,
    and every run shows an Approve/Reject card first
- **Conversations** persisted as JSONL, listed in the sidebar, reloadable after
  a crash mid-stream. New chats title themselves from the first prompt; rename
  with the ✎ button, a double-click, or the right-click menu.
- **Voice** — microphone button in the composer transcribes into the draft
  (Xiaomi MiMo ASR → Groq Whisper → OpenAI Whisper → ElevenLabs), and every
  assistant answer has a **speak** button (MiMo TTS → ElevenLabs → OpenAI).
- **9 themes** — Aura Nature (forest / cream / gold / olive) plus Midnight,
  Retro, Paper, Ember, Ocean, Dracula, Solarized, Graphite — with an optional
  background image behind the whole window
- **v2 extras** — right panel with an interactive file tree, an artifact
  preview tab (web / Android / game), a curated plugin catalog with semantic
  search, and built-in tools (shell, files, grep) that need no plugin

Out of scope for now (deliberately): subagents, jobs, workflows, presets, web
search, account auth.

## Build

```bash
cargo build --release          # -> target/release/aura-harness
cargo test  --workspace        # 105 unit/integration tests + live checks
```

## Run

```bash
./target/release/aura-harness
```

The factory reads the same keys as Aura Code: `~/.aura/keys.json` (wins by
default), then process environment, then `~/.aura/.env`, then the harness's own
`~/.config/aura-harness/credentials`. `AURA_KEYS_ENV_WINS=1` restores
environment-first order. The Settings sheet still accepts a DeepSeek fallback
key and writes it to `credentials` with mode `0600`.

On Aura OS (sway) the window opens as a normal floating window, centered —
not a full-screen tile. It carries the Wayland `app_id` `aura-harness`
(matching the `.desktop` file's `StartupWMClass`) and the desktop floats it:

    for_window [app_id="aura-harness"] floating enable, move position center

## Files

| Path | Purpose |
|------|---------|
| `~/.config/aura-harness/config.toml` | model, reasoning effort, system prompt, TTS voice |
| `~/.config/aura-harness/plugins.toml` | MCP servers and command tools |
| `~/.config/aura-harness/credentials` | `api_key=…`, mode `0600` |
| `~/.local/share/aura-harness/sessions/<uuid>.jsonl` | one conversation per file |

All four honor `AURA_HARNESS_HOME` and the usual `XDG_*` variables.

## Plugins

`plugins.toml` is created on first run with comments explaining both forms:

```toml
[mcp.mnemai]                     # MCP server over stdio
command = "node"
args = ["/mnt/bigdata/aura-nemai/mcp-server.mjs"]
enabled = true

[cmd.today]                      # local command tool, approved per run
command = "date"
args = ["+%Y-%m-%d"]
description = "Return today's date"
enabled = true
```

The Aura memory layer is added automatically when its server file is present.
[REA](https://github.com/morluto/rea) — reverse engineering as an MCP server —
drops in the same way: run `npx rea-agents setup`, then declare the command it
registers.

After editing, use **Settings → Reconnect**, or restart.

## Layout

```
crates/harness-core   # no GUI: API client, agent loop, MCP, tools, sessions, config
crates/harness-app    # egui/eframe shell: theme, chat, sidebar, composer, settings
```

`harness-core` is where the logic lives and where the tests are; `harness-app` is the
shell and holds all disk I/O.

## License

MIT
