//! Minimal, provider-agnostic semantic embeddings library.
//!
//! # Features
//!
//! - **`Embedder` trait** — one method, any provider
//! - **Built-in providers** — OpenAI, Voyage AI, Ollama, or any OpenAI-compatible endpoint
//! - **In-memory index** — brute-force cosine similarity search, thread-safe
//! - **JSON persistence** — `save`/`load` via `io::Write`/`io::Read`
//! - **Staleness detection** — content hashing to know when embeddings need refreshing

pub mod embedder;
pub mod hash;
pub mod index;
pub mod mock;
pub mod ollama;
pub mod openai;
pub mod vector;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A dense embedding vector.
pub type Vector = Vec<f32>;

/// A document stored in the embedding index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub content: String,
    pub content_hash: String,
    pub vector: Vector,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

/// A search result pairing a document with its similarity score.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub document: Document,
    pub score: f32,
}

// Re-exports for convenience.
pub use embedder::{EmbedError, Embedder};
pub use hash::content_hash;
pub use index::Index;
pub use mock::MockEmbedder;
pub use ollama::{Ollama, OllamaConfig};
pub use openai::{OpenAICompatible, OpenAIConfig};
pub use vector::{cosine_similarity, dot, magnitude, normalize};
