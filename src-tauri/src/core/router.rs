//! Model-agnostic router: local Ollama first, then free cloud tiers.
//! Never requires a paid API. With nothing configured it returns a
//! structured onboarding message instead of a raw error. Identical
//! requests are served from a short-lived cache, and providers that
//! rate-limit get skipped until their cooldown passes (§2 free-tier
//! hygiene) — the local model is exempt from both penalties.

use crate::core::reliability::{cache_key, CooldownTracker, ResponseCache};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CACHE_CAPACITY: usize = 64;
const CACHE_TTL: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatReply {
    pub content: String,
    pub provider: String,
    pub model: String,
    /// True when served from the response cache instead of a fresh call.
    pub cached: bool,
}

/// Provider-call failure with enough structure to drive backoff decisions.
#[derive(Debug)]
enum CallError {
    /// HTTP status plus optional Retry-After seconds from the response.
    Http {
        status: u16,
        retry_after: Option<u64>,
        detail: String,
    },
    Other(String),
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::Http { status, detail, .. } => write!(f, "http {status}: {detail}"),
            CallError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    /// Nothing is configured or reachable. Payload is user-facing onboarding text.
    #[error("{0}")]
    NoProvider(String),
    #[error("All configured providers failed. {0}")]
    AllProvidersFailed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderId {
    Ollama,
    Groq,
    OpenRouter,
}

impl ProviderId {
    pub fn name(self) -> &'static str {
        match self {
            ProviderId::Ollama => "ollama",
            ProviderId::Groq => "groq",
            ProviderId::OpenRouter => "openrouter",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RouterConfig {
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub groq_api_key: Option<String>,
    pub groq_model: String,
    pub openrouter_api_key: Option<String>,
    pub openrouter_model: String,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            ollama_base_url: "http://localhost:11434".into(),
            ollama_model: "llama3.2".into(),
            groq_api_key: None,
            groq_model: "llama-3.3-70b-versatile".into(),
            openrouter_api_key: None,
            openrouter_model: "meta-llama/llama-3.3-70b-instruct:free".into(),
        }
    }
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl RouterConfig {
    pub fn from_env() -> Self {
        let d = Self::default();
        Self {
            ollama_base_url: env_nonempty("OLLAMA_BASE_URL").unwrap_or(d.ollama_base_url),
            ollama_model: env_nonempty("OLLAMA_MODEL").unwrap_or(d.ollama_model),
            groq_api_key: env_nonempty("GROQ_API_KEY"),
            groq_model: env_nonempty("GROQ_MODEL").unwrap_or(d.groq_model),
            openrouter_api_key: env_nonempty("OPENROUTER_API_KEY"),
            openrouter_model: env_nonempty("OPENROUTER_MODEL").unwrap_or(d.openrouter_model),
        }
    }
}

/// Shown when no local model is reachable and no free cloud key is set.
pub fn onboarding_message() -> String {
    "I don't have a model to think with yet — everything here is free, pick either option:\n\n\
     1. Local (recommended, private, unlimited): install Ollama from https://ollama.com, \
     then run `ollama pull llama3.2` and send your message again.\n\
     2. Free cloud tier: copy `.env.example` to `.env` and add a free key — \
     GROQ_API_KEY (console.groq.com) or OPENROUTER_API_KEY (openrouter.ai) — then restart the app."
        .to_string()
}

fn ollama_body(model: &str, messages: &[ChatMessage]) -> serde_json::Value {
    serde_json::json!({ "model": model, "messages": messages, "stream": false })
}

fn openai_body(model: &str, messages: &[ChatMessage]) -> serde_json::Value {
    serde_json::json!({ "model": model, "messages": messages })
}

fn extract_ollama_content(v: &serde_json::Value) -> Option<String> {
    v["message"]["content"].as_str().map(str::to_string)
}

fn extract_openai_content(v: &serde_json::Value) -> Option<String> {
    v["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
}

pub struct Router {
    config: RouterConfig,
    client: reqwest::Client,
    cache: Mutex<ResponseCache<ChatReply>>,
    cooldowns: Mutex<CooldownTracker>,
}

impl Router {
    pub fn new(config: RouterConfig) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(4))
            .build()
            .expect("failed to build http client");
        Self {
            config,
            client,
            cache: Mutex::new(ResponseCache::new(CACHE_CAPACITY, CACHE_TTL)),
            cooldowns: Mutex::new(CooldownTracker::new()),
        }
    }

    pub fn config(&self) -> &RouterConfig {
        &self.config
    }

    /// Priority order for this request: local Ollama always first,
    /// cloud providers only when a key is present.
    pub fn provider_plan(&self) -> Vec<ProviderId> {
        let mut plan = vec![ProviderId::Ollama];
        if self.config.groq_api_key.is_some() {
            plan.push(ProviderId::Groq);
        }
        if self.config.openrouter_api_key.is_some() {
            plan.push(ProviderId::OpenRouter);
        }
        plan
    }

    pub fn has_cloud_fallback(&self) -> bool {
        self.provider_plan().len() > 1
    }

    pub async fn ollama_reachable(&self) -> bool {
        let url = format!(
            "{}/api/tags",
            self.config.ollama_base_url.trim_end_matches('/')
        );
        match self
            .client
            .get(&url)
            .timeout(Duration::from_secs(3))
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    pub async fn chat(&self, messages: &[ChatMessage]) -> Result<ChatReply, RouterError> {
        let key = cache_key(&serde_json::to_string(messages).unwrap_or_default());
        if let Some(mut hit) = self
            .cache
            .lock()
            .ok()
            .and_then(|mut cache| cache.get(key, Instant::now()))
        {
            hit.cached = true;
            return Ok(hit);
        }

        let mut failures: Vec<String> = Vec::new();
        for provider in self.provider_plan() {
            // Cloud providers on rate-limit cooldown get skipped; the local
            // model is unlimited and never penalized.
            if provider != ProviderId::Ollama {
                let cooling = self
                    .cooldowns
                    .lock()
                    .map(|c| !c.available(provider.name(), Instant::now()))
                    .unwrap_or(false);
                if cooling {
                    failures.push(format!("{}: on rate-limit cooldown", provider.name()));
                    continue;
                }
            }
            let result = match provider {
                ProviderId::Ollama => self.chat_ollama(messages).await,
                ProviderId::Groq => {
                    self.chat_openai_compatible(
                        "https://api.groq.com/openai/v1/chat/completions",
                        self.config.groq_api_key.as_deref().unwrap_or_default(),
                        &self.config.groq_model,
                        ProviderId::Groq,
                        messages,
                    )
                    .await
                }
                ProviderId::OpenRouter => {
                    self.chat_openai_compatible(
                        "https://openrouter.ai/api/v1/chat/completions",
                        self.config
                            .openrouter_api_key
                            .as_deref()
                            .unwrap_or_default(),
                        &self.config.openrouter_model,
                        ProviderId::OpenRouter,
                        messages,
                    )
                    .await
                }
            };
            match result {
                Ok(reply) => {
                    if let Ok(mut cooldowns) = self.cooldowns.lock() {
                        cooldowns.reset(provider.name());
                    }
                    if let Ok(mut cache) = self.cache.lock() {
                        cache.put(key, reply.clone(), Instant::now());
                    }
                    return Ok(reply);
                }
                Err(e) => {
                    if provider != ProviderId::Ollama {
                        if let CallError::Http {
                            status: 429 | 503,
                            retry_after,
                            ..
                        } = e
                        {
                            if let Ok(mut cooldowns) = self.cooldowns.lock() {
                                cooldowns.penalize(
                                    provider.name(),
                                    retry_after.map(Duration::from_secs),
                                    Instant::now(),
                                );
                            }
                        }
                    }
                    failures.push(format!("{}: {e}", provider.name()));
                }
            }
        }
        if self.has_cloud_fallback() {
            Err(RouterError::AllProvidersFailed(failures.join(" | ")))
        } else {
            // Only unreachable local Ollama was in the plan — this is a
            // fresh install, not a failure. Explain how to get running.
            Err(RouterError::NoProvider(onboarding_message()))
        }
    }

    async fn chat_ollama(&self, messages: &[ChatMessage]) -> Result<ChatReply, CallError> {
        let url = format!(
            "{}/api/chat",
            self.config.ollama_base_url.trim_end_matches('/')
        );
        let resp = self
            .client
            .post(&url)
            .timeout(Duration::from_secs(180))
            .json(&ollama_body(&self.config.ollama_model, messages))
            .send()
            .await
            .map_err(|e| CallError::Other(format!("request failed: {e}")))?;
        let status = resp.status();
        let retry_after = parse_retry_after(&resp);
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CallError::Other(format!("invalid response: {e}")))?;
        if !status.is_success() {
            return Err(CallError::Http {
                status: status.as_u16(),
                retry_after,
                detail: body["error"].to_string(),
            });
        }
        let content = extract_ollama_content(&body)
            .ok_or_else(|| CallError::Other("no content in response".to_string()))?;
        Ok(ChatReply {
            content,
            provider: ProviderId::Ollama.name().into(),
            model: self.config.ollama_model.clone(),
            cached: false,
        })
    }

    async fn chat_openai_compatible(
        &self,
        url: &str,
        api_key: &str,
        model: &str,
        provider: ProviderId,
        messages: &[ChatMessage],
    ) -> Result<ChatReply, CallError> {
        let resp = self
            .client
            .post(url)
            .timeout(Duration::from_secs(120))
            .bearer_auth(api_key)
            .json(&openai_body(model, messages))
            .send()
            .await
            .map_err(|e| CallError::Other(format!("request failed: {e}")))?;
        let status = resp.status();
        let retry_after = parse_retry_after(&resp);
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CallError::Other(format!("invalid response: {e}")))?;
        if !status.is_success() {
            return Err(CallError::Http {
                status: status.as_u16(),
                retry_after,
                detail: body["error"].to_string(),
            });
        }
        let content = extract_openai_content(&body)
            .ok_or_else(|| CallError::Other("no content in response".to_string()))?;
        Ok(ChatReply {
            content,
            provider: provider.name().into(),
            model: model.to_string(),
            cached: false,
        })
    }
}

fn parse_retry_after(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msgs() -> Vec<ChatMessage> {
        vec![ChatMessage {
            role: "user".into(),
            content: "hello".into(),
        }]
    }

    #[test]
    fn plan_is_ollama_only_without_keys() {
        let router = Router::new(RouterConfig::default());
        assert_eq!(router.provider_plan(), vec![ProviderId::Ollama]);
        assert!(!router.has_cloud_fallback());
    }

    #[test]
    fn plan_orders_local_before_cloud() {
        let config = RouterConfig {
            groq_api_key: Some("k".into()),
            openrouter_api_key: Some("k".into()),
            ..RouterConfig::default()
        };
        let router = Router::new(config);
        assert_eq!(
            router.provider_plan(),
            vec![ProviderId::Ollama, ProviderId::Groq, ProviderId::OpenRouter]
        );
    }

    #[test]
    fn ollama_body_disables_streaming_and_keeps_messages() {
        let body = ollama_body("llama3.2", &msgs());
        assert_eq!(body["stream"], false);
        assert_eq!(body["model"], "llama3.2");
        assert_eq!(body["messages"][0]["content"], "hello");
    }

    #[test]
    fn openai_body_shape() {
        let body = openai_body("m", &msgs());
        assert_eq!(body["model"], "m");
        assert_eq!(body["messages"][0]["role"], "user");
    }

    #[test]
    fn extracts_content_from_provider_responses() {
        let ollama = serde_json::json!({"message": {"role": "assistant", "content": "hi"}});
        assert_eq!(extract_ollama_content(&ollama).as_deref(), Some("hi"));
        let openai =
            serde_json::json!({"choices": [{"message": {"role": "assistant", "content": "hi"}}]});
        assert_eq!(extract_openai_content(&openai).as_deref(), Some("hi"));
        assert!(extract_openai_content(&serde_json::json!({})).is_none());
    }

    #[test]
    fn onboarding_points_at_free_options_only() {
        let text = onboarding_message();
        assert!(text.contains("ollama.com"));
        assert!(text.contains("GROQ_API_KEY"));
        assert!(text.contains("OPENROUTER_API_KEY"));
        assert!(text.to_lowercase().contains("free"));
    }

    #[tokio::test]
    async fn identical_request_is_served_from_cache_without_network() {
        // Router points at a dead port, but the cache is pre-seeded with
        // this exact request — chat must return the hit and mark it cached.
        let config = RouterConfig {
            ollama_base_url: "http://127.0.0.1:1".into(),
            ..RouterConfig::default()
        };
        let router = Router::new(config);
        let messages = msgs();
        let key = cache_key(&serde_json::to_string(&messages).unwrap());
        router.cache.lock().unwrap().put(
            key,
            ChatReply {
                content: "from cache".into(),
                provider: "ollama".into(),
                model: "llama3.2".into(),
                cached: false,
            },
            Instant::now(),
        );

        let reply = router.chat(&messages).await.expect("cache hit");
        assert_eq!(reply.content, "from cache");
        assert!(reply.cached, "hit must be marked as cached");

        // A different request misses the cache and fails normally.
        let other = vec![ChatMessage {
            role: "user".into(),
            content: "different".into(),
        }];
        assert!(router.chat(&other).await.is_err());
    }

    #[tokio::test]
    async fn cooled_down_cloud_provider_is_skipped() {
        let config = RouterConfig {
            ollama_base_url: "http://127.0.0.1:1".into(),
            groq_api_key: Some("k".into()),
            ..RouterConfig::default()
        };
        let router = Router::new(config);
        router.cooldowns.lock().unwrap().penalize(
            "groq",
            Some(Duration::from_secs(3600)),
            Instant::now(),
        );

        match router.chat(&msgs()).await {
            Err(RouterError::AllProvidersFailed(detail)) => {
                assert!(
                    detail.contains("groq: on rate-limit cooldown"),
                    "groq must be skipped, got: {detail}"
                );
            }
            other => panic!("expected AllProvidersFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unconfigured_router_returns_onboarding_not_error_soup() {
        // Points at a port nothing listens on; no cloud keys configured.
        let config = RouterConfig {
            ollama_base_url: "http://127.0.0.1:1".into(),
            ..RouterConfig::default()
        };
        let router = Router::new(config);
        match router.chat(&msgs()).await {
            Err(RouterError::NoProvider(text)) => assert!(text.contains("ollama.com")),
            other => panic!("expected NoProvider onboarding, got {other:?}"),
        }
    }
}
