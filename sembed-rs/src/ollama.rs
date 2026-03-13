//! Ollama embedding provider for local models.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::embedder::{EmbedError, Embedder};
use crate::Vector;

/// Configuration for the Ollama embeddings provider.
#[derive(Debug, Clone)]
pub struct OllamaConfig {
    /// Base URL. Defaults to "http://localhost:11434" if empty.
    pub base_url: String,

    /// Model name (e.g. "nomic-embed-text", "mxbai-embed-large").
    pub model: String,

    /// HTTP request timeout in seconds (None = 30s default).
    pub timeout_secs: Option<u64>,
}

/// Request body for the Ollama embed endpoint.
#[derive(Debug, Serialize)]
struct EmbedRequest {
    model: String,
    input: Vec<String>,
}

/// Response from the Ollama embed endpoint.
#[derive(Debug, Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vector>,
}

/// An embedder for a local Ollama instance.
pub struct Ollama {
    agent: ureq::Agent,
    cfg: OllamaConfig,
}

impl Ollama {
    /// Creates a new Ollama embedder.
    ///
    /// If `base_url` is empty, defaults to "http://localhost:11434".
    pub fn new(cfg: OllamaConfig) -> Result<Self, EmbedError> {
        let timeout = Duration::from_secs(cfg.timeout_secs.unwrap_or(30));
        let cfg = OllamaConfig {
            base_url: if cfg.base_url.is_empty() {
                "http://localhost:11434".to_string()
            } else {
                cfg.base_url
            },
            ..cfg
        };
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .build()
            .into();
        Ok(Self { agent, cfg })
    }
}

impl Embedder for Ollama {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vector>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let url = format!(
            "{}/api/embed",
            self.cfg.base_url.trim_end_matches('/')
        );
        let req_body = EmbedRequest {
            model: self.cfg.model.clone(),
            input: texts.to_vec(),
        };

        let response = self
            .agent
            .post(&url)
            .header("Content-Type", "application/json")
            .send_json(&req_body)?;

        let resp: EmbedResponse = response.into_body().read_json()?;

        if resp.embeddings.len() != texts.len() {
            return Err(EmbedError::Response(format!(
                "expected {} embeddings, got {}",
                texts.len(),
                resp.embeddings.len()
            )));
        }

        Ok(resp.embeddings)
    }
}
