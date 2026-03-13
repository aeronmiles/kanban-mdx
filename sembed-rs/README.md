# sembed-rs

Minimal, provider-agnostic semantic embeddings library for Rust. Embed text, store vectors, search by cosine similarity — with any provider or your own.

## Features

- **Single-trait abstraction** — implement `Embedder` (one method: `embed(&[String]) -> Vec<Vector>`) to plug in any provider
- **Built-in providers** — OpenAI, Voyage AI, Ollama, and any OpenAI-compatible endpoint out of the box
- **Thread-safe in-memory index** — brute-force cosine similarity search over stored documents, safe for concurrent reads and writes via `RwLock`
- **JSON persistence** — `save`/`load` to any `io::Write`/`io::Read` (files, buffers, network streams)
- **Staleness detection** — SHA-256 content hashing to track which documents need re-embedding
- **Vector math utilities** — `cosine_similarity`, `dot`, `magnitude`, `normalize`
- **Deterministic mock embedder** — hash-based vector generation for tests, no network required

## Installation

```toml
[dependencies]
sembed-rs = { git = "https://github.com/aeronmiles/sembed-rs.git" }
```

## Quick start

```rust
use sembed_rs::{Index, Document, OpenAICompatible, OpenAIConfig, Embedder, cosine_similarity};
use std::collections::HashMap;

// 1. Create an embedder
let embedder = OpenAICompatible::new(OpenAIConfig {
    base_url: "https://api.openai.com/v1".to_string(),
    api_key: std::env::var("OPENAI_API_KEY").unwrap(),
    model: "text-embedding-3-small".to_string(),
    dimensions: Some(256),
    input_type: None,
    timeout_secs: None,
})?;

// 2. Embed some text
let vectors = embedder.embed(&[
    "Rust is a systems programming language".to_string(),
    "Python is great for data science".to_string(),
])?;

// 3. Store in an index
let index = Index::new();
index.add(vec![
    Document {
        id: "doc-1".to_string(),
        content: "Rust is a systems programming language".to_string(),
        content_hash: sembed_rs::content_hash("Rust is a systems programming language"),
        vector: vectors[0].clone(),
        metadata: HashMap::new(),
    },
    Document {
        id: "doc-2".to_string(),
        content: "Python is great for data science".to_string(),
        content_hash: sembed_rs::content_hash("Python is great for data science"),
        vector: vectors[1].clone(),
        metadata: HashMap::new(),
    },
]);

// 4. Search
let query_vec = embedder.embed(&["programming languages".to_string()])?;
let results = index.search(&query_vec[0], 5);
for r in &results {
    println!("{}: {:.4}", r.document.id, r.score);
}

// 5. Persist to disk
let file = std::fs::File::create("embeddings.json")?;
index.save(file)?;
```

## Providers

### OpenAI

```rust
use sembed_rs::openai;

let embedder = openai("sk-...", "text-embedding-3-small", Some(256))?;
let vectors = embedder.embed(&["hello world".to_string()])?;
```

### Voyage AI

```rust
use sembed_rs::openai::voyage_ai;

// Voyage supports input_type: "document" for indexing, "query" for search
let doc_embedder = voyage_ai("pa-...", "voyage-3-lite", Some("document"))?;
let query_embedder = voyage_ai("pa-...", "voyage-3-lite", Some("query"))?;
```

### Ollama (local)

```rust
use sembed_rs::{Ollama, OllamaConfig};

let embedder = Ollama::new(OllamaConfig {
    base_url: "http://localhost:11434".to_string(), // empty string also defaults to this
    model: "nomic-embed-text".to_string(),
    timeout_secs: None,
})?;
```

### Custom OpenAI-compatible endpoint

```rust
use sembed_rs::{OpenAICompatible, OpenAIConfig};

let embedder = OpenAICompatible::new(OpenAIConfig {
    base_url: "https://my-provider.example.com/v1".to_string(),
    api_key: "my-key".to_string(),
    model: "my-model".to_string(),
    dimensions: None,
    input_type: None,
    timeout_secs: Some(60),
})?;
```

### Implementing your own

```rust
use sembed_rs::{Embedder, EmbedError, Vector};

struct MyEmbedder;

impl Embedder for MyEmbedder {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vector>, EmbedError> {
        // Your implementation here.
        // Return one vector per input text, in the same order.
        todo!()
    }
}
```

The `Embedder` trait requires `Send + Sync`, so implementations must be safe for concurrent use.

## Index

The `Index` is a flat in-memory vector store with brute-force cosine similarity search.

```rust
use sembed_rs::{Index, Document};
use std::collections::HashMap;

let index = Index::new();

// Add documents (upsert semantics — same ID replaces)
index.add(vec![Document {
    id: "1".to_string(),
    content: "hello".to_string(),
    content_hash: sembed_rs::content_hash("hello"),
    vector: vec![1.0, 0.0, 0.0],
    metadata: HashMap::from([("source".to_string(), "greeting".to_string())]),
}]);

// Search by vector
let results = index.search(&vec![1.0, 0.0, 0.0], 10);

// Get by ID
let doc = index.get("1");

// Remove by ID
index.remove(&["1".to_string()]);

// Check size
println!("docs: {}, empty: {}", index.len(), index.is_empty());
```

### Staleness detection

Track which documents need re-embedding after content changes:

```rust
use std::collections::HashMap;

let mut current_hashes = HashMap::new();
current_hashes.insert("1".to_string(), sembed_rs::content_hash("updated content"));
current_hashes.insert("3".to_string(), sembed_rs::content_hash("new document"));

// Returns IDs that are: stale (hash mismatch), missing (not in index), or orphaned (not in current)
let stale_ids = index.stale(&current_hashes);
```

### Persistence

```rust
// Save to any io::Write
let file = std::fs::File::create("index.json")?;
index.save(file)?;

// Load from any io::Read (merges into existing index)
let file = std::fs::File::open("index.json")?;
index.load(file)?;

// In-memory roundtrip
let mut buf = Vec::new();
index.save(&mut buf)?;
index.load(buf.as_slice())?;
```

The JSON format stores all documents with their vectors, content, hashes, and metadata. Loading merges into the existing index using upsert semantics.

## Vector math

Standalone utility functions for working with embedding vectors:

```rust
use sembed_rs::{cosine_similarity, dot, magnitude, normalize};

let a = vec![1.0, 2.0, 3.0];
let b = vec![4.0, 5.0, 6.0];

cosine_similarity(&a, &b);  // similarity in [-1, 1]
dot(&a, &b);                // 32.0
magnitude(&a);              // 3.7416...
normalize(&a);              // Some(unit vector) or None if zero/empty
```

All functions handle edge cases gracefully: mismatched lengths return `0.0`, empty/zero vectors return `0.0` or `None`.

## Content hashing

SHA-256 based hashing for staleness detection:

```rust
let hash = sembed_rs::content_hash("my document text");
// => "b9e68e1b1e..."  (64-char hex string)
```

Deterministic — same input always produces the same hash. Use this to track whether a document's content has changed since it was last embedded.

## Testing

The `MockEmbedder` generates deterministic vectors from text content using hashing — no API calls needed:

```rust
use sembed_rs::MockEmbedder;

let mock = MockEmbedder::new(128); // 128-dimensional vectors

let vecs = mock.embed(&["hello".to_string()]).unwrap();
assert_eq!(vecs[0].len(), 128);

// Same text always produces the same vector
let vecs2 = mock.embed(&["hello".to_string()]).unwrap();
assert_eq!(vecs[0], vecs2[0]);

// Inspect call history
let calls = mock.calls.lock().unwrap();
assert_eq!(calls.len(), 2);
```

Vectors are normalized to unit length so cosine similarity works correctly in tests.

## Dependencies

| Crate | Purpose |
|-------|---------|
| `serde` + `serde_json` | Document serialization and index persistence |
| `sha2` | Content hashing (SHA-256) |
| `ureq` | HTTP client for embedding API calls |
| `thiserror` | Error type derivation |

## License

MIT
