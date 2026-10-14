//! Provider descriptor table. One entry per provider family, five views derived:
//! 1. strip_routing_prefix()
//! 2. api_key_env_for_model()
//! 3. model_provider_family()
//! 4. KNOWN base urls reverse map
//! 5. create_provider() routing

// (using crate::config::load_api_key_named directly, not imported)

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
    pub prefixes: &'static [&'static str],
    pub bare_prefixes: &'static [&'static str],
    pub api_key_env: &'static [&'static str],
    pub transport: TransportClass,
    pub base_url: Option<&'static str>,
    pub base_url_env: &'static [&'static str],
    pub model_strip: bool,
}

pub const KNOWN_PROVIDERS: &[ProviderDescriptor] = &[
    // ── Anthropic ──────────────────────────────────────────────────────
    ProviderDescriptor {
        family: "anthropic",
        display_name: "Anthropic Claude",
        prefixes: &["claude-"],
        bare_prefixes: &["claude-"],
        api_key_env: &["ANTHROPIC_API_KEY", "CLAUDE_API_KEY"],
        transport: TransportClass::Anthropic,
        base_url: Some("https://api.anthropic.com"),
        base_url_env: &["ANTHROPIC_BASE_URL"],
        model_strip: false,
    },
    // ── OpenAI ─────────────────────────────────────────────────────────
    ProviderDescriptor {
        family: "openai",
        display_name: "OpenAI",
        prefixes: &["openai/"],
        bare_prefixes: &["gpt-", "o1-", "o3-", "o4-", "chatgpt-"],
        api_key_env: &["OPENAI_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.openai.com/v1"),
        base_url_env: &["OPENAI_BASE_URL"],
        model_strip: false,
    },
    // ── Google ──────────────────────────────────────────────────────────
    ProviderDescriptor {
        family: "google",
        display_name: "Google Gemini",
        prefixes: &["gemini/", "google/"],
        bare_prefixes: &["gemini-"],
        api_key_env: &["GOOGLE_API_KEY", "GEMINI_API_KEY"],
        transport: TransportClass::Google,
        base_url: Some("https://generativelanguage.googleapis.com"),
        base_url_env: &["GOOGLE_BASE_URL", "GEMINI_BASE_URL"],
        model_strip: false,
    },
    // ── DeepSeek ────────────────────────────────────────────────────────
    ProviderDescriptor {
        family: "deepseek",
        display_name: "DeepSeek",
        prefixes: &["deepseek/", "deepseek-"],
        bare_prefixes: &["deepseek-"],
        api_key_env: &["DEEPSEEK_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.deepseek.com/v1"),
        base_url_env: &["DEEPSEEK_BASE_URL"],
        model_strip: false,
    },
    // ── xAI (Grok) ──────────────────────────────────────────────────────
    ProviderDescriptor {
        family: "xai",
        display_name: "xAI Grok",
        prefixes: &["xai/", "grok/", "grok-"],
        bare_prefixes: &["grok-"],
        api_key_env: &["XAI_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.x.ai/v1"),
        base_url_env: &["XAI_BASE_URL"],
        model_strip: false,
    },
    // ── OpenRouter ──────────────────────────────────────────────────────
    ProviderDescriptor {
        family: "openrouter",
        display_name: "OpenRouter",
        prefixes: &["openrouter/"],
        bare_prefixes: &[],
        api_key_env: &["OPENROUTER_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://openrouter.ai/api/v1"),
        base_url_env: &["OPENROUTER_BASE_URL"],
        model_strip: false,
    },
    // ── Local (Ollama) ──────────────────────────────────────────────────
    ProviderDescriptor {
        family: "ollama",
        display_name: "Ollama (local)",
        prefixes: &["ollama/"],
        bare_prefixes: &["ollama:"],
        api_key_env: &[],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("http://localhost:11434/v1"),
        base_url_env: &["OLLAMA_HOST", "OLLAMA_BASE_URL"],
        model_strip: false,
    },
    // ── Local (LM Studio) ───────────────────────────────────────────────
    ProviderDescriptor {
        family: "lmstudio",
        display_name: "LM Studio (local)",
        prefixes: &["lmstudio/"],
        bare_prefixes: &[],
        api_key_env: &[],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("http://localhost:1234/v1"),
        base_url_env: &["LMSTUDIO_BASE_URL"],
        model_strip: false,
    },
    // ── Qwen (Alibaba Cloud) ───────────────────────────────────────────
    ProviderDescriptor {
        family: "qwen",
        display_name: "Qwen (Alibaba)",
        prefixes: &["qwen/", "qwen-"],
        bare_prefixes: &["qwen-"],
        api_key_env: &["QWEN_API_KEY", "DASHSCOPE_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://chat.qwen.ai/api"),
        base_url_env: &["QWEN_BASE_URL"],
        model_strip: false,
    },
    // ── Groq ─────────────────────────────────────────────────────────────
    ProviderDescriptor {
        family: "groq",
        display_name: "Groq",
        prefixes: &["groq/"],
        bare_prefixes: &[],
        api_key_env: &["GROQ_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.groq.com/openai/v1"),
        base_url_env: &[],
        model_strip: false,
    },
    // ── Mistral ──────────────────────────────────────────────────────────
    ProviderDescriptor {
        family: "mistral",
        display_name: "Mistral AI",
        prefixes: &["mistral/", "mistral-"],
        bare_prefixes: &["mistral-", "codestral-"],
        api_key_env: &["MISTRAL_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.mistral.ai/v1"),
        base_url_env: &[],
        model_strip: false,
    },
    // ── Cerebras ─────────────────────────────────────────────────────────
    ProviderDescriptor {
        family: "cerebras",
        display_name: "Cerebras",
        prefixes: &["cerebras/"],
        bare_prefixes: &[],
        api_key_env: &["CEREBRAS_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.cerebras.ai/v1"),
        base_url_env: &[],
        model_strip: false,
    },
    // ── NVIDIA NIM ───────────────────────────────────────────────────────
    ProviderDescriptor {
        family: "nvidia",
        display_name: "NVIDIA NIM",
        prefixes: &["nvidia/", "nvidia-"],
        bare_prefixes: &["nvidia-"],
        api_key_env: &["NVIDIA_API_KEY", "NIM_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://integrate.api.nvidia.com/v1"),
        base_url_env: &[],
        model_strip: false,
    },
    // ── MiniMax ──────────────────────────────────────────────────────────
    ProviderDescriptor {
        family: "minimax",
        display_name: "MiniMax",
        prefixes: &["minimax/", "minimax-"],
        bare_prefixes: &["minimax-"],
        api_key_env: &["MINIMAX_API_KEY"],
        transport: TransportClass::OpenAiCompatible,
        base_url: Some("https://api.minimaxi.chat/v1"),
        base_url_env: &[],
        model_strip: false,
    },
    // ── Fallback: generic OpenAI-compatible ─────────────────────────────
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

pub fn detect_provider_kind(model_id: &str) -> TransportClass {
    let m = model_id.to_lowercase();
    for desc in KNOWN_PROVIDERS {
        for bare in desc.bare_prefixes {
            if m.starts_with(bare) { return desc.transport; }
        }
        for p in desc.prefixes {
            if m.starts_with(p) { return desc.transport; }
        }
    }
    TransportClass::OpenAiCompatible
}

pub fn provider_family(model_id: &str) -> &'static str {
    let m = model_id.to_lowercase();
    for desc in KNOWN_PROVIDERS {
        for bare in desc.bare_prefixes {
            if m.starts_with(bare) { return desc.family; }
        }
        for p in desc.prefixes {
            if m.starts_with(p) { return desc.family; }
        }
    }
    "generic"
}

pub fn descriptor_for(model_id: &str) -> Option<&'static ProviderDescriptor> {
    let m = model_id.to_lowercase();
    for desc in KNOWN_PROVIDERS {
        for bare in desc.bare_prefixes {
            if m.starts_with(bare) { return Some(desc); }
        }
        for p in desc.prefixes {
            if m.starts_with(p) { return Some(desc); }
        }
    }
    // Fallback to generic
    KNOWN_PROVIDERS.last()
}

pub fn api_env_for(model_id: &str) -> Option<&'static str> {
    descriptor_for(model_id)?.api_key_env.first().copied()
}

pub fn get_api_key_for_model(model_id: &str) -> Option<String> {
    let desc = descriptor_for(model_id)?;
    for env_name in desc.api_key_env {
        if let Some(key) = crate::config::load_api_key_named(env_name) {
            return Some(key);
        }
    }
    None
}

pub fn base_url_for(model_id: &str) -> Option<String> {
    let desc = descriptor_for(model_id)?;
    // Check env vars directly (env override wins over default)
    for env_name in desc.base_url_env {
        if let Ok(url) = std::env::var(env_name) {
            if !url.trim().is_empty() { return Some(url.trim().to_string()); }
        }
    }
    desc.base_url.map(|s| s.to_string())
}

pub fn strip_routing_prefix(model_id: &str) -> &str {
    let m = model_id.to_lowercase();
    for desc in KNOWN_PROVIDERS {
        for p in desc.prefixes {
            if !p.ends_with('/') { continue; }
            if m.starts_with(p) {
                return &model_id[p.len()..];
            }
        }
    }
    // Special: strip prefixes like "deepseek/" (already handled) but allow "deepseek/" for re-sellers
    model_id
}