//! OpenAI-compatible embedding provider.
//!
//! Covers OpenAI, Voyage AI, and any custom endpoint that speaks the
//! OpenAI embeddings protocol.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::embedder::{EmbedError, Embedder};
use crate::Vector;

/// Configuration for an OpenAI-compatible embeddings provider.
#[derive(Debug, Clone)]
pub struct OpenAIConfig {
    /// API base URL (e.g. "https://api.openai.com/v1").
    /// Must not include the /embeddings path.
    pub base_url: String,

    /// API key for Bearer token authentication.
    pub api_key: String,

    /// Model name (e.g. "text-embedding-3-small", "voyage-3-lite").
    pub model: String,

    /// Desired output dimensions (None = provider default).
    pub dimensions: Option<i32>,

    /// Optional input type hint ("query" or "document").
    /// Used by Voyage AI; ignored by OpenAI.
    pub input_type: Option<String>,

    /// HTTP request timeout in seconds (None = 30s default).
    pub timeout_secs: Option<u64>,
}

/// Request body for OpenAI-compatible embedding endpoints.
#[derive(Debug, Serialize)]
struct EmbedRequest {
    input: Vec<String>,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_type: Option<String>,
}

/// Response from OpenAI-compatible embedding endpoints.
#[derive(Debug, Deserialize)]
struct EmbedResponse {
    data: Vec<EmbedData>,
}

#[derive(Debug, Deserialize)]
struct EmbedData {
    embedding: Vector,
    #[allow(dead_code)]
    index: usize,
}

/// An embedder for any OpenAI-compatible API.
pub struct OpenAICompatible {
    agent: ureq::Agent,
    cfg: OpenAIConfig,
}

impl OpenAICompatible {
    /// Creates a new OpenAI-compatible embedder from config.
    pub fn new(cfg: OpenAIConfig) -> Result<Self, EmbedError> {
        let timeout = Duration::from_secs(cfg.timeout_secs.unwrap_or(30));
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .build()
            .into();
        Ok(Self { agent, cfg })
    }
}

impl Embedder for OpenAICompatible {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vector>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let url = format!(
            "{}/embeddings",
            self.cfg.base_url.trim_end_matches('/')
        );
        let req_body = EmbedRequest {
            input: texts.to_vec(),
            model: self.cfg.model.clone(),
            dimensions: self.cfg.dimensions,
            input_type: self.cfg.input_type.clone(),
        };

        let response = self
            .agent
            .post(&url)
            .header("Authorization", &format!("Bearer {}", self.cfg.api_key))
            .header("Content-Type", "application/json")
            .send_json(&req_body)?;

        let resp: EmbedResponse = response.into_body().read_json()?;

        let mut data = resp.data;
        data.sort_by_key(|d| d.index);

        if data.len() != texts.len() {
            return Err(EmbedError::Response(format!(
                "expected {} embeddings, got {}",
                texts.len(),
                data.len()
            )));
        }

        Ok(data.into_iter().map(|d| d.embedding).collect())
    }
}

/// Creates an embedder for OpenAI.
pub fn openai(
    api_key: &str,
    model: &str,
    dimensions: Option<i32>,
) -> Result<OpenAICompatible, EmbedError> {
    OpenAICompatible::new(OpenAIConfig {
        base_url: "https://api.openai.com/v1".to_string(),
        api_key: api_key.to_string(),
        model: model.to_string(),
        dimensions,
        input_type: None,
        timeout_secs: None,
    })
}

/// Creates an embedder for Voyage AI.
pub fn voyage_ai(
    api_key: &str,
    model: &str,
    input_type: Option<&str>,
) -> Result<OpenAICompatible, EmbedError> {
    OpenAICompatible::new(OpenAIConfig {
        base_url: "https://api.voyageai.com/v1".to_string(),
        api_key: api_key.to_string(),
        model: model.to_string(),
        dimensions: None,
        input_type: input_type.map(String::from),
        timeout_secs: None,
    })
}
