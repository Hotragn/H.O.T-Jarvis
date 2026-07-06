//! Model-agnostic router: local Ollama first, then free cloud tiers.
//! Never requires a paid API. With nothing configured it returns a
//! structured onboarding message instead of a raw error.

use serde::{Deserialize, Serialize};
use std::time::Duration;

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
}

impl Router {
    pub fn new(config: RouterConfig) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(4))
            .build()
            .expect("failed to build http client");
        Self { config, client }
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
        let mut failures: Vec<String> = Vec::new();
        for provider in self.provider_plan() {
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
                Ok(reply) => return Ok(reply),
                Err(e) => failures.push(format!("{}: {e}", provider.name())),
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

    async fn chat_ollama(&self, messages: &[ChatMessage]) -> Result<ChatReply, String> {
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
            .map_err(|e| format!("request failed: {e}"))?;
        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("invalid response: {e}"))?;
        if !status.is_success() {
            return Err(format!("http {status}: {}", body["error"]));
        }
        let content =
            extract_ollama_content(&body).ok_or_else(|| "no content in response".to_string())?;
        Ok(ChatReply {
            content,
            provider: ProviderId::Ollama.name().into(),
            model: self.config.ollama_model.clone(),
        })
    }

    async fn chat_openai_compatible(
        &self,
        url: &str,
        api_key: &str,
        model: &str,
        provider: ProviderId,
        messages: &[ChatMessage],
    ) -> Result<ChatReply, String> {
        let resp = self
            .client
            .post(url)
            .timeout(Duration::from_secs(120))
            .bearer_auth(api_key)
            .json(&openai_body(model, messages))
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))?;
        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("invalid response: {e}"))?;
        if !status.is_success() {
            return Err(format!("http {status}: {}", body["error"]));
        }
        let content =
            extract_openai_content(&body).ok_or_else(|| "no content in response".to_string())?;
        Ok(ChatReply {
            content,
            provider: provider.name().into(),
            model: model.to_string(),
        })
    }
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
