//! Flat in-memory vector index with brute-force cosine search.
//!
//! Thread-safe via `RwLock`. Supports JSON persistence via `save`/`load`.

use std::collections::HashMap;
use std::io;
use std::sync::RwLock;

use crate::vector::cosine_similarity;
use crate::{Document, SearchResult, Vector};

/// Serialization wrapper for the index JSON format.
#[derive(serde::Serialize, serde::Deserialize)]
struct IndexJson {
    documents: Vec<Document>,
}

/// A flat in-memory vector index with brute-force cosine search.
///
/// Thread-safe for concurrent use. Documents are keyed by ID with upsert
/// semantics.
pub struct Index {
    docs: RwLock<HashMap<String, Document>>,
}

impl Index {
    /// Creates an empty index.
    pub fn new() -> Self {
        Self {
            docs: RwLock::new(HashMap::new()),
        }
    }

    /// Inserts or updates documents by ID (upsert semantics).
    pub fn add(&self, docs: Vec<Document>) {
        let mut map = self.docs.write().unwrap();
        for doc in docs {
            map.insert(doc.id.clone(), doc);
        }
    }

    /// Removes documents by ID. Missing IDs are silently ignored.
    pub fn remove(&self, ids: &[String]) {
        let mut map = self.docs.write().unwrap();
        for id in ids {
            map.remove(id);
        }
    }

    /// Returns a document by ID, if found.
    pub fn get(&self, id: &str) -> Option<Document> {
        let map = self.docs.read().unwrap();
        map.get(id).cloned()
    }

    /// Returns the top-k documents most similar to the query vector,
    /// sorted by descending cosine similarity.
    ///
    /// Returns an empty vec if k <= 0 or the index is empty.
    pub fn search(&self, query: &Vector, k: usize) -> Vec<SearchResult> {
        if k == 0 {
            return Vec::new();
        }
        let map = self.docs.read().unwrap();
        if map.is_empty() {
            return Vec::new();
        }

        let mut results: Vec<SearchResult> = map
            .values()
            .map(|doc| {
                let score = cosine_similarity(query, &doc.vector);
                SearchResult {
                    document: doc.clone(),
                    score,
                }
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        results.truncate(k);
        results
    }

    /// Returns document IDs where the stored content_hash differs from
    /// the corresponding value in `current`.
    ///
    /// IDs in the index but not in `current` are considered stale.
    /// IDs in `current` but not in the index are also returned (missing).
    pub fn stale(&self, current: &HashMap<String, String>) -> Vec<String> {
        let map = self.docs.read().unwrap();
        let mut stale = Vec::new();

        // Check for stale/missing documents.
        for (id, hash) in current {
            match map.get(id) {
                Some(doc) if doc.content_hash != *hash => stale.push(id.clone()),
                None => stale.push(id.clone()),
                _ => {}
            }
        }

        // Orphaned documents (in index but not in current).
        for id in map.keys() {
            if !current.contains_key(id) {
                stale.push(id.clone());
            }
        }

        stale
    }

    /// Returns a copy of all documents (no guaranteed order).
    pub fn all(&self) -> Vec<Document> {
        let map = self.docs.read().unwrap();
        map.values().cloned().collect()
    }

    /// Returns the number of documents in the index.
    pub fn len(&self) -> usize {
        self.docs.read().unwrap().len()
    }

    /// Returns true if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.docs.read().unwrap().is_empty()
    }

    /// Saves the index as JSON to a writer.
    pub fn save(&self, writer: impl io::Write) -> Result<(), io::Error> {
        let map = self.docs.read().unwrap();
        let docs: Vec<Document> = map.values().cloned().collect();
        let wrapper = IndexJson { documents: docs };
        serde_json::to_writer(writer, &wrapper)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    /// Loads documents from a JSON reader and merges them into the index (upsert).
    pub fn load(&self, reader: impl io::Read) -> Result<(), io::Error> {
        let wrapper: IndexJson = serde_json::from_reader(reader)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut map = self.docs.write().unwrap();
        for doc in wrapper.documents {
            map.insert(doc.id.clone(), doc);
        }
        Ok(())
    }
}

impl Default for Index {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.len();
        f.debug_struct("Index").field("len", &len).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_doc(id: &str, content: &str, hash: &str, vec: Vec<f32>) -> Document {
        Document {
            id: id.to_string(),
            content: content.to_string(),
            content_hash: hash.to_string(),
            vector: vec,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn add_and_get() {
        let idx = Index::new();
        assert!(idx.is_empty());

        idx.add(vec![make_doc("1", "hello", "abc", vec![1.0, 0.0])]);
        assert_eq!(idx.len(), 1);
        assert!(idx.get("1").is_some());
        assert!(idx.get("2").is_none());
    }

    #[test]
    fn add_replaces() {
        let idx = Index::new();
        idx.add(vec![make_doc("1", "old", "abc", vec![1.0, 0.0])]);
        idx.add(vec![make_doc("1", "new", "def", vec![0.0, 1.0])]);

        assert_eq!(idx.len(), 1);
        assert_eq!(idx.get("1").unwrap().content, "new");
    }

    #[test]
    fn remove() {
        let idx = Index::new();
        idx.add(vec![
            make_doc("1", "a", "a", vec![1.0]),
            make_doc("2", "b", "b", vec![2.0]),
        ]);

        idx.remove(&["1".to_string()]);
        assert_eq!(idx.len(), 1);
        assert!(idx.get("1").is_none());
        assert!(idx.get("2").is_some());
    }

    #[test]
    fn search_top_k() {
        let idx = Index::new();
        idx.add(vec![
            make_doc("1", "a", "a", vec![1.0, 0.0]),
            make_doc("2", "b", "b", vec![0.0, 1.0]),
            make_doc("3", "c", "c", vec![0.7, 0.7]),
        ]);

        let results = idx.search(&vec![1.0, 0.0], 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].document.id, "1");
    }

    #[test]
    fn search_empty() {
        let idx = Index::new();
        assert!(idx.search(&vec![1.0], 5).is_empty());
    }

    #[test]
    fn search_k_zero() {
        let idx = Index::new();
        idx.add(vec![make_doc("1", "a", "a", vec![1.0])]);
        assert!(idx.search(&vec![1.0], 0).is_empty());
    }

    #[test]
    fn stale_detection() {
        let idx = Index::new();
        idx.add(vec![
            make_doc("1", "a", "hash_a", vec![1.0]),
            make_doc("2", "b", "hash_b", vec![2.0]),
        ]);

        let mut expected = HashMap::new();
        expected.insert("1".to_string(), "hash_a_new".to_string()); // stale
        expected.insert("3".to_string(), "hash_c".to_string()); // missing

        let stale = idx.stale(&expected);
        assert!(stale.contains(&"1".to_string())); // stale hash
        assert!(stale.contains(&"3".to_string())); // missing
        assert!(stale.contains(&"2".to_string())); // orphaned
    }

    #[test]
    fn save_and_load() {
        let idx = Index::new();
        idx.add(vec![make_doc("1", "hello", "abc", vec![1.0, 2.0, 3.0])]);

        let mut buf = Vec::new();
        idx.save(&mut buf).unwrap();

        let idx2 = Index::new();
        idx2.load(buf.as_slice()).unwrap();
        assert_eq!(idx2.len(), 1);
        let doc = idx2.get("1").unwrap();
        assert_eq!(doc.content, "hello");
        assert_eq!(doc.vector, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn load_merges() {
        let idx = Index::new();
        idx.add(vec![make_doc("1", "existing", "a", vec![1.0])]);

        let idx_src = Index::new();
        idx_src.add(vec![make_doc("2", "new", "b", vec![2.0])]);
        let mut buf = Vec::new();
        idx_src.save(&mut buf).unwrap();

        idx.load(buf.as_slice()).unwrap();
        assert_eq!(idx.len(), 2);
        assert!(idx.get("1").is_some());
        assert!(idx.get("2").is_some());
    }
}
