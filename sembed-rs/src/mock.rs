//! Deterministic mock embedder for testing.
//!
//! Produces stable vectors from text content using a simple hash-based strategy.
//! No network calls — suitable for unit and integration tests.

use std::sync::{Arc, Mutex};

use crate::{EmbedError, Embedder, Vector};

/// A deterministic mock embedder.
///
/// Generates vectors by hashing input text, so:
/// - Same text always produces the same vector
/// - Different text produces different (but deterministic) vectors
/// - No API calls or network required
pub struct MockEmbedder {
    pub dimensions: usize,
    /// Records every `embed()` call for test assertions.
    pub calls: Arc<Mutex<Vec<Vec<String>>>>,
}

impl MockEmbedder {
    pub fn new(dimensions: usize) -> Self {
        Self {
            dimensions,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Embedder for MockEmbedder {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vector>, EmbedError> {
        // Record this call.
        self.calls.lock().unwrap().push(texts.to_vec());

        // Generate deterministic vectors from text content.
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            let vec = deterministic_vector(text, self.dimensions);
            results.push(vec);
        }
        Ok(results)
    }
}

/// Generate a deterministic vector from text.
///
/// Uses a simple hash-spread strategy: hash the text bytes combined with the
/// dimension index, then map each hash to a f32 value. The result is
/// normalized to a unit vector so cosine similarity works properly.
fn deterministic_vector(text: &str, dimensions: usize) -> Vector {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut vec = Vec::with_capacity(dimensions);
    for i in 0..dimensions {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        i.hash(&mut hasher);
        let h = hasher.finish();
        // Map to [-1.0, 1.0] range.
        let val = ((h % 20001) as f32 / 10000.0) - 1.0;
        vec.push(val);
    }

    // Normalize to unit vector for cosine similarity to work properly.
    let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for v in &mut vec {
            *v /= magnitude;
        }
    }

    vec
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_same_text() {
        let mock = MockEmbedder::new(8);
        let v1 = mock.embed(&["hello".to_string()]).unwrap();
        let v2 = mock.embed(&["hello".to_string()]).unwrap();
        assert_eq!(v1[0], v2[0]);
    }

    #[test]
    fn test_deterministic_different_text() {
        let mock = MockEmbedder::new(8);
        let v1 = mock.embed(&["hello".to_string()]).unwrap();
        let v2 = mock.embed(&["world".to_string()]).unwrap();
        assert_ne!(v1[0], v2[0]);
    }

    #[test]
    fn test_records_calls() {
        let mock = MockEmbedder::new(4);
        mock.embed(&["a".to_string(), "b".to_string()]).unwrap();
        mock.embed(&["c".to_string()]).unwrap();
        let calls = mock.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], vec!["a", "b"]);
        assert_eq!(calls[1], vec!["c"]);
    }

    #[test]
    fn test_correct_dimensions() {
        let mock = MockEmbedder::new(16);
        let vecs = mock.embed(&["test".to_string()]).unwrap();
        assert_eq!(vecs[0].len(), 16);
    }

    #[test]
    fn test_unit_vectors() {
        let mock = MockEmbedder::new(32);
        let vecs = mock.embed(&["test".to_string()]).unwrap();
        let magnitude: f32 = vecs[0].iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 1e-5);
    }
}
