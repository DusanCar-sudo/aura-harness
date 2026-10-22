//! The model factory — the full provider + model catalogue ported from the
//! Aura Code terminal UI (`src/providers/{factory,live-models}.ts`).
//!
//! Two layers:
//!   * [`PROVIDERS`] — one entry per provider family, with its API-key env
//!     names, default endpoint, routing prefix and whether it can list live
//!     models.
//!   * [`KNOWN_MODELS`] — the curated offline catalogue, grouped by family.
//!
//! The selector UI uses the static list immediately and replaces it with a
//! provider's live `/models` answer when the user asks to refresh. Everything
//! is best-effort: no key, no network, or a bad answer simply leaves the
//! curated list in place.

use std::time::Duration;

use serde_json::Value;

use crate::config::load_api_key_named;

/// One selectable provider in the factory.
#[derive(Debug, Clone, Copy)]
pub struct ProviderSpec {
    /// Stable family id, matching `provider::descriptors` and model ids.
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    /// API-key env names in lookup order; empty for local servers.
    pub env_keys: &'static [&'static str],
    /// Default endpoint (no trailing slash).
    pub base_url: Option<&'static str>,
    /// Env vars that override the endpoint, in lookup order.
    pub base_url_env: &'static [&'static str],
    /// Prefix added to a bare id returned by this provider's live `/models`.
    pub prefix: &'static str,
    /// The provider has a `/models` endpoint worth calling.
    pub live: bool,
    /// Local server: no key, and "configured" means reachable rather than keyed.
    pub local: bool,
}

/// A model row shown in the factory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub family: String,
    pub speed: String,
    pub free: bool,
}

pub struct StaticModel {
    pub id: &'static str,
    pub name: &'static str,
    pub family: &'static str,
    pub speed: &'static str,
}

const fn m(id: &'static str, name: &'static str, family: &'static str, speed: &'static str) -> StaticModel {
    StaticModel { id, name, family, speed }
}

/// Every provider the factory can route to without extra setup.
///
/// Bedrock / Azure / GitHub Copilot / Vertex / custom endpoints from the
/// terminal UI are deliberately absent: each needs its own auth or endpoint
/// scheme, and offering a model that cannot be called is worse than leaving it
/// out. All OpenAI-compatible providers are present.
pub const PROVIDERS: &[ProviderSpec] = &[
    // ── First-party & direct ───────────────────────────────────────────────
    ProviderSpec {
        id: "deepseek", name: "DeepSeek", desc: "V4.1 Flash, V4 Pro, Reasoner — direct API",
        env_keys: &["DEEPSEEK_API_KEY"], base_url: Some("https://api.deepseek.com/v1"),
        base_url_env: &["DEEPSEEK_BASE_URL"], prefix: "", live: true, local: false,
    },
    ProviderSpec {
        id: "openrouter", name: "OpenRouter", desc: "Pay-per-use aggregator, free models available",
        env_keys: &["OPENROUTER_API_KEY"], base_url: Some("https://openrouter.ai/api/v1"),
        base_url_env: &["OPENROUTER_BASE_URL"], prefix: "openrouter/", live: true, local: false,
    },
    ProviderSpec {
        id: "anthropic", name: "Anthropic", desc: "Claude models via API key",
        env_keys: &["ANTHROPIC_API_KEY", "CLAUDE_API_KEY"], base_url: Some("https://api.anthropic.com"),
        base_url_env: &["ANTHROPIC_BASE_URL"], prefix: "", live: true, local: false,
    },
    ProviderSpec {
        id: "openai", name: "OpenAI", desc: "GPT models direct API",
        env_keys: &["OPENAI_API_KEY"], base_url: Some("https://api.openai.com/v1"),
        base_url_env: &["OPENAI_BASE_URL"], prefix: "", live: true, local: false,
    },
    ProviderSpec {
        id: "google", name: "Google AI Studio", desc: "Native Gemini API",
        env_keys: &["GOOGLE_API_KEY", "GEMINI_API_KEY"], base_url: Some("https://generativelanguage.googleapis.com"),
        base_url_env: &["GOOGLE_BASE_URL", "GEMINI_BASE_URL"], prefix: "gemini/", live: true, local: false,
    },
    ProviderSpec {
        id: "xai", name: "xAI Grok", desc: "Grok models direct API",
        env_keys: &["XAI_API_KEY"], base_url: Some("https://api.x.ai/v1"),
        base_url_env: &["XAI_BASE_URL"], prefix: "", live: true, local: false,
    },
    // ── OpenAI-compatible clouds ───────────────────────────────────────────
    ProviderSpec {
        id: "qwen", name: "Qwen Cloud / DashScope", desc: "Qwen + multi-provider",
        env_keys: &["DASHSCOPE_API_KEY", "QWEN_API_KEY"],
        base_url: Some("https://dashscope-intl.aliyuncs.com/compatible-mode/v1"),
        base_url_env: &["DASHSCOPE_BASE_URL", "QWEN_BASE_URL"], prefix: "qwen/", live: true, local: false,
    },
    ProviderSpec {
        id: "minimax", name: "MiniMax", desc: "Global, Coding Plan & China",
        env_keys: &["MINIMAX_API_KEY"], base_url: Some("https://api.minimax.io/v1"),
        base_url_env: &["MINIMAX_BASE_URL"], prefix: "minimax/", live: true, local: false,
    },
    ProviderSpec {
        id: "kimi", name: "Kimi / Moonshot", desc: "Coding Plan, global & China endpoints",
        env_keys: &["MOONSHOT_API_KEY", "KIMI_API_KEY"], base_url: Some("https://api.moonshot.ai/v1"),
        base_url_env: &["MOONSHOT_BASE_URL"], prefix: "kimi/", live: true, local: false,
    },
    ProviderSpec {
        id: "zhipu", name: "Z.AI / GLM", desc: "Zhipu direct API; zhipu-coding/ for the Coding Plan",
        env_keys: &["ZHIPU_API_KEY"], base_url: Some("https://api.z.ai/api/paas/v4"),
        base_url_env: &["ZHIPU_BASE_URL"], prefix: "", live: true, local: false,
    },
    ProviderSpec {
        id: "xiaomi", name: "Xiaomi MiMo", desc: "MiMo V2.5 and V2 models, ASR + TTS",
        env_keys: &["XIAOMI_API_KEY"], base_url: Some("https://api.xiaomimimo.com/v1"),
        base_url_env: &["XIAOMI_BASE_URL"], prefix: "", live: true, local: false,
    },
    ProviderSpec {
        id: "groq", name: "Groq", desc: "Very fast inference + Whisper",
        env_keys: &["GROQ_API_KEY"], base_url: Some("https://api.groq.com/openai/v1"),
        base_url_env: &["GROQ_BASE_URL"], prefix: "groq/", live: true, local: false,
    },
    ProviderSpec {
        id: "nvidia", name: "NVIDIA NIM", desc: "Nemotron and open models via build.nvidia.com",
        env_keys: &["NVIDIA_API_KEY", "NVIDIA_NIM_API_KEY", "NIM_API_KEY"],
        base_url: Some("https://integrate.api.nvidia.com/v1"), base_url_env: &[], prefix: "nvidia/",
        live: true, local: false,
    },
    ProviderSpec {
        id: "fpt", name: "FPT Cloud AI", desc: "Marketplace: DeepSeek, GLM, Qwen, Gemma, GPT-OSS",
        env_keys: &["FPT_API_KEY"], base_url: Some("https://mkp-api.fptcloud.com/v1"),
        base_url_env: &["FPT_BASE_URL"], prefix: "fpt/", live: true, local: false,
    },
    ProviderSpec {
        id: "byteplus", name: "BytePlus ModelArk", desc: "DeepSeek V4 Flash/Pro GA, Seed, Kimi",
        env_keys: &["ARK_API_KEY"], base_url: Some("https://ark.ap-southeast.bytepluses.com/api/v3"),
        base_url_env: &["ARK_BASE_URL"], prefix: "byteplus/", live: true, local: false,
    },
    ProviderSpec {
        id: "huggingface", name: "Hugging Face", desc: "Inference Providers",
        env_keys: &["HUGGINGFACE_API_KEY", "HF_TOKEN"], base_url: Some("https://router.huggingface.co/v1"),
        base_url_env: &["HUGGINGFACE_BASE_URL"], prefix: "huggingface/", live: true, local: false,
    },
    ProviderSpec {
        id: "opencode", name: "OpenCode Zen / Go", desc: "Pay-as-you-go and $10/month subscription",
        env_keys: &["OPENCODE_API_KEY", "OPENCODE_GO_API_KEY"], base_url: Some("https://opencode.ai/zen/v1"),
        base_url_env: &["OPENCODE_BASE_URL"], prefix: "opencode/", live: true, local: false,
    },
    ProviderSpec {
        id: "stepfun", name: "StepFun", desc: "Agent / coding models",
        env_keys: &["STEPFUN_API_KEY"], base_url: Some("https://api.stepfun.com/v1"),
        base_url_env: &["STEPFUN_BASE_URL"], prefix: "stepfun/", live: true, local: false,
    },
    ProviderSpec {
        id: "tencent", name: "Tencent TokenHub", desc: "Hunyuan and partner models",
        env_keys: &["TENCENT_API_KEY"], base_url: Some("https://tokenhub.tencentmaas.com/v1"),
        base_url_env: &["TENCENT_BASE_URL"], prefix: "tencent/", live: true, local: false,
    },
    ProviderSpec {
        id: "fireworks", name: "Fireworks AI", desc: "OpenAI-compatible direct model API",
        env_keys: &["FIREWORKS_API_KEY"], base_url: Some("https://api.fireworks.ai/inference/v1"),
        base_url_env: &["FIREWORKS_BASE_URL"], prefix: "fireworks/", live: true, local: false,
    },
    ProviderSpec {
        id: "arcee", name: "Arcee AI", desc: "Trinity models, direct API",
        env_keys: &["ARCEE_API_KEY"], base_url: Some("https://conductor.arcee.ai/v1"),
        base_url_env: &["ARCEE_BASE_URL"], prefix: "arcee/", live: true, local: false,
    },
    ProviderSpec {
        id: "gmi", name: "GMI Cloud", desc: "Multi-model direct API",
        env_keys: &["GMI_API_KEY"], base_url: Some("https://api.gmi-serving.com/v1"),
        base_url_env: &["GMI_BASE_URL"], prefix: "gmi/", live: true, local: false,
    },
    ProviderSpec {
        id: "kilocode", name: "Kilo Code", desc: "Kilo Gateway API",
        env_keys: &["KILOCODE_API_KEY"], base_url: Some("https://api.kilocode.ai/api/openrouter"),
        base_url_env: &["KILOCODE_BASE_URL"], prefix: "kilocode/", live: true, local: false,
    },
    ProviderSpec {
        id: "upstage", name: "Upstage", desc: "Solar API",
        env_keys: &["UPSTAGE_API_KEY"], base_url: Some("https://api.upstage.ai/v1"),
        base_url_env: &["UPSTAGE_BASE_URL"], prefix: "upstage/", live: true, local: false,
    },
    ProviderSpec {
        id: "alibaba", name: "Alibaba Cloud Coding Plan", desc: "Dedicated coding tier",
        env_keys: &["ALIBABA_API_KEY"],
        base_url: Some("https://dashscope-intl.aliyuncs.com/compatible-mode/v1"),
        base_url_env: &["ALIBABA_BASE_URL"], prefix: "alibaba/", live: true, local: false,
    },
    ProviderSpec {
        id: "cerebras", name: "Cerebras", desc: "Ultra-fast wafer-scale inference",
        env_keys: &["CEREBRAS_API_KEY"], base_url: Some("https://api.cerebras.ai/v1"),
        base_url_env: &["CEREBRAS_BASE_URL"], prefix: "cerebras/", live: true, local: false,
    },
    ProviderSpec {
        id: "sambanova", name: "SambaNova Cloud", desc: "Fast 70B/405B inference",
        env_keys: &["SAMBANOVA_API_KEY"], base_url: Some("https://api.sambanova.ai/v1"),
        base_url_env: &["SAMBANOVA_BASE_URL"], prefix: "sambanova/", live: true, local: false,
    },
    ProviderSpec {
        id: "mistral", name: "Mistral AI", desc: "Codestral & Mistral models",
        env_keys: &["MISTRAL_API_KEY"], base_url: Some("https://api.mistral.ai/v1"),
        base_url_env: &["MISTRAL_BASE_URL"], prefix: "mistral/", live: true, local: false,
    },
    ProviderSpec {
        id: "together", name: "Together AI", desc: "Open models and fast inference",
        env_keys: &["TOGETHER_API_KEY"], base_url: Some("https://api.together.xyz/v1"),
        base_url_env: &["TOGETHER_BASE_URL"], prefix: "together/", live: true, local: false,
    },
    ProviderSpec {
        id: "cohere", name: "Cohere", desc: "Command R+ models",
        env_keys: &["COHERE_API_KEY"], base_url: Some("https://api.cohere.com/v2"),
        base_url_env: &["COHERE_BASE_URL"], prefix: "cohere/", live: true, local: false,
    },
    // ── Local backends ─────────────────────────────────────────────────────
    ProviderSpec {
        id: "ollama", name: "Ollama (local)", desc: "Local models on your GPU — free",
        env_keys: &[], base_url: Some("http://localhost:11434"),
        base_url_env: &["OLLAMA_BASE_URL", "OLLAMA_HOST"], prefix: "ollama/", live: true, local: true,
    },
    ProviderSpec {
        id: "lmstudio", name: "LM Studio", desc: "Local desktop model server",
        env_keys: &[], base_url: Some("http://localhost:1234"),
        base_url_env: &["LMSTUDIO_BASE_URL"], prefix: "lmstudio/", live: true, local: true,
    },
];

/// The curated offline catalogue. Mirrors Aura Code's `KNOWN_MODELS`, plus the
/// current DeepSeek V4.1 Flash snapshot.
pub const KNOWN_MODELS: &[StaticModel] = &[
    // ── Current DeepSeek flagship ─────────────────────────────────────────
    m("deepseek-flash", "DeepSeek V4.1 Flash 1017", "deepseek", "Fast · 1M context · current"),
    m("deepseek-v4.1-flash", "DeepSeek V4.1 Flash (alias)", "deepseek", "Fast · 1M context"),
    m("deepseek-v4-pro", "DeepSeek V4 Pro", "deepseek", "Powerful · 1M context"),
    m("deepseek/deepseek-chat", "DeepSeek Chat (V3)", "deepseek", "Powerful · fast"),
    m("deepseek/deepseek-reasoner", "DeepSeek Reasoner (R1)", "deepseek", "Reasoning · flagship"),
    m("deepseek-v4-flash", "DeepSeek V4 Flash (legacy route)", "deepseek", "Legacy · served by V4.1 Flash"),
    m("deepseek-v4-flash-vision-exp", "DeepSeek V4 Flash Vision (legacy)", "deepseek", "Vision · served by V4.1 Flash"),

    // ── Anthropic / Claude ───────────────────────────────────────────────
    m("claude-opus-4-5-20251001", "Claude Opus 4.5", "anthropic", "Powerful · strongest"),
    m("claude-sonnet-4-5-20251001", "Claude Sonnet 4.5", "anthropic", "Fast · balanced"),
    m("claude-haiku-4-5-20251001", "Claude Haiku 4.5", "anthropic", "Fastest · cheap"),
    m("claude-3-5-sonnet-20241022", "Claude 3.5 Sonnet", "anthropic", "Fast · legacy"),
    m("claude-3-5-haiku-20241022", "Claude 3.5 Haiku", "anthropic", "Fastest · legacy"),
    m("claude-3-opus-20240229", "Claude 3 Opus", "anthropic", "Powerful · legacy"),

    // ── OpenAI (offline fallback — refresh live for the current lineup) ──
    m("gpt-4o", "GPT-4o", "openai", "Powerful · multimodal"),
    m("gpt-4o-mini", "GPT-4o mini", "openai", "Fast · cheap"),
    m("gpt-4-turbo", "GPT-4 Turbo", "openai", "Powerful · legacy"),
    m("gpt-3.5-turbo", "GPT-3.5 Turbo", "openai", "Fastest · legacy"),
    m("o1", "o1", "openai", "Reasoning · flagship"),
    m("o1-mini", "o1-mini", "openai", "Reasoning · cheap"),
    m("o1-preview", "o1-preview", "openai", "Reasoning · legacy"),
    m("o3", "o3", "openai", "Reasoning · new flagship"),
    m("o3-mini", "o3-mini", "openai", "Reasoning · fast"),
    m("o4-mini", "o4-mini", "openai", "Reasoning · fastest"),

    // ── Google Gemini (offline fallback — refresh live) ──────────────────
    m("gemini-pro-latest", "Gemini Pro (latest)", "google", "Powerful · long context"),
    m("gemini-3.6-flash", "Gemini 3.6 Flash", "google", "Fast · cheap"),
    m("gemini-3.5-flash", "Gemini 3.5 Flash", "google", "Fast"),
    m("gemini-3.5-flash-lite", "Gemini 3.5 Flash Lite", "google", "Fastest · cheap"),
    m("gemini-3.1-flash-lite", "Gemini 3.1 Flash Lite", "google", "Fast · cheap"),
    m("gemini-3-flash-preview", "Gemini 3 Flash (preview)", "google", "Fast"),

    // ── Qwen (DashScope Cloud API) ───────────────────────────────────────
    m("qwen/qwen3-coder-plus", "Qwen3 Coder Plus", "qwen", "Powerful · code flagship"),
    m("qwen/qwen3-coder-flash", "Qwen3 Coder Flash", "qwen", "Fastest · code"),
    m("qwen/qwen3-max", "Qwen3 Max", "qwen", "Reasoning · flagship"),
    m("qwen/qwen3-plus", "Qwen3 Plus", "qwen", "Fast · balanced"),
    m("qwen/qwen-max", "Qwen Max", "qwen", "Powerful · general"),
    m("qwen/qwen-plus", "Qwen Plus", "qwen", "Fast · general"),
    m("qwen/qwen-turbo", "Qwen Turbo", "qwen", "Fastest · cheap"),
    m("qwen/qwen-long", "Qwen Long", "qwen", "Long context · 1M"),
    m("qwen/qwen2.5-coder-32b-instruct", "Qwen 2.5 Coder 32B", "qwen", "Powerful · code"),
    m("qwen/qwen2.5-72b-instruct", "Qwen 2.5 72B", "qwen", "Powerful · general"),

    // ── MiniMax ──────────────────────────────────────────────────────────
    m("minimax/MiniMax-Text-01", "MiniMax Text-01", "minimax", "Powerful · 4M context"),
    m("minimax/MiniMax-M2.5", "MiniMax M2.5", "minimax", "Reasoning · flagship"),
    m("minimax/MiniMax-M2.7", "MiniMax M2.7", "minimax", "Fast · agentic"),
    m("minimax/MiniMax-M3", "MiniMax M3", "minimax", "Powerful · agentic"),
    m("minimax/MiniMax-M2", "MiniMax M2", "minimax", "Fast · high speed"),
    m("minimax/abab6.5s-chat", "MiniMax Abab 6.5s", "minimax", "Fast · chat"),

    // ── Kimi / Moonshot ──────────────────────────────────────────────────
    m("kimi/kimi-k3", "Kimi K3", "kimi", "Powerful · agentic"),
    m("kimi/kimi-k2.7-code", "Kimi K2.7 Code", "kimi", "Coding"),
    m("kimi/kimi-k2.7-code-highspeed", "Kimi K2.7 Code Highspeed", "kimi", "Fast · coding"),
    m("kimi/kimi-k2.6", "Kimi K2.6", "kimi", "Balanced"),

    // ── Groq ─────────────────────────────────────────────────────────────
    m("groq/llama-3.3-70b-versatile", "Llama 3.3 70B (Groq)", "groq", "Ultra-fast · 128k"),
    m("groq/llama-3.1-8b-instant", "Llama 3.1 8B (Groq)", "groq", "Instant · 128k"),
    m("groq/deepseek-r1-distill-llama-70b", "DeepSeek R1 70B (Groq)", "groq", "Ultra-fast reasoning"),
    m("groq/mixtral-8x7b-32768", "Mixtral 8x7B (Groq)", "groq", "Fast · MoE"),

    // ── NVIDIA NIM (catalogue checked live 2026-09-29) ───────────────────
    m("nvidia/z-ai/glm-5.3-flash", "GLM 5.3 Flash (NIM)", "nvidia", "Fast · free"),
    m("nvidia/z-ai/glm-5.3", "GLM 5.3 (NIM)", "nvidia", "Powerful · free"),
    m("nvidia/deepseek-ai/deepseek-v4.1-flash", "DeepSeek V4.1 Flash (NIM)", "nvidia", "Fast · free"),
    m("nvidia/nemotron-3-ultra-550b-a55b", "Nemotron 3 Ultra 550B (NIM)", "nvidia", "Powerful · free"),

    // ── Xiaomi MiMo ──────────────────────────────────────────────────────
    m("mimo-v2.5-pro", "MiMo V2.5 Pro", "xiaomi", "Powerful · 1T params"),
    m("mimo-v2.5", "MiMo V2.5", "xiaomi", "Fast · 310B"),
    m("mimo-v2-flash", "MiMo V2 Flash", "xiaomi", "Fastest · pay-as-you-go (sk-) keys only"),
    m("mimo-v1", "MiMo V1", "xiaomi", "Legacy · pay-as-you-go (sk-) keys only"),

    // ── Zhipu (Z.ai GLM) ─────────────────────────────────────────────────
    m("glm-5.2", "GLM-5.2", "zhipu", "Powerful · 1M context"),
    m("glm-5.1", "GLM-5.1", "zhipu", "Powerful · agentic"),
    m("glm-5", "GLM-5", "zhipu", "Powerful · 744B MoE"),
    m("zhipu-coding/glm-5.2", "GLM-5.2 (Coding Plan)", "zhipu", "Subscription quota"),

    // ── xAI Grok ─────────────────────────────────────────────────────────
    m("grok-2", "Grok 2", "xai", "Powerful"),
    m("grok-2-mini", "Grok 2 mini", "xai", "Fast · cheap"),
    m("grok-beta", "Grok Beta", "xai", "Fast"),
    m("grok-vision-beta", "Grok Vision Beta", "xai", "Multimodal"),

    // ── StepFun ──────────────────────────────────────────────────────────
    m("stepfun/step-2-16k", "Step-2 16K", "stepfun", "Powerful · flagship"),
    m("stepfun/step-1-8k", "Step-1 8K", "stepfun", "Fast · balanced"),
    m("stepfun/step-1-flash", "Step-1 Flash", "stepfun", "Fastest · cheap"),

    // ── Fireworks AI ─────────────────────────────────────────────────────
    m("fireworks/accounts/fireworks/models/deepseek-r1", "DeepSeek R1 (Fireworks)", "fireworks", "Reasoning · fast"),
    m("fireworks/accounts/fireworks/models/deepseek-v3", "DeepSeek V3 (Fireworks)", "fireworks", "Powerful · fast"),
    m("fireworks/accounts/fireworks/models/llama-v3p3-70b-instruct", "Llama 3.3 70B (Fireworks)", "fireworks", "Balanced · 128k"),

    // ── FPT Cloud AI ─────────────────────────────────────────────────────
    m("fpt/DeepSeek-V4-Flash", "DeepSeek V4 Flash (FPT)", "fpt", "Fast · marketplace"),
    m("fpt/GLM-5.2", "GLM-5.2 (FPT)", "fpt", "Powerful · marketplace"),

    // ── BytePlus ModelArk ────────────────────────────────────────────────
    m("byteplus/deepseek-v4-flash-ga-260731", "DeepSeek V4 Flash GA", "byteplus", "Fast · GA build"),
    m("byteplus/deepseek-v4-pro-ga-260813", "DeepSeek V4 Pro GA", "byteplus", "Powerful · GA build"),

    // ── Upstage ──────────────────────────────────────────────────────────
    m("upstage/solar-pro", "Solar Pro", "upstage", "Powerful · 64k"),
    m("upstage/solar-mini", "Solar Mini", "upstage", "Fast · cheap"),

    // ── Arcee AI ─────────────────────────────────────────────────────────
    m("arcee/trinity-large-preview", "Trinity Large Preview", "arcee", "Powerful · agentic"),
    m("arcee/arcee-spark", "Arcee Spark", "arcee", "Fast · compact"),

    // ── Tencent TokenHub ─────────────────────────────────────────────────
    m("tencent/hunyuan-large", "Hunyuan Large", "tencent", "Powerful · MoE"),
    m("tencent/hunyuan-standard", "Hunyuan Standard", "tencent", "Balanced · 32k"),

    // ── GMI Cloud ────────────────────────────────────────────────────────
    m("gmi/deepseek-ai/deepseek-r1", "DeepSeek R1 (GMI)", "gmi", "Reasoning · direct"),
    m("gmi/deepseek-ai/deepseek-v3", "DeepSeek V3 (GMI)", "gmi", "Powerful · direct"),

    // ── Kilo Code ────────────────────────────────────────────────────────
    m("kilocode/auto", "Kilo Auto", "kilocode", "Auto-routed"),

    // ── Alibaba Cloud Coding Plan ────────────────────────────────────────
    m("alibaba/qwen3-coder-plus", "Qwen3 Coder Plus (Alibaba Plan)", "alibaba", "Powerful · Coding Tier"),
    m("alibaba/qwen-max", "Qwen Max (Alibaba Plan)", "alibaba", "Powerful · Coding Tier"),

    // ── OpenCode Zen / Go ────────────────────────────────────────────────
    m("opencode/big-pickle", "Big Pickle (free)", "opencode", "Powerful · free"),
    m("opencode/mimo-v2.5-free", "MiMo V2.5 (free)", "opencode", "Fast · free"),
    m("opencode/nemotron-3-ultra-free", "Nemotron 3 Ultra (free)", "opencode", "Powerful · free"),
    m("opencode/hy3-free", "HY3 (free)", "opencode", "Fast · free"),
    m("opencode/gpt-5.4", "GPT-5.4", "opencode", "Powerful · paid"),
    m("opencode/claude-sonnet-5", "Claude Sonnet 5", "opencode", "Powerful · paid"),
    m("go-anthropic/claude-sonnet-5", "Claude Sonnet 5 (Go)", "opencode", "Powerful"),
    m("go-anthropic/claude-haiku-4-5", "Claude Haiku 4.5 (Go)", "opencode", "Fast"),
    m("go-anthropic/claude-opus-5", "Claude Opus 5 (Go)", "opencode", "Powerful"),
    m("go-anthropic/minimax-m3", "MiniMax M3 (Go)", "opencode", "Anthropic API · agentic"),
    m("go-anthropic/minimax-m2.7", "MiniMax M2.7 (Go)", "opencode", "Anthropic API · fast"),
    m("go-anthropic/minimax-m2.5", "MiniMax M2.5 (Go)", "opencode", "Anthropic API · budget"),
    m("go-anthropic/qwen3.7-max", "Qwen3.7 Max (Go)", "opencode", "Anthropic API · powerful"),
    m("go-anthropic/qwen3.7-plus", "Qwen3.7 Plus (Go)", "opencode", "Anthropic API · balanced"),
    m("go-anthropic/qwen3.6-plus", "Qwen3.6 Plus (Go)", "opencode", "Anthropic API · balanced"),

    // ── OpenRouter (offline fallback — refresh live for the full catalogue) ─
    m("openrouter/anthropic/claude-3.5-sonnet", "Claude 3.5 Sonnet (OR)", "openrouter", "Fast"),
    m("openrouter/anthropic/claude-3-opus", "Claude 3 Opus (OR)", "openrouter", "Powerful"),
    m("openrouter/openai/gpt-4o", "GPT-4o (OR)", "openrouter", "Powerful"),
    m("openrouter/openai/o1", "o1 (OR)", "openrouter", "Reasoning"),
    m("openrouter/google/gemini-2.0-flash-exp", "Gemini 2.0 Flash (OR)", "openrouter", "Fast"),
    m("openrouter/meta-llama/llama-3.1-405b-instruct", "Llama 3.1 405B (OR)", "openrouter", "Open · powerful"),
    m("openrouter/meta-llama/llama-3.1-70b-instruct", "Llama 3.1 70B (OR)", "openrouter", "Open · fast"),
    m("openrouter/meta-llama/llama-3.1-8b-instruct", "Llama 3.1 8B (OR)", "openrouter", "Open · cheap"),
    m("openrouter/mistralai/mistral-large-latest", "Mistral Large (OR)", "openrouter", "Powerful"),
    m("openrouter/mistralai/mixtral-8x7b-instruct", "Mixtral 8x7B (OR)", "openrouter", "Open · fast"),
    m("openrouter/qwen/qwen-2.5-72b-instruct", "Qwen 2.5 72B (OR)", "openrouter", "Open · strong"),
    m("openrouter/qwen/qwen-2.5-coder-32b-instruct", "Qwen 2.5 Coder 32B (OR)", "openrouter", "Open · code"),
    m("openrouter/deepseek/deepseek-chat", "DeepSeek V3 (OR)", "openrouter", "Open · strong"),
    m("openrouter/deepseek/deepseek-r1", "DeepSeek R1 (OR)", "openrouter", "Reasoning · open"),
    m("openrouter/deepseek/deepseek-v4-pro", "DeepSeek V4 Pro (OR)", "openrouter", "Powerful · open"),
    m("openrouter/deepseek/deepseek-flash", "DeepSeek V4.1 Flash (OR)", "openrouter", "Fast · open"),
    m("openrouter/google/gemma-2-27b-it", "Gemma 2 27B (OR)", "openrouter", "Open · fast"),

    // ── Cerebras ─────────────────────────────────────────────────────────
    m("cerebras/llama-3.3-70b", "Llama 3.3 70B (Cerebras free)", "cerebras", "Ultra-fast · free"),
    m("cerebras/llama3.1-8b", "Llama 3.1 8B (Cerebras free)", "cerebras", "Instant · free"),
    m("cerebras/deepseek-r1-distill-llama-70b", "DeepSeek R1 70B (Cerebras free)", "cerebras", "Reasoning · free"),
    m("cerebras/gpt-oss-120b", "GPT OSS 120B (Cerebras free)", "cerebras", "Powerful · free"),

    // ── SambaNova ────────────────────────────────────────────────────────
    m("sambanova/Meta-Llama-3.3-70B-Instruct", "Llama 3.3 70B (SambaNova free)", "sambanova", "Ultra-fast · free"),
    m("sambanova/DeepSeek-R1-Distill-Llama-70B", "DeepSeek R1 70B (SambaNova free)", "sambanova", "Reasoning · free"),
    m("sambanova/Meta-Llama-3.1-405B-Instruct", "Llama 3.1 405B (SambaNova free)", "sambanova", "Powerful · free"),

    // ── Mistral AI ───────────────────────────────────────────────────────
    m("mistral/codestral-latest", "Codestral Latest", "mistral", "Code · fast"),
    m("mistral/mistral-large-latest", "Mistral Large Latest", "mistral", "Powerful · flagship"),
    m("mistral/mistral-small-latest", "Mistral Small Latest", "mistral", "Fast · cheap"),
    m("mistral/open-mistral-nemo", "Mistral Nemo", "mistral", "Fast · compact"),

    // ── Together AI ──────────────────────────────────────────────────────
    m("together/meta-llama/Llama-3.3-70B-Instruct-Turbo", "Llama 3.3 70B Turbo (Together)", "together", "Fast · 128k"),
    m("together/deepseek-ai/DeepSeek-R1", "DeepSeek R1 (Together)", "together", "Reasoning · fast"),
    m("together/Qwen/Qwen2.5-Coder-32B-Instruct", "Qwen 2.5 Coder 32B (Together)", "together", "Code · 32k"),

    // ── Cohere ───────────────────────────────────────────────────────────
    m("cohere/command-r-plus-08-2024", "Command R+ 08-2024", "cohere", "Powerful · 128k"),
    m("cohere/command-r-08-2024", "Command R 08-2024", "cohere", "Fast · 128k"),

    // ── Hugging Face ─────────────────────────────────────────────────────
    m("huggingface/meta-llama/Llama-3.3-70B-Instruct", "Llama 3.3 70B (HF)", "huggingface", "Open · 128k"),
    m("huggingface/deepseek-ai/DeepSeek-R1", "DeepSeek R1 (HF)", "huggingface", "Reasoning · open"),
    m("huggingface/Qwen/Qwen2.5-Coder-32B-Instruct", "Qwen 2.5 Coder 32B (HF)", "huggingface", "Code · open"),

    // ── Ollama (local) ───────────────────────────────────────────────────
    m("ollama/llama3.2", "Llama 3.2 (local)", "ollama", "Local · small"),
    m("ollama/llama3.1", "Llama 3.1 (local)", "ollama", "Local · 8B-70B"),
    m("ollama/llama3.3", "Llama 3.3 (local)", "ollama", "Local · 70B"),
    m("ollama/qwen2.5", "Qwen 2.5 (local)", "ollama", "Local · multilingual"),
    m("ollama/qwen2.5-coder", "Qwen 2.5 Coder (local)", "ollama", "Local · code"),
    m("ollama/codellama", "Code Llama (local)", "ollama", "Local · code"),
    m("ollama/mistral", "Mistral (local)", "ollama", "Local · 7B"),
    m("ollama/mistral-nemo", "Mistral Nemo (local)", "ollama", "Local · 12B"),
    m("ollama/mixtral", "Mixtral (local)", "ollama", "Local · MoE"),
    m("ollama/phi3", "Phi-3 (local)", "ollama", "Local · tiny"),
    m("ollama/gemma2", "Gemma 2 (local)", "ollama", "Local · Google"),
    m("ollama/deepseek-coder-v2", "DeepSeek Coder V2 (local)", "ollama", "Local · code"),
    m("ollama/command-r", "Command-R (local)", "ollama", "Local · Cohere"),

    // ── LM Studio / local OpenAI-compatible ─────────────────────────────
    m("local/qwen2.5-coder-32b-instruct", "Qwen 2.5 Coder 32B (local)", "lmstudio", "Local · code"),
    m("local/llama-3.3-70b-instruct", "Llama 3.3 70B (local)", "lmstudio", "Local · strong"),
    m("local/mistral-large", "Mistral Large (local)", "lmstudio", "Local · powerful"),
];

pub fn provider(id: &str) -> Option<&'static ProviderSpec> {
    PROVIDERS.iter().find(|p| p.id == id)
}

pub fn provider_for_model(model_id: &str) -> &'static str {
    crate::provider::provider_family(model_id)
}

/// The curated list, as owned rows the UI can merge with live results.
pub fn known_models() -> Vec<ModelInfo> {
    KNOWN_MODELS
        .iter()
        .map(|m| ModelInfo {
            id: m.id.to_string(),
            name: m.name.to_string(),
            family: m.family.to_string(),
            speed: m.speed.to_string(),
            free: m.speed.to_lowercase().contains("free"),
        })
        .collect()
}

pub fn models_for(family: &str) -> Vec<ModelInfo> {
    known_models().into_iter().filter(|m| m.family == family).collect()
}

pub fn is_known(model_id: &str) -> bool {
    KNOWN_MODELS.iter().any(|m| m.id == model_id)
}

/// Whether the selected model can take image content parts.
///
/// Deliberately generous for aggregators and local servers: if we are unsure,
/// let the request through — the provider answers with its own clear error.
/// The important half is knowing which mainstream text models cannot, so the
/// composer can say so before spending a round-trip.
pub fn supports_images(model_id: &str) -> bool {
    let id = model_id.trim().to_ascii_lowercase();
    if id.is_empty() {
        return false;
    }
    match crate::provider::provider_family(&id) {
        "anthropic" => true,
        "google" => true,
        "openai" => [
            "gpt-4o",
            "gpt-4.1",
            "gpt-4-turbo",
            "gpt-5",
            "o3",
            "o4",
            "computer-use",
        ]
        .iter()
        .any(|p| id.contains(p)),
        // DeepSeek V4 Pro answers 200 and silently ignores images, so it must
        // stay blocked. V4.1 Flash and the vision route do process them
        // (verified live 2026-10-22).
        "deepseek" => {
            id.contains("vision")
                || matches!(
                    id.as_str(),
                    "deepseek-flash" | "deepseek-v4.1-flash" | "deepseek-v4-flash"
                )
        }
        "xai" => id.contains("vision") || id.contains("grok-3") || id.contains("grok-4"),
        "openrouter" => true,
        _ => ["vision", "-vl", "vl-", "multimodal", "omni", "llava", "pixtral"]
            .iter()
            .any(|p| id.contains(p)),
    }
}

/// Human name for the selected model id, falling back to the id itself.
pub fn display_name(model_id: &str) -> String {
    KNOWN_MODELS
        .iter()
        .find(|m| m.id == model_id)
        .map(|m| m.name.to_string())
        .unwrap_or_else(|| model_id.to_string())
}

/// Filter a model list by a substring of its name, id, family or speed.
pub fn filter_models(models: &[ModelInfo], query: &str) -> Vec<ModelInfo> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return models.to_vec();
    }
    models
        .iter()
        .filter(|m| {
            m.name.to_lowercase().contains(&q)
                || m.id.to_lowercase().contains(&q)
                || m.family.to_lowercase().contains(&q)
                || m.speed.to_lowercase().contains(&q)
        })
        .cloned()
        .collect()
}

/// Filter providers by name, id or description.
pub fn filter_providers(query: &str) -> Vec<&'static ProviderSpec> {
    let q = query.trim().to_lowercase();
    PROVIDERS
        .iter()
        .filter(|p| {
            q.is_empty()
                || p.name.to_lowercase().contains(&q)
                || p.id.to_lowercase().contains(&q)
                || p.desc.to_lowercase().contains(&q)
        })
        .collect()
}

/// Prefer the curated display name for a known id. Live endpoint names are
/// often raw (`deepseek-flash`); the factory shows the name a person recognises.
fn enrich(id: String, fallback_name: String, family: &str, speed: &str, free: bool) -> ModelInfo {
    if let Some(known) = KNOWN_MODELS.iter().find(|m| m.id == id) {
        return ModelInfo {
            id,
            name: known.name.to_string(),
            family: family.to_string(),
            speed: speed.to_string(),
            free: free || known.speed.to_lowercase().contains("free"),
        };
    }
    ModelInfo {
        id,
        name: fallback_name,
        family: family.to_string(),
        speed: speed.to_string(),
        free,
    }
}

/// First configured API key for a provider (any of its env names).
pub fn api_key_for(spec: &ProviderSpec) -> Option<String> {
    spec.env_keys
        .iter()
        .find_map(|name| load_api_key_named(name))
}

/// True when a provider can be called right now.
pub fn is_configured(spec: &ProviderSpec) -> bool {
    if spec.local {
        return true;
    }
    api_key_for(spec).is_some()
}

/// Human-readable key status for the factory row.
pub fn key_status(spec: &ProviderSpec) -> &'static str {
    if spec.local {
        "local server"
    } else if api_key_for(spec).is_some() {
        "key ready"
    } else if spec.env_keys.is_empty() {
        "no key needed"
    } else {
        "no key"
    }
}

/// Endpoint for a provider, honouring its `<X>_BASE_URL` override.
pub fn base_url_for(spec: &ProviderSpec) -> String {
    for env_name in spec.base_url_env {
        if let Ok(url) = std::env::var(env_name) {
            let url = url.trim().trim_end_matches('/');
            if !url.is_empty() {
                // OLLAMA_HOST is a bare host:port; everything else is a URL.
                return if url.starts_with("http") { url.to_string() } else { format!("http://{url}") };
            }
        }
    }
    // Xiaomi's endpoint depends on the key type.
    if spec.id == "xiaomi" {
        let key = api_key_for(spec).unwrap_or_default();
        return if key.starts_with("tp-") {
            "https://token-plan-sgp.xiaomimimo.com/v1".to_string()
        } else {
            "https://api.xiaomimimo.com/v1".to_string()
        };
    }
    spec.base_url.unwrap_or("").trim_end_matches('/').to_string()
}

/// Live model list for one provider. Best-effort: returns `Err` with a short
/// reason the UI can show, and never panics on a malformed answer.
pub async fn fetch_live_models(
    http: &reqwest::Client,
    spec: &ProviderSpec,
) -> Result<Vec<ModelInfo>, String> {
    if !spec.live {
        return Err("this provider has no live model list".into());
    }
    let timeout = Duration::from_secs(8);
    let fut = fetch_live_inner(http, spec);
    match tokio::time::timeout(timeout, fut).await {
        Ok(result) => result,
        Err(_) => Err("timed out after 8s".into()),
    }
}

async fn fetch_live_inner(
    http: &reqwest::Client,
    spec: &ProviderSpec,
) -> Result<Vec<ModelInfo>, String> {
    let key = api_key_for(spec);
    if !spec.local && spec.id != "openrouter" && key.is_none() {
        return Err("add an API key for this provider first".into());
    }

    match spec.id {
        "anthropic" => fetch_anthropic(http, key.as_deref().unwrap_or("")).await,
        "google" => fetch_google(http, key.as_deref().unwrap_or("")).await,
        "ollama" => fetch_ollama(http, spec).await,
        "openrouter" => fetch_openrouter(http, key.as_deref()).await,
        _ => fetch_openai_compatible(http, spec, key.as_deref()).await,
    }
}

async fn fetch_openai_compatible(
    http: &reqwest::Client,
    spec: &ProviderSpec,
    key: Option<&str>,
) -> Result<Vec<ModelInfo>, String> {
    let base = base_url_for(spec);
    if base.is_empty() {
        return Err("no endpoint configured".into());
    }
    let url = format!("{base}/models");
    let mut req = http.get(url);
    if let Some(key) = key {
        req = req.bearer_auth(key);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    let value: Value = resp.json().await.map_err(|e| e.to_string())?;
    let items = value
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| value.get("models").and_then(Value::as_array))
        .ok_or_else(|| "unexpected answer (no data array)".to_string())?;

    let mut out: Vec<ModelInfo> = items
        .iter()
        .filter_map(|item| {
            let id = item.get("id").and_then(Value::as_str)?;
            if id.is_empty() {
                return None;
            }
            let name = item.get("name").and_then(Value::as_str).unwrap_or(id);
            let free = item
                .get("pricing")
                .and_then(|p| p.get("prompt"))
                .and_then(Value::as_str)
                .map(|p| p == "0")
                .unwrap_or(false);
            Some(enrich(
                format!("{}{}", spec.prefix, id),
                name.to_string(),
                spec.id,
                "Live",
                free,
            ))
        })
        .collect();
    out.sort_by_key(|m| m.id.to_lowercase());
    out.dedup_by(|a, b| a.id.eq_ignore_ascii_case(&b.id));
    if out.is_empty() {
        return Err("provider returned an empty model list".into());
    }
    Ok(out)
}

async fn fetch_anthropic(http: &reqwest::Client, key: &str) -> Result<Vec<ModelInfo>, String> {
    if key.is_empty() {
        return Err("add ANTHROPIC_API_KEY first".into());
    }
    let mut out = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..10 {
        let mut url = "https://api.anthropic.com/v1/models?limit=100".to_string();
        if let Some(after_id) = &after {
            url.push_str("&after_id=");
            url.push_str(after_id);
        }
        let resp = http
            .get(url)
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return if out.is_empty() {
                Err(format!("HTTP {}", resp.status().as_u16()))
            } else {
                Ok(out)
            };
        }
        let value: Value = resp.json().await.map_err(|e| e.to_string())?;
        if let Some(items) = value.get("data").and_then(Value::as_array) {
            for item in items {
                if item.get("type").and_then(Value::as_str) != Some("model") {
                    continue;
                }
                let Some(id) = item.get("id").and_then(Value::as_str) else { continue };
                let name = item.get("display_name").and_then(Value::as_str).unwrap_or(id);
                out.push(ModelInfo {
                    id: id.to_string(),
                    name: name.to_string(),
                    family: "anthropic".into(),
                    speed: "Live".into(),
                    free: false,
                });
            }
        }
        let has_more = value.get("has_more").and_then(Value::as_bool).unwrap_or(false);
        let last = value.get("last_id").and_then(Value::as_str).map(str::to_string);
        match (has_more, last) {
            (true, Some(id)) => after = Some(id),
            _ => break,
        }
    }
    if out.is_empty() {
        return Err("provider returned an empty model list".into());
    }
    Ok(out)
}

async fn fetch_google(http: &reqwest::Client, key: &str) -> Result<Vec<ModelInfo>, String> {
    if key.is_empty() {
        return Err("add GOOGLE_API_KEY first".into());
    }
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models?key={key}");
    let resp = http.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    let value: Value = resp.json().await.map_err(|e| e.to_string())?;
    let items = value
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| "unexpected answer (no models array)".to_string())?;
    let mut out = Vec::new();
    for item in items {
        let supports = item
            .get("supportedGenerationMethods")
            .and_then(Value::as_array)
            .map(|methods| methods.iter().any(|m| m.as_str() == Some("generateContent")))
            .unwrap_or(true);
        if !supports {
            continue;
        }
        let Some(name) = item.get("name").and_then(Value::as_str) else { continue };
        let id = name.strip_prefix("models/").unwrap_or(name);
        let display = item.get("displayName").and_then(Value::as_str).unwrap_or(id);
        out.push(ModelInfo {
            id: format!("gemini/{id}"),
            name: display.to_string(),
            family: "google".into(),
            speed: "Live".into(),
            free: false,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    if out.is_empty() {
        return Err("provider returned an empty model list".into());
    }
    Ok(out)
}

async fn fetch_ollama(http: &reqwest::Client, spec: &ProviderSpec) -> Result<Vec<ModelInfo>, String> {
    let base = base_url_for(spec);
    let base = base.trim_end_matches("/v1").trim_end_matches('/');
    let resp = http
        .get(format!("{base}/api/tags"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} (is Ollama running?)", resp.status().as_u16()));
    }
    let value: Value = resp.json().await.map_err(|e| e.to_string())?;
    let items = value
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| "unexpected answer (no models array)".to_string())?;
    let mut out = Vec::new();
    for item in items {
        let Some(name) = item.get("name").and_then(Value::as_str) else { continue };
        out.push(ModelInfo {
            id: format!("ollama/{name}"),
            name: name.to_string(),
            family: "ollama".into(),
            speed: "Local".into(),
            free: true,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    if out.is_empty() {
        return Err("no local models installed".into());
    }
    Ok(out)
}

async fn fetch_openrouter(
    http: &reqwest::Client,
    key: Option<&str>,
) -> Result<Vec<ModelInfo>, String> {
    let mut req = http.get("https://openrouter.ai/api/v1/models");
    if let Some(key) = key {
        req = req.bearer_auth(key);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    let value: Value = resp.json().await.map_err(|e| e.to_string())?;
    let items = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "unexpected answer (no data array)".to_string())?;
    let mut out: Vec<ModelInfo> = items
        .iter()
        .filter_map(|item| {
            let id = item.get("id").and_then(Value::as_str)?;
            let name = item.get("name").and_then(Value::as_str).unwrap_or(id);
            let free = item
                .get("pricing")
                .and_then(|p| p.get("prompt"))
                .and_then(Value::as_str)
                .map(|p| p == "0")
                .unwrap_or(false);
            Some(enrich(
                format!("openrouter/{id}"),
                name.to_string(),
                "openrouter",
                "Live",
                free,
            ))
        })
        .collect();
    // 300+ models: keep the list navigable, free ones first, then alphabetical.
    out.sort_by(|a, b| {
        b.free
            .cmp(&a.free)
            .then_with(|| a.id.to_lowercase().cmp(&b.id.to_lowercase()))
    });
    out.truncate(400);
    if out.is_empty() {
        return Err("provider returned an empty model list".into());
    }
    Ok(out)
}

/// True when a recorder binary is available.
pub fn recorder_available() -> bool {
    ["pw-record", "parec", "arecord"].iter().any(|bin| which(bin))
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let path = dir.join(bin);
                path.is_file()
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_is_unique_and_populated() {
        assert!(PROVIDERS.len() >= 30, "expected the full provider list");
        assert!(KNOWN_MODELS.len() >= 150, "expected the full model catalogue");
        let mut ids: Vec<_> = KNOWN_MODELS.iter().map(|m| m.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), KNOWN_MODELS.len(), "duplicate model ids");
        for provider in PROVIDERS {
            assert!(!provider.name.is_empty());
            assert!(!provider.desc.is_empty());
        }
    }

    #[test]
    fn every_static_model_has_a_provider() {
        for model in KNOWN_MODELS {
            assert!(
                provider(model.family).is_some(),
                "{} belongs to unknown provider {}",
                model.id,
                model.family
            );
        }
    }

    #[test]
    fn deepseek_v41_flash_is_present() {
        let found = KNOWN_MODELS
            .iter()
            .find(|m| m.id == "deepseek-flash")
            .expect("DeepSeek V4.1 Flash 1017 must be in the factory");
        assert!(found.name.contains("V4.1 Flash"));
        assert!(found.name.contains("1017"));
    }

    #[test]
    fn image_capability_is_honest_about_text_models() {
        assert!(supports_images("claude-opus-4-5-20251001"));
        assert!(supports_images("gemini-2.5-flash"));
        assert!(supports_images("gpt-4o"));
        assert!(supports_images("deepseek-v4-flash-vision-exp"));
        assert!(supports_images("deepseek-flash"));
        assert!(supports_images("grok-vision-beta"));
        assert!(!supports_images("deepseek-v4-pro"));
        assert!(!supports_images("deepseek/deepseek-chat"));
        assert!(!supports_images("gpt-3.5-turbo"));
        assert!(!supports_images(""));
    }

    #[test]
    fn openrouter_has_more_than_claude() {
        let models = models_for("openrouter");
        assert!(models.len() > 5);
        assert!(models.iter().any(|m| m.id.contains("deepseek")));
        assert!(models.iter().any(|m| m.id.contains("llama")));
        assert!(models.iter().any(|m| m.id.contains("gemini")));
    }

    #[test]
    fn filter_matches_name_and_id() {
        let models = models_for("deepseek");
        assert!(filter_models(&models, "v4.1").iter().any(|m| m.id == "deepseek-flash"));
        assert!(filter_models(&models, "reasoner").iter().any(|m| m.id.contains("reasoner")));
        assert!(filter_models(&models, "").len() == models.len());
    }

    #[test]
    fn provider_filter_and_lookup_work() {
        assert_eq!(filter_providers("openrouter").len(), 1);
        assert_eq!(provider("openrouter").unwrap().name, "OpenRouter");
        assert_eq!(provider_for_model("openrouter/anthropic/claude-3.5-sonnet"), "openrouter");
    }

    /// Live checks. Run with:
    /// `cargo test -p harness-core models::tests::live_ -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_openrouter_lists_many_models() {
        let http = reqwest::Client::new();
        let spec = provider("openrouter").unwrap();
        let models = fetch_live_models(&http, spec).await.expect("OpenRouter /models");
        println!("OpenRouter returned {} models", models.len());
        assert!(models.len() > 50, "OpenRouter should expose its full catalogue");
        assert!(models.iter().any(|m| m.id.contains("deepseek")));
        assert!(models.iter().any(|m| m.id.contains("anthropic")));
    }

    #[tokio::test]
    #[ignore]
    async fn live_deepseek_lists_v41_flash() {
        let http = reqwest::Client::new();
        let spec = provider("deepseek").unwrap();
        let models = fetch_live_models(&http, spec).await.expect("DeepSeek /models");
        println!("DeepSeek returned: {:?}", models.iter().map(|m| &m.id).collect::<Vec<_>>());
        assert!(models.iter().any(|m| m.id.contains("deepseek")));
    }
}
