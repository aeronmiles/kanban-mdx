//! Semantic search embedding system for kanban tasks.
//!
//! Provides the [`Manager`] which coordinates embedding generation and index
//! persistence. The index stores chunks (title+tags preamble and markdown
//! sections) rather than whole tasks, enabling both task-level search and
//! section-level find.
//!
//! Core embedding functionality (index, providers, vector math) is provided
//! by the [`sembed_rs`] crate. This module adds kanban-specific logic: chunking,
//! the manager, and provider factory from config.

pub mod chunk;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use sembed_rs::{self, Document, Index, SqliteStore};

use crate::model::config::{Config, SemanticSearchConfig};
use crate::model::task::Task;

use chunk::{chunk_task, parse_chunk_id, task_content, Chunk};

/// Default name for the embeddings index file.
pub const INDEX_FILE: &str = ".embeddings.db";

/// Legacy JSON index file name (removed on startup if present).
const LEGACY_INDEX_FILE: &str = ".embeddings.json";

/// Environment variable name for the embedding provider API key.
pub const API_KEY_ENV: &str = "KANBAN_EMBED_API_KEY";

/// Maximum number of chunks to embed in a single API call.
const MAX_BATCH: usize = 512;

// Re-export sembed_rs types used by the rest of the codebase.
pub use sembed_rs::EmbedError;

// ---------------------------------------------------------------------------
// Search results (kanban-specific, aggregated by task)
// ---------------------------------------------------------------------------

/// A task-level semantic search result.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub task_id: i32,
    pub score: f32,
}

/// A section-level semantic find result.
#[derive(Debug, Clone)]
pub struct FindResult {
    pub task_id: i32,
    pub chunk: usize,
    pub header: String,
    pub line: usize,
    pub score: f32,
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Coordinates embedding generation and index persistence for kanban tasks.
pub struct Manager {
    index: Index,
    doc_embedder: Box<dyn sembed_rs::Embedder>,
    query_embedder: Box<dyn sembed_rs::Embedder>,
    index_path: PathBuf,
}

/// Error type for the embed manager.
#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    #[error("{0}")]
    Embed(#[from] EmbedError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("store error: {0}")]
    Store(#[from] sembed_rs::StoreError),

    #[error("{0}")]
    Config(String),
}

impl Manager {
    /// Test-only constructor that accepts pre-built embedders.
    ///
    /// Uses an in-memory index with no persistence.
    #[cfg(test)]
    pub fn with_embedders(
        doc_embedder: Box<dyn sembed_rs::Embedder>,
        query_embedder: Box<dyn sembed_rs::Embedder>,
        index_path: PathBuf,
    ) -> Self {
        Self {
            index: Index::new(),
            doc_embedder,
            query_embedder,
            index_path,
        }
    }

    /// Creates a Manager from kanban config.
    ///
    /// Opens (or creates) a SQLite-backed embedding index. If a legacy
    /// `.embeddings.json` exists, it is deleted (embeddings will be
    /// recomputed on next sync).
    pub fn new(cfg: &Config) -> Result<Self, ManagerError> {
        let (doc_embedder, query_embedder) =
            create_embedders(&cfg.semantic_search).map_err(ManagerError::Embed)?;

        let index_path = cfg.dir().join(INDEX_FILE);

        // Remove legacy JSON index if present.
        let legacy_path = cfg.dir().join(LEGACY_INDEX_FILE);
        if legacy_path.exists() {
            let _ = fs::remove_file(&legacy_path);
        }

        // Open SQLite store and create index.
        let store = SqliteStore::open(&index_path)?;
        let index = Index::with_store(Box::new(store))?;

        Ok(Self {
            index,
            doc_embedder,
            query_embedder,
            index_path,
        })
    }

    /// Returns the number of documents in the index.
    pub fn doc_count(&self) -> usize {
        self.index.len()
    }

    /// Returns the path to the index file.
    pub fn index_path(&self) -> &Path {
        &self.index_path
    }

    /// Syncs all provided tasks with the embedding index.
    ///
    /// Detects stale entries via content hashing and only re-embeds changed
    /// tasks. Removed tasks are pruned from the index.
    pub fn sync(&mut self, tasks: &[Task]) -> Result<SyncStats, ManagerError> {
        // Build current hash map keyed by task ID.
        let mut current_hash: HashMap<String, String> = HashMap::with_capacity(tasks.len());
        for t in tasks {
            current_hash.insert(
                t.id.to_string(),
                sembed_rs::content_hash(&task_content(t)),
            );
        }

        // Build the expected chunk ID -> hash map for staleness detection.
        let mut expected_chunks: HashMap<String, String> = HashMap::new();
        let mut all_chunks: HashMap<i32, Vec<Chunk>> = HashMap::new();

        for t in tasks {
            let chunks = chunk_task(t);
            let hash = &current_hash[&t.id.to_string()];
            for c in &chunks {
                expected_chunks.insert(c.chunk_id(), hash.clone());
            }
            all_chunks.insert(t.id, chunks);
        }

        // Detect stale chunk IDs.
        let stale_ids = self.index.stale(&expected_chunks);
        let mut stale_task_ids: std::collections::HashSet<i32> =
            std::collections::HashSet::new();
        for id in &stale_ids {
            if let Some((task_id, _)) = parse_chunk_id(id) {
                stale_task_ids.insert(task_id);
            }
        }

        // Collect tasks that need re-embedding: stale or new.
        let mut to_embed: Vec<Chunk> = Vec::new();
        for (tid, chunks) in &all_chunks {
            if stale_task_ids.contains(tid) {
                to_embed.extend(chunks.iter().cloned());
                continue;
            }
            // Check if any chunk is missing from the index.
            let mut missing = false;
            for c in chunks {
                if self.index.get(&c.chunk_id()).is_none() {
                    missing = true;
                    break;
                }
            }
            if missing {
                to_embed.extend(chunks.iter().cloned());
            }
        }

        let mut embedded_count = 0;

        // Embed all chunks that need updating, batched to stay within API limits.
        if !to_embed.is_empty() {
            for batch_start in (0..to_embed.len()).step_by(MAX_BATCH) {
                let batch_end = (batch_start + MAX_BATCH).min(to_embed.len());
                let batch = &to_embed[batch_start..batch_end];

                let texts: Vec<String> = batch.iter().map(|c| c.text.clone()).collect();
                let vecs = self.doc_embedder.embed(&texts)?;

                let docs: Vec<Document> = batch
                    .iter()
                    .zip(vecs.into_iter())
                    .map(|(c, vec)| {
                        let hash = current_hash[&c.task_id.to_string()].clone();
                        let mut metadata = HashMap::new();
                        metadata.insert("header".to_string(), c.header.clone());
                        metadata.insert("line".to_string(), c.line.to_string());
                        Document {
                            id: c.chunk_id(),
                            content: c.text.clone(),
                            content_hash: hash,
                            vector: vec,
                            metadata,
                        }
                    })
                    .collect();

                embedded_count += docs.len();
                self.index.add(docs);
            }
        }

        // Prune chunks for deleted tasks and removed sections.
        let mut remove_ids: Vec<String> = Vec::new();
        for id in &stale_ids {
            if !expected_chunks.contains_key(id) {
                remove_ids.push(id.clone());
            }
        }
        // Also prune any legacy single-doc entries (plain task ID without ":").
        for doc in self.index.all() {
            if !doc.id.contains(':') {
                remove_ids.push(doc.id.clone());
            }
        }
        let pruned_count = remove_ids.len();
        if !remove_ids.is_empty() {
            self.index.remove(&remove_ids);
        }

        Ok(SyncStats {
            total_tasks: tasks.len(),
            total_chunks: self.index.len(),
            embedded: embedded_count,
            pruned: pruned_count,
        })
    }

    /// Searches the index for the top-k most similar tasks, aggregated by
    /// best chunk score per task.
    pub fn search(&self, query: &str, k: usize) -> Result<Vec<SearchResult>, ManagerError> {
        if self.index.is_empty() {
            return Ok(Vec::new());
        }

        let vecs = self.query_embedder.embed(&[query.to_string()])?;
        if vecs.is_empty() {
            return Ok(Vec::new());
        }
        let query_vec = &vecs[0];

        // Compute similarity for all chunks.
        let mut best: HashMap<i32, f32> = HashMap::new();
        for doc in self.index.all() {
            let task_id = if let Some((tid, _)) = parse_chunk_id(&doc.id) {
                tid
            } else if let Ok(id) = doc.id.parse::<i32>() {
                // Legacy single-doc ID support.
                id
            } else {
                continue;
            };

            let score = sembed_rs::cosine_similarity(query_vec, &doc.vector);
            let entry = best.entry(task_id).or_insert(0.0);
            if score > *entry {
                *entry = score;
            }
        }

        // Sort by score descending.
        let mut results: Vec<SearchResult> = best
            .into_iter()
            .map(|(task_id, score)| SearchResult { task_id, score })
            .collect();
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        if k > 0 && k < results.len() {
            results.truncate(k);
        }

        Ok(results)
    }

    /// Searches the index for the top-k most similar sections with chunk metadata.
    pub fn find(&self, query: &str, k: usize) -> Result<Vec<FindResult>, ManagerError> {
        if self.index.is_empty() {
            return Ok(Vec::new());
        }

        let vecs = self.query_embedder.embed(&[query.to_string()])?;
        if vecs.is_empty() {
            return Ok(Vec::new());
        }
        let query_vec = &vecs[0];

        let mut hits: Vec<(FindResult, f32)> = Vec::new();
        for doc in self.index.all() {
            let (task_id, chunk_idx) = match parse_chunk_id(&doc.id) {
                Some(v) => v,
                None => continue, // skip legacy single-doc entries
            };

            let score = sembed_rs::cosine_similarity(query_vec, &doc.vector);
            let header = doc
                .metadata
                .get("header")
                .cloned()
                .unwrap_or_default();
            let line: usize = doc
                .metadata
                .get("line")
                .and_then(|l| l.parse().ok())
                .unwrap_or(0);

            hits.push((
                FindResult {
                    task_id,
                    chunk: chunk_idx,
                    header,
                    line,
                    score,
                },
                score,
            ));
        }

        hits.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let limit = if k > 0 { k.min(hits.len()) } else { hits.len() };
        let results: Vec<FindResult> = hits.into_iter().take(limit).map(|(r, _)| r).collect();

        Ok(results)
    }

    /// Removes all documents from the index and deletes the index file.
    pub fn clear(&mut self) -> Result<(), ManagerError> {
        // Drop the old index (closes SQLite connection) and create a fresh in-memory one.
        self.index = Index::new();
        if self.index_path.exists() {
            fs::remove_file(&self.index_path)?;
        }
        // Also remove WAL/SHM sidecar files if present.
        let wal = self.index_path.with_extension("db-wal");
        let shm = self.index_path.with_extension("db-shm");
        let _ = fs::remove_file(wal);
        let _ = fs::remove_file(shm);
        Ok(())
    }
}

/// Statistics returned by a sync operation.
#[derive(Debug, Clone)]
pub struct SyncStats {
    /// Total number of tasks processed.
    pub total_tasks: usize,
    /// Total number of chunks in the index after sync.
    pub total_chunks: usize,
    /// Number of chunks that were (re-)embedded.
    pub embedded: usize,
    /// Number of chunks pruned from the index.
    pub pruned: usize,
}

// ---------------------------------------------------------------------------
// Status information
// ---------------------------------------------------------------------------

/// Information about the current state of the embedding index.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EmbedStatus {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    pub dimensions: i32,
    pub index_file: String,
    pub documents: usize,
    pub file_size_bytes: u64,
    pub last_sync: String,
}

/// Gathers status information about the embedding index without creating
/// a full Manager (which requires API credentials).
pub fn get_status(cfg: &Config) -> EmbedStatus {
    let ss = &cfg.semantic_search;
    let index_path = cfg.dir().join(INDEX_FILE);

    let effective_dims = if ss.dimensions > 0 {
        ss.dimensions
    } else {
        sembed_rs::model_default_dimensions(&ss.model).unwrap_or(0)
    };

    let mut status = EmbedStatus {
        enabled: ss.enabled,
        provider: ss.provider.clone(),
        model: ss.model.clone(),
        dimensions: effective_dims,
        index_file: String::new(),
        documents: 0,
        file_size_bytes: 0,
        last_sync: String::new(),
    };

    if let Ok(metadata) = fs::metadata(&index_path) {
        status.index_file = index_path.to_string_lossy().to_string();
        status.file_size_bytes = metadata.len();

        if let Ok(modified) = metadata.modified() {
            let dt: chrono::DateTime<chrono::Utc> = modified.into();
            status.last_sync = dt.to_rfc3339();
        }
    }

    // Try to load and count documents via SQLite store.
    if ss.enabled && !status.index_file.is_empty() {
        if let Ok(store) = SqliteStore::open(&index_path) {
            if let Ok(count) = sembed_rs::Store::count(&store) {
                status.documents = count;
            }
        }
    }

    status
}

// ---------------------------------------------------------------------------
// Provider factory (kanban config → sembed_rs embedders)
// ---------------------------------------------------------------------------

/// Creates embedder instances from the semantic search configuration.
///
/// Returns separate document and query embedders. For Voyage AI, these use
/// different `input_type` parameters ("document" vs "query"). For other
/// providers, both embedders are identical.
fn create_embedders(
    cfg: &SemanticSearchConfig,
) -> Result<(Box<dyn sembed_rs::Embedder>, Box<dyn sembed_rs::Embedder>), EmbedError> {
    let api_key = std::env::var(API_KEY_ENV).unwrap_or_default();

    if api_key.is_empty() && cfg.provider != "ollama" {
        return Err(EmbedError::Config(format!(
            "semantic search enabled but {} is not set; export {}=<your-api-key> and retry",
            API_KEY_ENV, API_KEY_ENV
        )));
    }

    let dimensions = if cfg.dimensions > 0 {
        Some(cfg.dimensions)
    } else {
        None
    };

    match cfg.provider.as_str() {
        "voyage" => {
            let base_url = if cfg.base_url.is_empty() {
                "https://api.voyageai.com/v1".to_string()
            } else {
                cfg.base_url.clone()
            };
            // Voyage API does not support the `dimensions` parameter.
            let doc = Box::new(sembed_rs::OpenAICompatible::new(sembed_rs::OpenAIConfig {
                base_url: base_url.clone(),
                api_key: api_key.clone(),
                model: cfg.model.clone(),
                dimensions: None,
                input_type: Some("document".to_string()),
                timeout_secs: None,
            })?);
            let query = Box::new(sembed_rs::OpenAICompatible::new(sembed_rs::OpenAIConfig {
                base_url,
                api_key,
                model: cfg.model.clone(),
                dimensions: None,
                input_type: Some("query".to_string()),
                timeout_secs: None,
            })?);
            Ok((doc, query))
        }
        "openai" => {
            let base_url = if cfg.base_url.is_empty() {
                "https://api.openai.com/v1".to_string()
            } else {
                cfg.base_url.clone()
            };
            let doc = Box::new(sembed_rs::OpenAICompatible::new(sembed_rs::OpenAIConfig {
                base_url: base_url.clone(),
                api_key: api_key.clone(),
                model: cfg.model.clone(),
                dimensions,
                input_type: None,
                timeout_secs: None,
            })?);
            let query = Box::new(sembed_rs::OpenAICompatible::new(sembed_rs::OpenAIConfig {
                base_url,
                api_key,
                model: cfg.model.clone(),
                dimensions,
                input_type: None,
                timeout_secs: None,
            })?);
            Ok((doc, query))
        }
        "ollama" => {
            let doc = Box::new(sembed_rs::Ollama::new(sembed_rs::OllamaConfig {
                base_url: cfg.base_url.clone(),
                model: cfg.model.clone(),
                timeout_secs: None,
            })?);
            let query = Box::new(sembed_rs::Ollama::new(sembed_rs::OllamaConfig {
                base_url: cfg.base_url.clone(),
                model: cfg.model.clone(),
                timeout_secs: None,
            })?);
            Ok((doc, query))
        }
        "custom" => {
            if cfg.base_url.is_empty() {
                return Err(EmbedError::Config(
                    "custom provider requires base_url in config".to_string(),
                ));
            }
            let input_type = if cfg.input_type.is_empty() {
                None
            } else {
                Some(cfg.input_type.clone())
            };
            let doc = Box::new(sembed_rs::OpenAICompatible::new(sembed_rs::OpenAIConfig {
                base_url: cfg.base_url.clone(),
                api_key: api_key.clone(),
                model: cfg.model.clone(),
                dimensions,
                input_type: input_type.clone(),
                timeout_secs: None,
            })?);
            let query = Box::new(sembed_rs::OpenAICompatible::new(sembed_rs::OpenAIConfig {
                base_url: cfg.base_url.clone(),
                api_key,
                model: cfg.model.clone(),
                dimensions,
                input_type,
                timeout_secs: None,
            })?);
            Ok((doc, query))
        }
        "" => Err(EmbedError::Config(
            "no provider configured; add semantic_search.provider to config.toml \
             (voyage, openai, ollama, or custom)"
                .into(),
        )),
        other => Err(EmbedError::Config(format!(
            "unsupported embedding provider: {:?}",
            other
        ))),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_deterministic() {
        let h1 = sembed_rs::content_hash("hello world");
        let h2 = sembed_rs::content_hash("hello world");
        assert_eq!(h1, h2);
        assert!(!h1.is_empty());
    }

    #[test]
    fn test_content_hash_different() {
        let h1 = sembed_rs::content_hash("hello");
        let h2 = sembed_rs::content_hash("world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = sembed_rs::cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = sembed_rs::cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let a: Vec<f32> = Vec::new();
        let b: Vec<f32> = Vec::new();
        assert_eq!(sembed_rs::cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_index_add_and_get() {
        let idx = Index::new();
        assert!(idx.is_empty());

        let doc = Document {
            id: "1:0".to_string(),
            content: "hello".to_string(),
            content_hash: "abc".to_string(),
            vector: vec![1.0, 0.0],
            metadata: HashMap::new(),
        };
        idx.add(vec![doc]);

        assert_eq!(idx.len(), 1);
        assert!(idx.get("1:0").is_some());
        assert!(idx.get("1:1").is_none());
    }

    #[test]
    fn test_index_add_replaces() {
        let idx = Index::new();

        let doc1 = Document {
            id: "1:0".to_string(),
            content: "old".to_string(),
            content_hash: "abc".to_string(),
            vector: vec![1.0, 0.0],
            metadata: HashMap::new(),
        };
        idx.add(vec![doc1]);

        let doc2 = Document {
            id: "1:0".to_string(),
            content: "new".to_string(),
            content_hash: "def".to_string(),
            vector: vec![0.0, 1.0],
            metadata: HashMap::new(),
        };
        idx.add(vec![doc2]);

        assert_eq!(idx.len(), 1);
        assert_eq!(idx.get("1:0").unwrap().content, "new");
    }

    #[test]
    fn test_index_remove() {
        let idx = Index::new();
        idx.add(vec![
            Document {
                id: "1:0".to_string(),
                content: "a".to_string(),
                content_hash: "a".to_string(),
                vector: vec![1.0],
                metadata: HashMap::new(),
            },
            Document {
                id: "2:0".to_string(),
                content: "b".to_string(),
                content_hash: "b".to_string(),
                vector: vec![2.0],
                metadata: HashMap::new(),
            },
        ]);

        idx.remove(&["1:0".to_string()]);
        assert_eq!(idx.len(), 1);
        assert!(idx.get("1:0").is_none());
        assert!(idx.get("2:0").is_some());
    }

    #[test]
    fn test_index_stale() {
        let idx = Index::new();
        idx.add(vec![
            Document {
                id: "1:0".to_string(),
                content: "a".to_string(),
                content_hash: "hash_a".to_string(),
                vector: vec![1.0],
                metadata: HashMap::new(),
            },
            Document {
                id: "2:0".to_string(),
                content: "b".to_string(),
                content_hash: "hash_b".to_string(),
                vector: vec![2.0],
                metadata: HashMap::new(),
            },
        ]);

        let mut expected = HashMap::new();
        expected.insert("1:0".to_string(), "hash_a_new".to_string()); // stale
        expected.insert("3:0".to_string(), "hash_c".to_string()); // new

        let stale = idx.stale(&expected);
        assert!(stale.contains(&"1:0".to_string())); // stale hash
        assert!(stale.contains(&"3:0".to_string())); // missing
        assert!(stale.contains(&"2:0".to_string())); // orphaned
    }

    #[test]
    fn test_create_embedders_unsupported() {
        let cfg = SemanticSearchConfig {
            enabled: true,
            provider: "unknown".to_string(),
            model: "test".to_string(),
            ..Default::default()
        };
        let result = create_embedders(&cfg);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_embedders_missing_api_key() {
        std::env::remove_var(API_KEY_ENV);
        let cfg = SemanticSearchConfig {
            enabled: true,
            provider: "voyage".to_string(),
            model: "voyage-3".to_string(),
            ..Default::default()
        };
        let result = create_embedders(&cfg);
        let err_msg = result.err().expect("expected error").to_string();
        assert!(err_msg.contains(API_KEY_ENV));
    }

    #[test]
    fn test_create_embedders_ollama_no_key() {
        std::env::remove_var(API_KEY_ENV);
        let cfg = SemanticSearchConfig {
            enabled: true,
            provider: "ollama".to_string(),
            model: "nomic-embed-text".to_string(),
            base_url: "http://localhost:11434".to_string(),
            ..Default::default()
        };
        let result = create_embedders(&cfg);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_embedders_custom_missing_base_url() {
        std::env::set_var(API_KEY_ENV, "test-key");
        let cfg = SemanticSearchConfig {
            enabled: true,
            provider: "custom".to_string(),
            model: "test".to_string(),
            base_url: String::new(),
            ..Default::default()
        };
        let result = create_embedders(&cfg);
        assert!(result.is_err());
        std::env::remove_var(API_KEY_ENV);
    }

    // -----------------------------------------------------------------------
    // Integration tests using MockEmbedder
    // -----------------------------------------------------------------------

    use std::sync::{Arc, Mutex};

    fn make_test_task(id: i32, title: &str, body: &str) -> Task {
        Task {
            id,
            title: title.to_string(),
            body: body.to_string(),
            status: "todo".to_string(),
            file: format!("/tmp/test-{}.md", id),
            ..Default::default()
        }
    }

    fn test_manager(dir: &Path) -> (Manager, Arc<Mutex<Vec<Vec<String>>>>) {
        let doc_mock = sembed_rs::MockEmbedder::new(8);
        let query_mock = sembed_rs::MockEmbedder::new(8);
        let doc_calls = doc_mock.calls.clone();
        let mgr = Manager::with_embedders(
            Box::new(doc_mock),
            Box::new(query_mock),
            dir.join(INDEX_FILE),
        );
        (mgr, doc_calls)
    }

    #[test]
    fn test_sync_embeds_all_tasks() {
        let dir = tempfile::tempdir().unwrap();
        let (mut mgr, _calls) = test_manager(dir.path());

        let tasks = vec![
            make_test_task(1, "Implement login", "Add OAuth2 flow"),
            make_test_task(2, "Fix database bug", "Connection pool exhaustion"),
            make_test_task(3, "Update docs", "Add API reference"),
        ];

        let stats = mgr.sync(&tasks).unwrap();

        assert_eq!(stats.total_tasks, 3);
        assert!(stats.embedded > 0, "expected some chunks to be embedded");
        assert!(
            mgr.doc_count() > 0,
            "expected doc_count > 0 after sync, got {}",
            mgr.doc_count()
        );
        // Each task produces at least a preamble chunk + a body chunk (since all
        // have non-empty bodies), so total chunks should be >= 2 * 3.
        assert!(
            mgr.doc_count() >= 6,
            "expected at least 6 chunks for 3 tasks with bodies, got {}",
            mgr.doc_count()
        );
    }

    #[test]
    fn test_sync_detects_stale() {
        let dir = tempfile::tempdir().unwrap();
        let (mut mgr, calls) = test_manager(dir.path());

        let tasks = vec![
            make_test_task(1, "Task one", "Body one"),
            make_test_task(2, "Task two", "Body two"),
        ];

        // First sync — should embed all tasks.
        mgr.sync(&tasks).unwrap();
        let calls_after_first = calls.lock().unwrap().len();
        assert!(calls_after_first > 0);

        // Second sync — nothing changed, no new embed calls.
        mgr.sync(&tasks).unwrap();
        let calls_after_second = calls.lock().unwrap().len();
        assert_eq!(
            calls_after_first, calls_after_second,
            "expected no new embed calls when nothing changed"
        );

        // Third sync — modify task 2's body.
        let mut modified_tasks = tasks.clone();
        modified_tasks[1] = make_test_task(2, "Task two", "Body two updated");
        let stats = mgr.sync(&modified_tasks).unwrap();
        let calls_after_third = calls.lock().unwrap().len();
        assert!(
            calls_after_third > calls_after_second,
            "expected new embed calls after modifying a task"
        );
        assert!(
            stats.embedded > 0,
            "expected some chunks re-embedded for the changed task"
        );
    }

    #[test]
    fn test_sync_prunes_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let (mut mgr, _calls) = test_manager(dir.path());

        let tasks = vec![
            make_test_task(1, "Task one", "Body one"),
            make_test_task(2, "Task two", "Body two"),
            make_test_task(3, "Task three", "Body three"),
        ];

        mgr.sync(&tasks).unwrap();
        let count_before = mgr.doc_count();

        // Re-sync with only 2 tasks — task 3 is deleted.
        let fewer_tasks = vec![
            make_test_task(1, "Task one", "Body one"),
            make_test_task(2, "Task two", "Body two"),
        ];
        let stats = mgr.sync(&fewer_tasks).unwrap();

        assert!(
            stats.pruned > 0,
            "expected pruned > 0 when a task is removed"
        );
        assert!(
            mgr.doc_count() < count_before,
            "expected doc_count to decrease after pruning, before={} after={}",
            count_before,
            mgr.doc_count()
        );
    }

    #[test]
    fn test_search_returns_ranked_results() {
        let dir = tempfile::tempdir().unwrap();
        let (mut mgr, _calls) = test_manager(dir.path());

        let tasks = vec![
            make_test_task(1, "Implement user authentication", "OAuth2 login flow"),
            make_test_task(2, "Fix database connection pool", "Pool exhaustion under load"),
            make_test_task(3, "Write API documentation", "REST endpoints reference"),
            make_test_task(4, "Add search functionality", "Full text search with ranking"),
            make_test_task(5, "Refactor error handling", "Consistent error types"),
        ];

        mgr.sync(&tasks).unwrap();

        let results = mgr.search("authentication login", 5).unwrap();
        assert!(
            !results.is_empty(),
            "expected non-empty search results"
        );

        // Verify results are sorted by score descending.
        for w in results.windows(2) {
            assert!(
                w[0].score >= w[1].score,
                "results not sorted by score descending: {} < {}",
                w[0].score,
                w[1].score
            );
        }
    }

    #[test]
    fn test_find_returns_section_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let (mut mgr, _calls) = test_manager(dir.path());

        let body = "## Implementation\n\
                     Build the core search engine with vector similarity.\n\n\
                     ## Testing\n\
                     Add unit tests for embedding and ranking.";

        let tasks = vec![make_test_task(1, "Search feature", body)];

        mgr.sync(&tasks).unwrap();

        let results = mgr.find("search engine", 10).unwrap();
        assert!(
            !results.is_empty(),
            "expected non-empty find results"
        );

        // Check that at least one result has a populated header.
        let has_header = results.iter().any(|r| !r.header.is_empty());
        assert!(
            has_header,
            "expected at least one find result with a non-empty header"
        );

        // Check that section results have the correct headers.
        let headers: Vec<&str> = results.iter().map(|r| r.header.as_str()).collect();
        let has_impl = headers.iter().any(|h| h.contains("Implementation"));
        let has_test = headers.iter().any(|h| h.contains("Testing"));
        assert!(
            has_impl || has_test,
            "expected section headers from the task body, got: {:?}",
            headers
        );
    }

    #[test]
    fn test_find_empty_index() {
        let dir = tempfile::tempdir().unwrap();
        let (mgr, _calls) = test_manager(dir.path());

        // No sync — index is empty.
        let results = mgr.find("anything", 10).unwrap();
        assert!(
            results.is_empty(),
            "expected empty results from empty index, got {} results",
            results.len()
        );
    }

    #[test]
    fn test_clear_removes_index() {
        let dir = tempfile::tempdir().unwrap();
        let (mut mgr, _calls) = test_manager(dir.path());

        let tasks = vec![
            make_test_task(1, "Task one", "Body one"),
            make_test_task(2, "Task two", "Body two"),
        ];

        mgr.sync(&tasks).unwrap();
        assert!(mgr.doc_count() > 0);

        mgr.clear().unwrap();

        assert_eq!(mgr.doc_count(), 0, "doc_count should be 0 after clear");
    }

    // -----------------------------------------------------------------------
    // get_status tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_status_disabled_default_config() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::new_default("test");
        cfg.set_dir(dir.path().to_path_buf());

        let status = get_status(&cfg);
        assert!(!status.enabled);
        assert_eq!(status.documents, 0);
        assert!(status.index_file.is_empty());
        assert_eq!(status.provider, "voyage");
        assert!(status.model.is_empty());
        assert_eq!(status.file_size_bytes, 0);
        assert!(status.last_sync.is_empty());
    }

    #[test]
    fn test_get_status_enabled_no_index() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::new_default("test");
        cfg.set_dir(dir.path().to_path_buf());
        cfg.semantic_search.enabled = true;
        cfg.semantic_search.provider = "voyage".to_string();
        cfg.semantic_search.model = "voyage-3-lite".to_string();

        let status = get_status(&cfg);
        assert!(status.enabled);
        assert_eq!(status.provider, "voyage");
        assert_eq!(status.model, "voyage-3-lite");
        assert_eq!(status.documents, 0);
        assert!(status.index_file.is_empty());
    }

    #[test]
    fn test_get_status_with_populated_index() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join(INDEX_FILE);

        // Create and populate a SQLite index.
        let store = SqliteStore::open(&index_path).unwrap();
        let docs: Vec<Document> = (0..3)
            .map(|i| Document {
                id: format!("{}:0", i + 1),
                content: format!("task {}", i + 1),
                content_hash: format!("hash_{}", i + 1),
                vector: vec![i as f32, 1.0],
                metadata: HashMap::new(),
            })
            .collect();
        sembed_rs::Store::upsert(&store, &docs).unwrap();
        drop(store);

        let mut cfg = Config::new_default("test");
        cfg.set_dir(dir.path().to_path_buf());
        cfg.semantic_search.enabled = true;
        cfg.semantic_search.provider = "openai".to_string();
        cfg.semantic_search.model = "text-embedding-3-small".to_string();

        let status = get_status(&cfg);
        assert!(status.enabled);
        assert_eq!(status.documents, 3);
        assert!(!status.index_file.is_empty());
        assert!(status.file_size_bytes > 0);
        assert!(!status.last_sync.is_empty());
        assert!(
            chrono::DateTime::parse_from_rfc3339(&status.last_sync).is_ok(),
            "last_sync should be valid RFC 3339: {}",
            status.last_sync
        );
    }

    #[test]
    fn test_get_status_disabled_with_existing_index() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join(INDEX_FILE);

        let store = SqliteStore::open(&index_path).unwrap();
        sembed_rs::Store::upsert(
            &store,
            &[Document {
                id: "1:0".to_string(),
                content: "task".to_string(),
                content_hash: "h".to_string(),
                vector: vec![1.0],
                metadata: HashMap::new(),
            }],
        )
        .unwrap();
        drop(store);

        let mut cfg = Config::new_default("test");
        cfg.set_dir(dir.path().to_path_buf());

        let status = get_status(&cfg);
        assert!(!status.enabled);
        assert_eq!(status.documents, 0, "disabled should skip document count");
        assert!(!status.index_file.is_empty(), "file metadata still reported");
        assert!(status.file_size_bytes > 0, "file size still reported");
    }

    #[test]
    fn test_embed_status_serializes_to_json() {
        let status = EmbedStatus {
            enabled: true,
            provider: "voyage".to_string(),
            model: "voyage-3".to_string(),
            dimensions: 1024,
            index_file: "/path/to/.embeddings.db".to_string(),
            documents: 42,
            file_size_bytes: 8192,
            last_sync: "2026-03-01T12:00:00Z".to_string(),
        };

        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["enabled"], true);
        assert_eq!(json["provider"], "voyage");
        assert_eq!(json["model"], "voyage-3");
        assert_eq!(json["documents"], 42);
        assert_eq!(json["file_size_bytes"], 8192);
        assert_eq!(json["last_sync"], "2026-03-01T12:00:00Z");
        assert_eq!(json["index_file"], "/path/to/.embeddings.db");
    }
}
