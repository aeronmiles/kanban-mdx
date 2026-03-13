//! Content hashing for staleness detection.

use sha2::{Digest, Sha256};

/// Returns the SHA-256 hex digest of content.
///
/// Suitable for staleness detection in embedding indexes.
pub fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let h1 = content_hash("hello world");
        let h2 = content_hash("hello world");
        assert_eq!(h1, h2);
        assert!(!h1.is_empty());
    }

    #[test]
    fn different_inputs() {
        let h1 = content_hash("hello");
        let h2 = content_hash("world");
        assert_ne!(h1, h2);
    }
}
