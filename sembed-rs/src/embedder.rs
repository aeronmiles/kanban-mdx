//! Embedding provider abstraction.

use crate::Vector;

/// Errors that can occur during embedding operations.
#[derive(Debug, thiserror::Error)]
pub enum EmbedError {
    #[error("embedding API request failed: {0}")]
    Request(String),

    #[error("embedding API error (status {status}): {body}")]
    ApiError { status: u16, body: String },

    #[error("unexpected embedding response: {0}")]
    Response(String),

    #[error("provider configuration error: {0}")]
    Config(String),

    #[error("HTTP client error: {0}")]
    Http(#[from] ureq::Error),
}

/// Trait for generating vector embeddings from text.
pub trait Embedder: Send + Sync {
    /// Embeds a batch of texts and returns their embedding vectors
    /// in the same order as the input.
    fn embed(&self, texts: &[String]) -> Result<Vec<Vector>, EmbedError>;
}
