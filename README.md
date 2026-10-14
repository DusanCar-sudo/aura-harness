# Aura Harness

The DeepSeek harness as a standalone desktop application — one Rust binary, no
browser tab, no Node server.

Streaming chat with visible reasoning, rich markdown, file attachments, and
direct access to your plugins (MCP servers and local command tools).

## What it is

`aura-harness` is a native `egui`/`eframe` client that talks straight to the
DeepSeek chat-completions API. It is independent: its own binary, its own
config, its own sessions. It reaches the Aura memory layer the same way any
other agent does — by attaching the Mnemai MCP server as a plugin, not by
sharing code with the TUI, web client, or Droid.

## Features

- **Streaming answers** with chain-of-thought shown live in a collapsible
  "Thought for Ns" block (deepseek-flash / deepseek-v4-pro, thinking
  off/low/high/max)
- **Rich markdown** — syntax-highlighted code blocks with a copy button
- **Attachments** — pick text files (1 MiB each) and they ride along as context
- **Plugins, immediately available**
  - **MCP servers** over stdio; their tools appear to the model as
    `mcp_<server>_<tool>` and run automatically
  - **Local command tools** (`cmd_<name>`) — argv only, never a shell string,
    and every run shows an Approve/Reject card first
- **Conversations** persisted as JSONL, listed in the sidebar, reloadable after
  a crash mid-stream
- **Aura Nature** theming (forest / cream / gold / olive, stripe accents)

Out of scope for v1 (deliberately): subagents, jobs, workflows, presets, web
search, account auth, voice.

## Build

```bash
cargo build --release          # -> target/release/aura-harness
cargo test  --workspace        # 54 tests
```

## Run

```bash
./target/release/aura-harness
```

First run opens Settings, because there is no API key yet. Paste a DeepSeek key
from <https://platform.deepseek.com> and hit **Save key**; it is written to
`~/.config/aura-harness/credentials` with mode `0600`. The app also accepts
`DEEPSEEK_API_KEY` in the environment, which takes precedence.

## Files

| Path | Purpose |
|------|---------|
| `~/.config/aura-harness/config.toml` | model, reasoning effort, system prompt |
| `~/.config/aura-harness/plugins.toml` | MCP servers and command tools |
| `~/.config/aura-harness/credentials` | `api_key=…`, mode `0600` |
| `~/.local/share/aura-harness/sessions/<uuid>.jsonl` | one conversation per file |

All four honor `AURA_HARNESS_HOME` and the usual `XDG_*` variables.

## Plugins

`plugins.toml` is created on first run with comments explaining both forms:

```toml
[mcp.mnemai]                     # MCP server over stdio
command = "node"
args = ["/mnt/bigdata/aura-mnemai/mcp-server.mjs"]
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
