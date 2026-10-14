# Aura Harness v2 — Design Spec

**Date:** 2026-10-15
**Status:** Approved

## Goal

Upgrade `aura-harness` from a single-provider DeepSeek chat client into a universal multi-provider AI workstation with tabbed conversations, curated plugin discovery, and universal config storage.

## Architecture

The app is a Rust `eframe`/`egui` TUI-shell with a tokio runtime. The upgrade adds:

1. **Provider layer** (`harness-core/src/provider/`) — multi-provider factory ported from aura-code's TypeScript system
2. **Universal config** (`harness-core/src/config.rs` extension) — `.env` + `config.json` shared across all Aura tools
3. **Plugin catalog** (`harness-core/src/catalog.rs`) — built-in curated database of MCP servers and skills by category
4. **UI shell** (`harness-app/src/ui/`) — tabbed chat, right-panel file tree + plugin browser

## Components

### 1. Provider Factory (ported from aura-code)

**Source:** `aura-code/src/providers/factory.ts`, `provider-descriptors.ts`, `resilient-factory.ts`

**Port to Rust:**
- `provider/descriptors.rs` — provider table (family, display name, routing prefixes, API key env vars, transport class, base URL)
- `provider/factory.rs` — routing logic: model name → provider → instantiate client
- `provider/resilience.rs` — rate limiting, retry with backoff, circuit breaker, fallback chains
- `provider/transport.rs` — HTTP client for OpenAI-compatible APIs (streaming)
- `provider/anthropic.rs` — Anthropic API client
- `provider/google.rs` — Google Gemini API client

**30+ providers:** OpenAI, Anthropic, Google, DeepSeek, xAI, OpenRouter, Together, Groq, Mistral, Cerebras, SambaNova, DeepInfra, Fireworks, NVIDIA NIM, Qwen, MiniMax, Kimi/Moonshot, Zhipu, StepFun, BytePlus, Upstage, Arcee, Tencent, GMI Cloud, Kilo Code, Alibaba, OpenCode Zen, OpenCode Go, Cohere, Hugging Face, Ollama (local), LM Studio (local)

**Routing:** Model name prefixes determine provider (e.g. `claude-*` → Anthropic, `gpt-*` → OpenAI, `deepseek/` → DeepSeek, `ollama/` → local)

### 2. Universal Config (`.env` + `config.json`)

**Location:** `~/.aura/`

**`.env` format:**
```
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
DEEPSEEK_API_KEY=sk-...
GOOGLE_API_KEY=...
XAI_API_KEY=...
OPENROUTER_API_KEY=...
# ... etc
```

**`config.json` format:**
```json
{
  "defaultModel": "deepseek/deepseek-chat",
  "effort": "high",
  "theme": "nature",
  "activePlugins": ["mnemai", "brave-search"],
  "fallbacks": ["deepseek/deepseek-chat", "openrouter/meta-llama/llama-3.3-70b"]
}
```

**Loader:** Checks `DEEPSEEK_API_KEY` env var first, then `~/.aura/.env`, then `~/.config/aura-harness/credentials` (backward compat)

### 3. Curated Plugin Catalog

**Built-in database** (`catalog.rs`) — static list of known MCP servers and skills:

| Category | Examples |
|----------|----------|
| Gaming | @modelcontextprotocol/server-everything, game-design skills, summertime |
| Social | Social Media MCP, Twitter MCP, Web Search |
| Coding | GitHub MCP, Git MCP, Filesystem MCP, sequential-thinking |
| Productivity | Memory MCP, Time MCP, Fetch MCP |
| Research | Brave Search MCP, Puppeteer MCP, PostgreSQL MCP |

**Search:** Keyword matching across name, description, and category tags. Semantic enough that "gaming" matches game-related entries.

**Install:** One-click from catalog. Manual add via command paste. Import from `~/.aura/mcp.json`.

### 4. UI Shell — Tabbed Chat + Right Panel

**Layout:**
```
┌──────────┬─────────────────────────────┬──────────────┐
│ Sidebar  │  Tab Bar [chat1] [chat2] [+] │ Right Panel  │
│          │─────────────────────────────│              │
│ Sessions │                             │ File Tree    │
│          │  Chat Messages              │ (project     │
│ + New    │                             │  files for   │
│          │  [Tool Cards]               │  attachment) │
│          │                             │──────────────│
│          │                             │ Plugin       │
│          │  [Composer]                 │ Browser      │
│          │                             │ (catalog +   │
│          │                             │  search)     │
└──────────┴─────────────────────────────┴──────────────┘
```

**Tabs:** Multiple conversations open simultaneously. Each tab has its own session, model selection, and message history.

**Right Panel:** Toggleable split view:
- Top: File tree (current working directory, for attaching files to chat)
- Bottom: Plugin browser (catalog search + one-click install + manual add)

### 5. Plugin Compatibility

- Accept ANY MCP server over stdio
- Auto-detect command wrapper (npx, uvx, node, direct binary)
- Import existing configs from `~/.aura/mcp.json`
- Skills/workflows adopted as MCP tools

## Data Flow

```
User types message
  → Tab's session builds history
  → Provider factory routes model name → correct provider
  → Provider streams response (text + tool calls)
  → Tool calls execute via MCP (auto-approve or user approval)
  → Results fed back to provider
  → Response rendered in tab
```

## Files to Create/Modify

### New files:
- `crates/harness-core/src/provider/mod.rs` — provider module
- `crates/harness-core/src/provider/descriptors.rs` — provider table
- `crates/harness-core/src/provider/factory.rs` — routing + instantiation
- `crates/harness-core/src/provider/resilience.rs` — rate limiting, retry, circuit breaker
- `crates/harness-core/src/provider/transport.rs` — OpenAI-compatible HTTP client
- `crates/harness-core/src/provider/anthropic.rs` — Anthropic client
- `crates/harness-core/src/provider/google.rs` — Google client
- `crates/harness-core/src/catalog.rs` — curated plugin database

### Modified files:
- `crates/harness-core/src/config.rs` — universal .env + config.json loading
- `crates/harness-core/src/api.rs` — multi-provider chat options
- `crates/harness-core/src/agent.rs` — provider-aware agent loop
- `crates/harness-app/src/main.rs` — tabbed UI, right panel
- `crates/harness-app/src/ui/chat.rs` — tabbed chat rendering
- `crates/harness-app/src/ui/mod.rs` — new UI modules (tabs, file tree, plugin browser)
- `crates/harness-app/src/ui/settings.rs` — universal config editing
- `crates/harness-app/Cargo.toml` — add wgpu feature for eframe

## Testing

- Provider routing tests (model name → correct provider)
- Config loading tests (.env parsing, backward compat)
- Catalog search tests
- Resilience tests (retry, circuit breaker)
- Integration test: send message through full stack

## Non-Goals (v2)

- Subagents
- Workflows/presets
- Voice
- Web search (built into providers instead)
- Account auth (API keys only)
