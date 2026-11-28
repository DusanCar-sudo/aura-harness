//! Provider descriptor table — one entry per provider family.
//!
//! Ported from Aura Code's `src/providers/provider-descriptors.ts`. Every
//! routing decision is derived from this one table:
//!
//!   1. `strip_routing_prefix()` — from `prefixes`
//!   2. `api_env_for()` / `get_api_key_for_model()` — from `api_key_env`
//!   3. `provider_family()` — from `family`
//!   4. `base_url_for()` — from `base_url` / `base_url_env`
//!   5. chat dispatch — from `transport`
//!
//! Order is significant: resellers (OpenCode, FPT, BytePlus) come before the
//! vendors whose bare model names they carry, so a prefixed id reaches the
//! reseller rather than the vendor's own API.

use std::sync::RwLock;

use crate::config::CustomProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TransportClass {
    Anthropic,
    Google,
    OpenAiCompatible,
}

#[derive(Debug, Clone)]
pub struct ProviderDescriptor {
    pub family: &'static str,
    pub display_name: &'static str,
    /// Slash-terminated routing prefixes. These are stripped from the wire id.
    pub prefixes: &'static [&'static str],
    /// Bare model-id prefixes (vendor API names) — select but do not strip.
    pub bare_prefixes: &'static [&'static str],
    pub api_key_env: &'static [&'static str],
    pub transport: TransportClass,
    pub base_url: Option<&'static str>,
    pub base_url_env: &'static [&'static str],
    pub model_strip: bool,
}

pub const ZHIPU_GENERAL_BASE_URL: &str = "https://api.z.ai/api/paas/v4";
pub const ZHIPU_CODING_BASE_URL: &str = "https://api.z.ai/api/coding/paas/v4";
pub const ALIBABA_CODING_BASE_URL: &str = "https://coding-intl.dashscope.aliyuncs.com/v1";
pub const OPENCODE_ZEN_BASE_URL: &str = "https://opencode.ai/zen/v1";
pub const OPENCODE_GO_BASE_URL: &str = "https://opencode.ai/zen/go/v1";
pub const KIMI_PAYG_BASE_URL: &str = "https://api.moonshot.ai/v1";
pub const KIMI_CODING_BASE_URL: &str = "https://api.kimi.com/coding/v1";

static CUSTOM_PROVIDERS: RwLock<Vec<CustomProvider>> = RwLock::new(Vec::new());

/// Replace the runtime custom-provider registry (synced from `Config`).
pub fn set_custom_providers(providers: Vec<CustomProvider>) {
    if let Ok(mut guard) = CUSTOM_PROVIDERS.write() {
        *guard = providers;
    }
}

/// Snapshot the registered custom providers.
pub fn custom_providers() -> Vec<CustomProvider> {
    CUSTOM_PROVIDERS
        .read()
        .map(|g| g.clone())
        .unwrap_or_default()
}

fn custom_for_model(model_id: &str) -> Option<CustomProvider> {
    let lower = model_id.to_lowercase();
    let list = CUSTOM_PROVIDERS.read().ok()?;
    list.iter()
        .find(|p| {
            let prefix = normalized_custom_prefix(p);
            !prefix.is_empty() && lower.starts_with(&prefix.to_lowercase())
        })
        .cloned()
}

fn normalized_custom_prefix(p: &CustomProvider) -> String {
    let raw = if p.prefix.trim().is_empty() {
        format!("{}/", p.id.trim())
    } else {
        let trimmed = p.prefix.trim();
        if trimmed.ends_with('/') {
            trimmed.to_string()
        } else {
            format!("{trimmed}/")
        }
    };
    raw
}

pub const KNOWN_PROVIDERS: &[ProviderDescriptor] = &[
    // ── Resellers first ────────────────────────────────────────────────────
    ProviderDescriptor {
        family: "opencode",
        display_name: "OpenCode Zen / Go",
        prefixes: &["go-anthropic/", "opencode-go/", "opencode/", "zen/"],
        bare_prefixes: &[],
        api_key_env: &["OPENCODE_API_KEY", "OPENCODE_GO_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some(OPENCODE_ZEN_BASE_URL),
        base_url_env: &["OPENCODE_BASE_URL"],
        model_strip: true,
    },
    // FPT and BytePlus resell other vendors' models under their own gateway.
    ProviderDescriptor {
        family: "fpt",
        display_name: "FPT Cloud AI",
        prefixes: &["fpt/"],
        bare_prefixes: &[],
        api_key_env: &["FPT_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://mkp-api.fptcloud.com/v1"),
        base_url_env: &["FPT_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "byteplus",
        display_name: "BytePlus ModelArk",
        prefixes: &["byteplus/"],
        bare_prefixes: &[],
        api_key_env: &["ARK_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://ark.ap-southeast.bytepluses.com/api/v3"),
        base_url_env: &["ARK_BASE_URL"],
        model_strip: true,
    },
    // ── Native transports ──────────────────────────────────────────────────
    ProviderDescriptor {
        family: "anthropic",
        display_name: "Anthropic Claude",
        prefixes: &["anthropic/", "claude/"],
        bare_prefixes: &["claude-"],
        api_key_env: &["ANTHROPIC_API_KEY", "CLAUDE_API_KEY"],
        transport: TransportClass::Anthropic,
        base_url: Some("https://api.anthropic.com"),
        base_url_env: &["ANTHROPIC_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "google",
        display_name: "Google AI Studio",
        prefixes: &["gemini/", "google/"],
        bare_prefixes: &["gemini-"],
        api_key_env: &["GOOGLE_API_KEY", "GEMINI_API_KEY"],
        transport: TransportClass::Google,
        base_url: Some("https://generativelanguage.googleapis.com"),
        base_url_env: &["GOOGLE_BASE_URL", "GEMINI_BASE_URL"],
        model_strip: true,
    },
    // ── OpenAI-compatible vendors ──────────────────────────────────────────
    ProviderDescriptor {
        family: "openai",
        display_name: "OpenAI",
        prefixes: &["openai/"],
        bare_prefixes: &["gpt-", "o1", "o3", "o4", "chatgpt-"],
        api_key_env: &["OPENAI_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.openai.com/v1"),
        base_url_env: &["OPENAI_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "deepseek",
        display_name: "DeepSeek",
        prefixes: &["deepseek/"],
        bare_prefixes: &["deepseek-"],
        api_key_env: &["DEEPSEEK_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.deepseek.com/v1"),
        base_url_env: &["DEEPSEEK_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "xai",
        display_name: "xAI Grok",
        prefixes: &["xai/", "grok/"],
        bare_prefixes: &["grok-"],
        api_key_env: &["XAI_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.x.ai/v1"),
        base_url_env: &["XAI_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "qwen",
        display_name: "Qwen (Alibaba)",
        prefixes: &["qwen/"],
        bare_prefixes: &["qwen-"],
        api_key_env: &["DASHSCOPE_API_KEY", "QWEN_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://dashscope-intl.aliyuncs.com/compatible-mode/v1"),
        base_url_env: &["DASHSCOPE_BASE_URL", "QWEN_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "minimax",
        display_name: "MiniMax",
        prefixes: &["minimax/"],
        bare_prefixes: &["minimax-"],
        api_key_env: &["MINIMAX_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.minimax.io/v1"),
        base_url_env: &["MINIMAX_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "kimi",
        display_name: "Kimi / Moonshot",
        prefixes: &["kimi-coding/", "kimi/"],
        bare_prefixes: &[],
        api_key_env: &["MOONSHOT_API_KEY", "KIMI_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some(KIMI_PAYG_BASE_URL),
        base_url_env: &["MOONSHOT_BASE_URL", "KIMI_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "zhipu",
        display_name: "Zhipu (Z.ai GLM)",
        prefixes: &["zhipu-coding/", "zhipu/"],
        bare_prefixes: &["glm-"],
        api_key_env: &["ZHIPU_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some(ZHIPU_GENERAL_BASE_URL),
        base_url_env: &["ZHIPU_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "xiaomi",
        display_name: "Xiaomi MiMo",
        prefixes: &["xiaomi/", "mimo/"],
        bare_prefixes: &["mimo-"],
        api_key_env: &["XIAOMI_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.xiaomimimo.com/v1"),
        base_url_env: &["XIAOMI_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "groq",
        display_name: "Groq",
        prefixes: &["groq/"],
        bare_prefixes: &[],
        api_key_env: &["GROQ_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.groq.com/openai/v1"),
        base_url_env: &["GROQ_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "nvidia",
        display_name: "NVIDIA NIM",
        prefixes: &["nvidia/"],
        bare_prefixes: &["nvidia-"],
        api_key_env: &["NVIDIA_API_KEY", "NVIDIA_NIM_API_KEY", "NIM_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://integrate.api.nvidia.com/v1"),
        base_url_env: &[],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "huggingface",
        display_name: "Hugging Face",
        prefixes: &["huggingface/"],
        bare_prefixes: &[],
        api_key_env: &["HUGGINGFACE_API_KEY", "HF_TOKEN"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://router.huggingface.co/v1"),
        base_url_env: &["HUGGINGFACE_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "stepfun",
        display_name: "StepFun",
        prefixes: &["stepfun/"],
        bare_prefixes: &[],
        api_key_env: &["STEPFUN_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.stepfun.com/v1"),
        base_url_env: &["STEPFUN_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "tencent",
        display_name: "Tencent TokenHub",
        prefixes: &["tencent/"],
        bare_prefixes: &[],
        api_key_env: &["TENCENT_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://tokenhub.tencentmaas.com/v1"),
        base_url_env: &["TENCENT_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "fireworks",
        display_name: "Fireworks AI",
        prefixes: &["fireworks/"],
        bare_prefixes: &[],
        api_key_env: &["FIREWORKS_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.fireworks.ai/inference/v1"),
        base_url_env: &["FIREWORKS_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "arcee",
        display_name: "Arcee AI",
        prefixes: &["arcee/"],
        bare_prefixes: &[],
        api_key_env: &["ARCEE_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://conductor.arcee.ai/v1"),
        base_url_env: &["ARCEE_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "gmi",
        display_name: "GMI Cloud",
        prefixes: &["gmi/"],
        bare_prefixes: &[],
        api_key_env: &["GMI_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.gmi-serving.com/v1"),
        base_url_env: &["GMI_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "kilocode",
        display_name: "Kilo Code",
        prefixes: &["kilocode/"],
        bare_prefixes: &[],
        api_key_env: &["KILOCODE_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.kilocode.ai/api/openrouter"),
        base_url_env: &["KILOCODE_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "upstage",
        display_name: "Upstage",
        prefixes: &["upstage/"],
        bare_prefixes: &[],
        api_key_env: &["UPSTAGE_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.upstage.ai/v1"),
        base_url_env: &["UPSTAGE_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "alibaba",
        display_name: "Alibaba Coding Plan",
        prefixes: &["alibaba/"],
        bare_prefixes: &[],
        api_key_env: &["ALIBABA_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some(ALIBABA_CODING_BASE_URL),
        base_url_env: &["ALIBABA_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "inception",
        display_name: "Inception",
        prefixes: &["inception/"],
        bare_prefixes: &["mercury-"],
        api_key_env: &["INCEPTION_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.inceptionlabs.ai/v1"),
        base_url_env: &["INCEPTION_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "nebius",
        display_name: "Nebius AI Studio",
        prefixes: &["nebius/"],
        bare_prefixes: &[],
        api_key_env: &["NEBIUS_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.studio.nebius.ai/v1"),
        base_url_env: &["NEBIUS_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "cerebras",
        display_name: "Cerebras",
        prefixes: &["cerebras/"],
        bare_prefixes: &[],
        api_key_env: &["CEREBRAS_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.cerebras.ai/v1"),
        base_url_env: &["CEREBRAS_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "sambanova",
        display_name: "SambaNova",
        prefixes: &["sambanova/"],
        bare_prefixes: &[],
        api_key_env: &["SAMBANOVA_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.sambanova.ai/v1"),
        base_url_env: &["SAMBANOVA_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "mistral",
        display_name: "Mistral AI",
        prefixes: &["mistral/"],
        bare_prefixes: &["mistral-", "codestral-", "pixtral-", "open-mistral-"],
        api_key_env: &["MISTRAL_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.mistral.ai/v1"),
        base_url_env: &["MISTRAL_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "together",
        display_name: "Together AI",
        prefixes: &["together/"],
        bare_prefixes: &[],
        api_key_env: &["TOGETHER_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.together.xyz/v1"),
        base_url_env: &["TOGETHER_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "cohere",
        display_name: "Cohere",
        prefixes: &["cohere/"],
        bare_prefixes: &["command-"],
        api_key_env: &["COHERE_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.cohere.com/v2"),
        base_url_env: &["COHERE_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "openrouter",
        display_name: "OpenRouter",
        prefixes: &["openrouter/"],
        bare_prefixes: &[],
        api_key_env: &["OPENROUTER_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://openrouter.ai/api/v1"),
        base_url_env: &["OPENROUTER_BASE_URL"],
        model_strip: true,
    },
    // ── Local backends ─────────────────────────────────────────────────────
    ProviderDescriptor {
        family: "aura-local",
        display_name: "Aura Local Model",
        prefixes: &["aura-local/"],
        bare_prefixes: &["lfm3-1.7b-aura", "lfm2.5-1.2b-aura", "gemma-archimedes"],
        api_key_env: &[],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("http://127.0.0.1:1234/v1"),
        base_url_env: &["AURA_LOCAL_BASE_URL", "LMSTUDIO_BASE_URL"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "ollama",
        display_name: "Ollama (local)",
        prefixes: &["ollama/", "ollama:"],
        bare_prefixes: &[],
        api_key_env: &[],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("http://localhost:11434/v1"),
        base_url_env: &["OLLAMA_BASE_URL", "OLLAMA_HOST"],
        model_strip: true,
    },
    ProviderDescriptor {
        family: "lmstudio",
        display_name: "LM Studio (local)",
        prefixes: &["lmstudio/", "local/"],
        bare_prefixes: &[],
        api_key_env: &[],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("http://127.0.0.1:1234/v1"),
        base_url_env: &["LMSTUDIO_BASE_URL"],
        model_strip: true,
    },
    // ── Fallback ───────────────────────────────────────────────────────────
    ProviderDescriptor {
        family: "generic",
        display_name: "Generic (OpenAI-compatible)",
        prefixes: &[],
        bare_prefixes: &[],
        api_key_env: &["OPENAI_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.openai.com/v1"),
        base_url_env: &[],
        model_strip: false,
    },
];

fn claims(desc: &ProviderDescriptor, model: &str) -> bool {
    desc.prefixes.iter().any(|p| model.starts_with(p))
        || desc.bare_prefixes.iter().any(|p| model.starts_with(p))
}

pub fn detect_provider_kind(model_id: &str) -> TransportClass {
    if custom_for_model(model_id).is_some() {
        return TransportClass::OpenAiCompatible;
    }
    descriptor_for(model_id)
        .map(|d| d.transport)
        .unwrap_or(TransportClass::OpenAiCompatible)
}

pub fn provider_family(model_id: &str) -> &'static str {
    if let Some(c) = custom_for_model(model_id) {
        // Leak-free static lookup isn't needed for built-ins; for custom
        // providers return a leaked stable id once per unique custom id or
        // match `custom`.
        return intern_custom_id(&c.id);
    }
    descriptor_for(model_id)
        .map(|d| d.family)
        .unwrap_or("generic")
}

fn intern_custom_id(id: &str) -> &'static str {
    static INTERNED: RwLock<Vec<&'static str>> = RwLock::new(Vec::new());
    if let Ok(read) = INTERNED.read() {
        if let Some(&s) = read.iter().find(|&&s| s == id) {
            return s;
        }
    }
    if let Ok(mut write) = INTERNED.write() {
        if let Some(&s) = write.iter().find(|&&s| s == id) {
            return s;
        }
        let leaked: &'static str = Box::leak(id.to_string().into_boxed_str());
        write.push(leaked);
        return leaked;
    }
    "custom"
}

pub fn descriptor_for(model_id: &str) -> Option<&'static ProviderDescriptor> {
    let model = model_id.to_lowercase();
    KNOWN_PROVIDERS.iter().find(|d| claims(d, &model))
}

pub fn api_env_for(model_id: &str) -> Option<&'static str> {
    descriptor_for(model_id)?.api_key_env.first().copied()
}

pub fn get_api_key_for_model(model_id: &str) -> Option<String> {
    if let Some(c) = custom_for_model(model_id) {
        if c.api_key_env.trim().is_empty() {
            return Some("custom".to_string());
        }
        return crate::config::load_api_key_named(c.api_key_env.trim());
    }
    let desc = descriptor_for(model_id)?;
    let lower = model_id.to_lowercase();
    if desc.family == "opencode"
        && (lower.starts_with("go-anthropic/") || lower.starts_with("opencode-go/"))
    {
        for env_name in ["OPENCODE_GO_API_KEY", "OPENCODE_API_KEY"] {
            if let Some(key) = crate::config::load_api_key_named(env_name) {
                return Some(key);
            }
        }
        return None;
    }
    for env_name in desc.api_key_env {
        if let Some(key) = crate::config::load_api_key_named(env_name) {
            return Some(key);
        }
    }
    if desc.api_key_env.is_empty() {
        return Some("local".to_string());
    }
    None
}

/// Default endpoint for a model, honouring `<X>_BASE_URL` overrides (from
/// `keys.json`, env, and `.env`) and providers whose endpoint depends on the
/// model or key plan.
pub fn base_url_for(model_id: &str) -> Option<String> {
    if let Some(c) = custom_for_model(model_id) {
        let base = c.base_url.trim().trim_end_matches('/');
        if !base.is_empty() {
            return Some(base.to_string());
        }
    }
    let desc = descriptor_for(model_id)?;
    let model = model_id.to_lowercase();

    // Zhipu's Coding Plan has its own host and quota.
    if desc.family == "zhipu" && model.starts_with("zhipu-coding/") {
        if let Some(url) = crate::config::load_api_key_named("ZHIPU_CODING_BASE_URL") {
            let url = url.trim().trim_end_matches('/');
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
        return Some(ZHIPU_CODING_BASE_URL.to_string());
    }

    // OpenCode Go subscription gateway vs OpenCode Zen PAYG gateway.
    if desc.family == "opencode"
        && (model.starts_with("go-anthropic/") || model.starts_with("opencode-go/"))
    {
        if let Some(url) = crate::config::load_api_key_named("OPENCODE_GO_BASE_URL") {
            let url = url.trim().trim_end_matches('/');
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
        return Some(OPENCODE_GO_BASE_URL.to_string());
    }

    // Kimi Coding Plan endpoint vs Moonshot PAYG.
    if desc.family == "kimi" && model.starts_with("kimi-coding/") {
        if let Some(url) = crate::config::load_api_key_named("KIMI_CODING_BASE_URL") {
            let url = url.trim().trim_end_matches('/');
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
        return Some(KIMI_CODING_BASE_URL.to_string());
    }

    for env_name in desc.base_url_env {
        if let Some(url) = crate::config::load_api_key_named(env_name) {
            let url = url.trim().trim_end_matches('/');
            if !url.is_empty() {
                // OLLAMA_HOST is a bare host:port; everything else is a URL.
                return Some(if url.starts_with("http") {
                    url.to_string()
                } else {
                    format!("http://{url}")
                });
            }
        }
    }

    // MiMo has two hosts: tp- keys use the Token Plan, sk- keys pay-as-you-go.
    if desc.family == "xiaomi" {
        let key = get_api_key_for_model(model_id).unwrap_or_default();
        return Some(if key.starts_with("tp-") {
            "https://token-plan-sgp.xiaomimimo.com/v1".to_string()
        } else {
            "https://api.xiaomimimo.com/v1".to_string()
        });
    }

    desc.base_url.map(|s| s.trim_end_matches('/').to_string())
}

/// Strip the routing prefix from a model id so the provider receives the wire
/// name it expects. Bare vendor names (`deepseek-flash`, `glm-5.2`) pass
/// through untouched.
pub fn strip_routing_prefix(model_id: &str) -> &str {
    let model = model_id.to_lowercase();
    if let Some(c) = custom_for_model(model_id) {
        let prefix = normalized_custom_prefix(&c).to_lowercase();
        if model.starts_with(&prefix) && model_id.len() >= prefix.len() {
            return &model_id[prefix.len()..];
        }
    }
    // Ollama's older `name:tag` shorthand selects the provider but is not part
    // of the model id the server expects.
    if let Some(rest) = model_id.strip_prefix("ollama:") {
        return rest;
    }
    // OpenRouter meta-routers (`openrouter/free`, `openrouter/auto`) keep
    // their `openrouter/` namespace on the wire unless double-prefixed.
    if model.starts_with("openrouter/") {
        let rest = &model_id["openrouter/".len()..];
        if !rest.contains('/') && !rest.is_empty() {
            return model_id;
        }
        return rest;
    }
    let mut prefixes: Vec<&str> = KNOWN_PROVIDERS
        .iter()
        .flat_map(|d| d.prefixes.iter().copied())
        .filter(|p| p.ends_with('/'))
        .collect();
    prefixes.sort_by_key(|p| std::cmp::Reverse(p.len()));
    for prefix in prefixes {
        if model.starts_with(prefix) {
            return &model_id[prefix.len()..];
        }
    }
    model_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_and_prefixes_route() {
        assert_eq!(provider_family("openrouter/anthropic/claude-3.5-sonnet"), "openrouter");
        assert_eq!(provider_family("fpt/DeepSeek-V4-Flash"), "fpt");
        assert_eq!(provider_family("byteplus/deepseek-v4-flash-ga-260731"), "byteplus");
        assert_eq!(provider_family("go-anthropic/claude-sonnet-5"), "opencode");
        assert_eq!(provider_family("zhipu-coding/glm-5.2"), "zhipu");
        assert_eq!(provider_family("deepseek-flash"), "deepseek");
        assert_eq!(provider_family("mimo-v2.5-pro"), "xiaomi");
        assert_eq!(provider_family("glm-5.2"), "zhipu");
        assert_eq!(provider_family("gpt-4o"), "openai");
        assert_eq!(provider_family("o3"), "openai");
        assert_eq!(provider_family("claude-sonnet-4"), "anthropic");
        assert_eq!(provider_family("gemini-2.5-pro"), "google");
    }

    #[test]
    fn wire_ids_lose_only_the_routing_prefix() {
        assert_eq!(strip_routing_prefix("openrouter/anthropic/claude-3.5-sonnet"), "anthropic/claude-3.5-sonnet");
        assert_eq!(strip_routing_prefix("deepseek/deepseek-chat"), "deepseek-chat");
        assert_eq!(strip_routing_prefix("deepseek-flash"), "deepseek-flash");
        assert_eq!(strip_routing_prefix("fpt/DeepSeek-V4-Flash"), "DeepSeek-V4-Flash");
        assert_eq!(strip_routing_prefix("zhipu-coding/glm-5.2"), "glm-5.2");
        assert_eq!(strip_routing_prefix("go-anthropic/claude-opus-5"), "claude-opus-5");
        assert_eq!(strip_routing_prefix("ollama:llama3.1"), "llama3.1");
        assert_eq!(strip_routing_prefix("openrouter/free"), "openrouter/free");
        assert_eq!(strip_routing_prefix("openrouter/auto"), "openrouter/auto");
        assert_eq!(strip_routing_prefix("openrouter/openrouter/free"), "openrouter/free");
    }

    #[test]
    fn transports_are_selected() {
        assert_eq!(detect_provider_kind("claude-sonnet-4"), TransportClass::Anthropic);
        assert_eq!(detect_provider_kind("gemini-2.5-pro"), TransportClass::Google);
        assert_eq!(detect_provider_kind("claude/sonnet"), TransportClass::Anthropic);
        assert_eq!(detect_provider_kind("gpt-4o"), TransportClass::OpenAiCompatible);
        assert_eq!(detect_provider_kind("openrouter/x"), TransportClass::OpenAiCompatible);
        assert_eq!(detect_provider_kind("aura-local/lfm3-1.7b-aura-grounded"), TransportClass::OpenAiCompatible);
    }

    #[test]
    fn local_models_resolve_keyless_and_default_1234() {
        let _lock = crate::env_lock::guard();
        let dir = std::env::temp_dir().join(format!("aura-local-{}", uuid::Uuid::new_v4()));
        std::env::set_var("AURA_HARNESS_HOME", &dir);
        std::env::set_var("AURA_HOME", dir.join("aura"));
        assert_eq!(provider_family("aura-local/lfm3-1.7b-aura-grounded"), "aura-local");
        assert_eq!(provider_family("lmstudio/liquid/lfm2.5-1.2b"), "lmstudio");
        assert_eq!(strip_routing_prefix("aura-local/lfm3-1.7b-aura-grounded"), "lfm3-1.7b-aura-grounded");
        assert_eq!(strip_routing_prefix("lmstudio/liquid/lfm2.5-1.2b"), "liquid/lfm2.5-1.2b");
        assert!(get_api_key_for_model("aura-local/lfm3-1.7b-aura-grounded").is_some());
        assert!(get_api_key_for_model("lmstudio/liquid/lfm2.5-1.2b").is_some());
        assert_eq!(
            base_url_for("aura-local/lfm3-1.7b-aura-grounded").as_deref(),
            Some("http://127.0.0.1:1234/v1")
        );
        std::env::remove_var("AURA_HARNESS_HOME");
        std::env::remove_var("AURA_HOME");
    }

    #[test]
    fn zhipu_coding_endpoint_is_distinct() {
        let _lock = crate::env_lock::guard();
        let dir = std::env::temp_dir().join(format!("aura-zhipu-{}", uuid::Uuid::new_v4()));
        std::env::set_var("AURA_HARNESS_HOME", &dir);
        std::env::set_var("AURA_HOME", dir.join("aura"));
        assert_eq!(
            base_url_for("zhipu-coding/glm-5.2").as_deref(),
            Some(ZHIPU_CODING_BASE_URL)
        );
        assert_eq!(
            base_url_for("glm-5.2").as_deref(),
            Some(ZHIPU_GENERAL_BASE_URL)
        );
        std::env::remove_var("AURA_HARNESS_HOME");
        std::env::remove_var("AURA_HOME");
    }
}
