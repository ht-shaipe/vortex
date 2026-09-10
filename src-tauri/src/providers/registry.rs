use crate::providers::types::ProviderDef;
use std::collections::HashMap;

pub struct ProviderRegistry {
    providers: HashMap<String, ProviderDef>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        let mut providers = HashMap::new();

        let defs = vec![
            ProviderDef {
                id: "openai".into(), alias: "oa".into(), name: "OpenAI".into(),
                icon: "openai".into(), color: "#412991".into(),
                service_kinds: vec!["llm".into(), "embedding".into(), "image".into()],
                no_auth: false, has_free: false, free_note: None, auth_hint: Some("API key from platform.openai.com".into()),
                base_url: "https://api.openai.com".into(), chat_path: "/v1/chat/completions".into(),
                models_path: "/v1/models".into(), api_format: "openai".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "anthropic".into(), alias: "an".into(), name: "Anthropic".into(),
                icon: "claude".into(), color: "#D97757".into(),
                service_kinds: vec!["llm".into()],
                no_auth: false, has_free: false, free_note: None, auth_hint: Some("API key from console.anthropic.com".into()),
                base_url: "https://api.anthropic.com".into(), chat_path: "/v1/messages".into(),
                models_path: "/v1/models".into(), api_format: "anthropic".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "gemini".into(), alias: "gm".into(), name: "Google Gemini".into(),
                icon: "gemini".into(), color: "#4285F4".into(),
                service_kinds: vec!["llm".into(), "embedding".into()],
                no_auth: false, has_free: true, free_note: Some("Free tier available".into()), auth_hint: Some("API key from aistudio.google.com".into()),
                base_url: "https://generativelanguage.googleapis.com".into(), chat_path: "/v1beta/models/{model}:generateContent".into(),
                models_path: "/v1beta/models".into(), api_format: "gemini".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "deepseek".into(), alias: "ds".into(), name: "DeepSeek".into(),
                icon: "deepseek".into(), color: "#4D6BFE".into(),
                service_kinds: vec!["llm".into()],
                no_auth: false, has_free: true, free_note: Some("Free tier with limits".into()), auth_hint: Some("API key from platform.deepseek.com".into()),
                base_url: "https://api.deepseek.com".into(), chat_path: "/v1/chat/completions".into(),
                models_path: "/v1/models".into(), api_format: "openai".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "groq".into(), alias: "gq".into(), name: "Groq".into(),
                icon: "groq".into(), color: "#F55036".into(),
                service_kinds: vec!["llm".into()],
                no_auth: false, has_free: true, free_note: Some("Generous free tier".into()), auth_hint: Some("API key from console.groq.com".into()),
                base_url: "https://api.groq.com/openai".into(), chat_path: "/v1/chat/completions".into(),
                models_path: "/v1/models".into(), api_format: "openai".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "xai".into(), alias: "xa".into(), name: "xAI (Grok)".into(),
                icon: "grok".into(), color: "#000000".into(),
                service_kinds: vec!["llm".into()],
                no_auth: false, has_free: false, free_note: None, auth_hint: Some("API key from x.ai".into()),
                base_url: "https://api.x.ai".into(), chat_path: "/v1/chat/completions".into(),
                models_path: "/v1/models".into(), api_format: "openai".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "mistral".into(), alias: "ml".into(), name: "Mistral".into(),
                icon: "mistral".into(), color: "#F70000".into(),
                service_kinds: vec!["llm".into(), "embedding".into()],
                no_auth: false, has_free: false, free_note: None, auth_hint: Some("API key from console.mistral.ai".into()),
                base_url: "https://api.mistral.ai".into(), chat_path: "/v1/chat/completions".into(),
                models_path: "/v1/models".into(), api_format: "openai".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "openrouter".into(), alias: "or".into(), name: "OpenRouter".into(),
                icon: "openrouter".into(), color: "#6366F1".into(),
                service_kinds: vec!["llm".into(), "image".into()],
                no_auth: false, has_free: true, free_note: Some("Some free models".into()), auth_hint: Some("API key from openrouter.ai".into()),
                base_url: "https://openrouter.ai/api".into(), chat_path: "/v1/chat/completions".into(),
                models_path: "/v1/models".into(), api_format: "openai".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "cohere".into(), alias: "ch".into(), name: "Cohere".into(),
                icon: "cohere".into(), color: "#39594D".into(),
                service_kinds: vec!["llm".into(), "embedding".into(), "rerank".into()],
                no_auth: false, has_free: true, free_note: Some("Free trial tier".into()), auth_hint: Some("API key from dashboard.cohere.com".into()),
                base_url: "https://api.cohere.ai".into(), chat_path: "/v2/chat".into(),
                models_path: "/v2/models".into(), api_format: "cohere".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "together".into(), alias: "tg".into(), name: "Together AI".into(),
                icon: "together".into(), color: "#00D1FF".into(),
                service_kinds: vec!["llm".into(), "image".into()],
                no_auth: false, has_free: true, free_note: Some("$5 free credits".into()), auth_hint: Some("API key from api.together.xyz".into()),
                base_url: "https://api.together.xyz".into(), chat_path: "/v1/chat/completions".into(),
                models_path: "/v1/models".into(), api_format: "openai".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "fireworks".into(), alias: "fw".into(), name: "Fireworks AI".into(),
                icon: "fireworks".into(), color: "#FF6B35".into(),
                service_kinds: vec!["llm".into(), "image".into()],
                no_auth: false, has_free: false, free_note: None, auth_hint: Some("API key from app.fireworks.ai".into()),
                base_url: "https://api.fireworks.ai/inference".into(), chat_path: "/v1/chat/completions".into(),
                models_path: "/v1/models".into(), api_format: "openai".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "cerebras".into(), alias: "cb".into(), name: "Cerebras".into(),
                icon: "cerebras".into(), color: "#7C3AED".into(),
                service_kinds: vec!["llm".into()],
                no_auth: false, has_free: true, free_note: Some("1M tokens/day free".into()), auth_hint: Some("API key from cloud.cerebras.ai".into()),
                base_url: "https://api.cerebras.ai".into(), chat_path: "/v1/chat/completions".into(),
                models_path: "/v1/models".into(), api_format: "openai".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "nvidia".into(), alias: "nv".into(), name: "NVIDIA NIM".into(),
                icon: "nvidia".into(), color: "#76B900".into(),
                service_kinds: vec!["llm".into(), "embedding".into()],
                no_auth: false, has_free: true, free_note: Some("~40 RPM free".into()), auth_hint: Some("API key from build.nvidia.com".into()),
                base_url: "https://integrate.api.nvidia.com".into(), chat_path: "/v1/chat/completions".into(),
                models_path: "/v1/models".into(), api_format: "openai".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "cloudflare".into(), alias: "cf".into(), name: "Cloudflare AI".into(),
                icon: "cloudflare".into(), color: "#F48120".into(),
                service_kinds: vec!["llm".into(), "embedding".into(), "image".into()],
                no_auth: false, has_free: true, free_note: Some("10K neurons/day free".into()), auth_hint: Some("API token from dash.cloudflare.com".into()),
                base_url: "https://api.cloudflare.com/client/v4/accounts/{account_id}/ai".into(),
                chat_path: "/v1/chat/completions".into(), models_path: "/models/search".into(),
                api_format: "cloudflare".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "ollama".into(), alias: "ol".into(), name: "Ollama (Local)".into(),
                icon: "ollama".into(), color: "#000000".into(),
                service_kinds: vec!["llm".into(), "embedding".into()],
                no_auth: true, has_free: true, free_note: Some("Local, unlimited".into()), auth_hint: None,
                base_url: "http://localhost:11434".into(), chat_path: "/v1/chat/completions".into(),
                models_path: "/v1/models".into(), api_format: "openai".into(), auth_type: "noauth".into(),
            },
            ProviderDef {
                id: "siliconflow".into(), alias: "sf".into(), name: "SiliconFlow".into(),
                icon: "siliconflow".into(), color: "#7C3AED".into(),
                service_kinds: vec!["llm".into(), "image".into()],
                no_auth: false, has_free: true, free_note: Some("Free models available".into()), auth_hint: Some("API key from siliconflow.cn".into()),
                base_url: "https://api.siliconflow.cn".into(), chat_path: "/v1/chat/completions".into(),
                models_path: "/v1/models".into(), api_format: "openai".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "huggingface".into(), alias: "hf".into(), name: "HuggingFace".into(),
                icon: "huggingface".into(), color: "#FFD21E".into(),
                service_kinds: vec!["llm".into(), "embedding".into()],
                no_auth: false, has_free: true, free_note: Some("Free Inference API".into()), auth_hint: Some("Access token from huggingface.co".into()),
                base_url: "https://api-inference.huggingface.co".into(), chat_path: "/v1/chat/completions".into(),
                models_path: "/v1/models".into(), api_format: "openai".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "pollinations".into(), alias: "pl".into(), name: "Pollinations".into(),
                icon: "pollinations".into(), color: "#00B894".into(),
                service_kinds: vec!["llm".into()],
                no_auth: true, has_free: true, free_note: Some("Free, no key needed".into()), auth_hint: None,
                base_url: "https://text.pollinations.ai".into(), chat_path: "/openai".into(),
                models_path: "/v1/models".into(), api_format: "openai".into(), auth_type: "noauth".into(),
            },
            ProviderDef {
                id: "perplexity".into(), alias: "pp".into(), name: "Perplexity".into(),
                icon: "perplexity".into(), color: "#20B8CD".into(),
                service_kinds: vec!["llm".into(), "webSearch".into()],
                no_auth: false, has_free: false, free_note: None, auth_hint: Some("API key from perplexity.ai".into()),
                base_url: "https://api.perplexity.ai".into(), chat_path: "/chat/completions".into(),
                models_path: "/models".into(), api_format: "openai".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "qwen".into(), alias: "qw".into(), name: "Qwen (通义千问)".into(),
                icon: "qwen".into(), color: "#6157FF".into(),
                service_kinds: vec!["llm".into(), "embedding".into()],
                no_auth: false, has_free: true, free_note: Some("Free tier available".into()), auth_hint: Some("API key from dashscope.aliyun.com".into()),
                base_url: "https://dashscope.aliyuncs.com/compatible-mode".into(), chat_path: "/v1/chat/completions".into(),
                models_path: "/v1/models".into(), api_format: "openai".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "minimax".into(), alias: "mm".into(), name: "MiniMax".into(),
                icon: "minimax".into(), color: "#0066FF".into(),
                service_kinds: vec!["llm".into()],
                no_auth: false, has_free: false, free_note: None, auth_hint: Some("API key from minimax.chat".into()),
                base_url: "https://api.minimax.chat".into(), chat_path: "/v1/text/chatcompletion_v2".into(),
                models_path: "/v1/models".into(), api_format: "openai".into(), auth_type: "apikey".into(),
            },
            ProviderDef {
                id: "custom-openai".into(), alias: "cx".into(), name: "Custom OpenAI-Compatible".into(),
                icon: "api".into(), color: "#607D8B".into(),
                service_kinds: vec!["llm".into(), "embedding".into()],
                no_auth: false, has_free: false, free_note: None, auth_hint: Some("Set base URL and API key".into()),
                base_url: "https://your-api-endpoint.com".into(), chat_path: "/v1/chat/completions".into(),
                models_path: "/v1/models".into(), api_format: "openai".into(), auth_type: "apikey".into(),
            },
        ];

        for def in defs {
            providers.insert(def.id.clone(), def);
        }

        Self { providers }
    }

    pub fn get(&self, id: &str) -> Option<&ProviderDef> {
        self.providers.get(id)
    }

    pub fn list(&self) -> Vec<&ProviderDef> {
        let mut list: Vec<_> = self.providers.values().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    pub fn resolve_model_provider(&self, model_str: &str) -> Option<(String, &ProviderDef, String)> {
        if let Some((provider_id, model)) = model_str.split_once('/') {
            if let Some(def) = self.providers.get(provider_id) {
                return Some((provider_id.to_string(), def, model.to_string()));
            }
        }

        let model_lower = model_str.to_lowercase();
        for (id, def) in &self.providers {
            if model_lower.starts_with(&format!("{}-", def.alias)) || model_lower.starts_with(&format!("{}-", id)) {
                let model = model_str.splitn(2, '-').nth(1).unwrap_or(model_str).to_string();
                return Some((id.clone(), def, model));
            }
        }

        None
    }
}
