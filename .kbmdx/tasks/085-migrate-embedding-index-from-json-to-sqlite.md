---
id: 85
title: Migrate embedding index from JSON to SQLite
status: done
priority: high
created: '2026-03-14T08:24:49.518935Z'
updated: '2026-03-14T11:49:50.650617Z'
tags:
- sembed
- persistence
- embeddings
claimed_by: claude
claimed_at: '2026-03-14T08:39:52.562513Z'
---

## Motivation

The current embedding index (.embeddings.json) is a single JSON file containing all documents with their vectors serialized as JSON number arrays. This works at current scale (50–500 tasks, <1 MB) but has three real problems:

1. **No atomic writes.** Every sync rewrites the entire file via serde_json::to_writer. A crash mid-write corrupts the index — no recovery, full re-embed required.
2. **Full-file rewrite on every change.** Updating one chunk re-serializes all chunks. At 500 tasks × 5 chunks × 256 dims, that's ~3.5 MB rewritten for a one-line edit.
3. **Wasteful encoding.** JSON float arrays use ~12 bytes per f32 (vs 4 bytes binary). The content field duplicates text already in task .md files.

SQLite solves all three: WAL mode gives atomic writes, row-level SQL gives incremental updates, and BLOB columns store vectors at native density.

## Design

### Scope

Replace the persistence backend in sembed-rs's Index. The in-memory data model (HashMap<String, Document> behind RwLock), the public API surface, and the brute-force cosine search all stay exactly the same. Only save/load and the on-disk format change.

### Non-goals

- ANN/HNSW indexing (brute-force is <1ms at current scale)
- Changing the Embedder trait or provider logic
- Changing the Manager chunking/sync logic in kanban-mdx
- Supporting concurrent writer processes (single-writer is fine)

### Schema

```sql
CREATE TABLE IF NOT EXISTS documents (
    id          TEXT PRIMARY KEY,
    content     TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    vector      BLOB NOT NULL,
    metadata    TEXT  -- JSON, nullable (empty = no metadata)
);

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- meta row: ("schema_version", "1")
```

- **vector BLOB**: packed little-endian f32 array. Encode: `bytemuck::cast_slice::<f32, u8>(&v)`. Decode: `bytemuck::cast_slice::<u8, f32>(blob)`.
- **metadata TEXT**: JSON object or NULL. Matches current serde behavior (skip_serializing_if = "HashMap::is_empty").
- **meta table**: schema version for future migrations.

### File naming

`.embeddings.db` (replaces `.embeddings.json`). Update INDEX_FILE const in embed/mod.rs.

### Persistence trait (new)

Abstract the persistence backend behind a trait so the Index is not coupled to SQLite directly. This also keeps the door open for future backends and simplifies testing.

```rust
pub trait Store: Send + Sync {
    fn load_all(&self) -> Result<Vec<Document>, StoreError>;
    fn upsert(&self, docs: &[Document]) -> Result<(), StoreError>;
    fn remove(&self, ids: &[String]) -> Result<(), StoreError>;
    fn count(&self) -> Result<usize, StoreError>;
}
```

Provide two implementations:
- `SqliteStore` — production (rusqlite)
- `MemoryStore` — testing (current HashMap behavior, no persistence)

### Index changes

- `Index::new()` takes no store — remains a pure in-memory index (unchanged API)
- New: `Index::with_store(store: Box<dyn Store>)` — loads from store on construction, auto-persists on add/remove
- `save()` and `load()` remain for backward compat but are soft-deprecated (only used by MemoryStore / tests)
- Search, stale, get, all, len, is_empty — unchanged

### Migration

On Manager::new(), if .embeddings.json exists and .embeddings.db does not:
1. Load JSON index (existing code path)
2. Write all documents to new SQLite store
3. Rename .embeddings.json → .embeddings.json.bak
4. Log migration to activity log

If both exist, prefer .db. If neither exists, create fresh .db.

### Atomic write strategy

- Open SQLite in WAL mode (PRAGMA journal_mode=WAL)
- Batch upserts in a single transaction (BEGIN/INSERT OR REPLACE/COMMIT)
- Crash during transaction → SQLite auto-rollback, no corruption
- No need for temp-file-then-rename dance

## Implementation Plan

### Phase 1: Add rusqlite to sembed-rs

- [ ] Add `rusqlite = { version = "0.34", features = ["bundled"] }` to sembed-rs/Cargo.toml
- [ ] Add `bytemuck = { version = "1", features = ["derive"] }` for zero-copy vector casting
- [ ] Verify cargo build still works, check binary size delta

### Phase 2: Implement Store trait and SqliteStore

- [ ] Define `Store` trait in sembed-rs/src/store.rs
- [ ] Define `StoreError` enum (Io, Sqlite, Corrupt)
- [ ] Implement `SqliteStore` in sembed-rs/src/store/sqlite.rs
  - [ ] new(path) → open/create DB, run CREATE TABLE IF NOT EXISTS, set WAL mode
  - [ ] load_all() → SELECT * FROM documents, decode BLOBs
  - [ ] upsert(docs) → BEGIN + INSERT OR REPLACE in batches of 500 + COMMIT
  - [ ] remove(ids) → DELETE FROM documents WHERE id IN (...)
  - [ ] count() → SELECT count(*) FROM documents
- [ ] Implement `MemoryStore` in sembed-rs/src/store/memory.rs (wraps Vec<Document>)
- [ ] Unit tests for SqliteStore: round-trip, upsert semantics, remove, crash safety (drop mid-transaction)
- [ ] Unit tests for vector BLOB encoding: f32 round-trip, endianness, empty vector

### Phase 3: Wire Store into Index

- [ ] Add Index::with_store(store) constructor
- [ ] On with_store: call store.load_all(), populate HashMap
- [ ] On add(): write to HashMap + store.upsert()
- [ ] On remove(): remove from HashMap + store.remove()
- [ ] Keep save()/load() for JSON compat (used by MemoryStore, tests)
- [ ] All existing Index tests pass unchanged (they use Index::new() which has no store)
- [ ] New tests: Index::with_store(SqliteStore) end-to-end

### Phase 4: Integrate into kanban-mdx Manager

- [ ] Update Manager::new() to create SqliteStore(.embeddings.db)
- [ ] Update Manager::new() to pass store to Index::with_store()
- [ ] Remove manual save() calls from Manager::sync() (store auto-persists)
- [ ] Update INDEX_FILE const to ".embeddings.db"
- [ ] Implement JSON→SQLite migration path in Manager::new()
- [ ] Update Manager::clear() to drop all rows + delete .db file
- [ ] Update embed status CLI output to show .db file size

### Phase 5: Update tests and cleanup

- [ ] Update all integration tests in src/embed/ that reference .embeddings.json
- [ ] Verify embed sync / embed clear / find / search all work end-to-end
- [ ] Add .embeddings.db to default .gitignore entries
- [ ] Remove .embeddings.json from default .gitignore entries (keep for migration)
- [ ] Update README embed section to reference SQLite
- [ ] cargo test --workspace passes

## Dependencies

- `rusqlite 0.34` with `bundled` feature (statically links SQLite, no system dep)
- `bytemuck 1` with `derive` feature (zero-copy BLOB ↔ f32 casting)

## Risks

- **Binary size**: rusqlite bundled adds ~2-3 MB. Acceptable for the project.
- **Build time**: SQLite C compilation adds ~10s to clean builds. Incremental builds unaffected.
- **Cross-compilation**: bundled SQLite compiles with cc crate — works on all targets Rust supports, but needs a C compiler in the toolchain.

## Acceptance Criteria

- cargo test --workspace passes
- embed sync creates .embeddings.db (not .json)
- Existing .embeddings.json auto-migrates on first run
- Crash during sync does not corrupt the index (kill -9 test)
- embed clear removes the .db file
- No change to Manager public API
- No change to Index public API (new with_store is additive)
