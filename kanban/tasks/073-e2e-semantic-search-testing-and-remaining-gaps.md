---
id: 73
title: 'E2E: Complete semantic search testing and fix remaining gaps'
status: done
priority: high
created: 2026-03-12T18:00:00Z
updated: 2026-03-12T17:32:26.298744Z
started: 2026-03-12T17:32:26.298744Z
completed: 2026-03-12T17:32:26.298744Z
tags:
    - kanban-mdx
class: standard
---

End-to-end audit and completion of the semantic search feature across CLI, TUI, and the sembed crate. The infrastructure is largely built but has no integration/e2e tests, and the `SearchFilter` parser doesn't handle the `~` prefix despite the TUI `App` already having async semantic dispatch code.

## Current state summary

### What works (production code exists, unit-tested)

- **sembed crate** (`sembed/src/`): Complete provider-agnostic embedding library — `Embedder` trait, `OpenAICompatible` (OpenAI/Voyage/custom), `Ollama`, `Index` (flat in-memory brute-force cosine similarity), JSON persistence, content hashing. All core functions have unit tests.
- **Embedding manager** (`src/embed/mod.rs`): `Manager` struct coordinates chunking, batch embedding (max 512), staleness detection, sync, `search()` (task-level), `find()` (section-level). Unit-tested for index ops, hashing, staleness, save/load.
- **Task chunking** (`src/embed/chunk.rs`): Splits tasks by `##`/`###` markdown sections with line number metadata. Unit-tested.
- **CLI `embed sync|status|clear`** (`src/cli/embed.rs`): All three subcommands fully implemented with table/compact/json output. No integration tests.
- **CLI `find`** (`src/cli/find.rs`): Fully implemented — calls `Manager::new()`, `sync()`, `find()`, renders results in table/compact/json. Registered at `Commands::Find` in `src/cli/root.rs`. **Note: Task #69 describes this as a stub, but it is actually complete production code.** No integration tests.
- **TUI async semantic search** (`src/tui/app.rs`):
  - `is_semantic_query()` / `sem_query_text()` / `dsl_portion()` — parse `~` from query strings
  - `on_search_query_changed()` — arms 300ms debounce when `~` detected
  - `tick_semantic_debounce()` — fires `fire_semantic()` after debounce
  - `fire_semantic_board_search()` — spawns background thread, calls `Manager::new()` + `search()`
  - `fire_sem_detail_find()` — spawns background thread, calls `Manager::new()` + `find()`
  - `filtered_tasks()` — applies `sem_ids` intersection with DSL-filtered tasks
  - Semantic results received via `mpsc` channels, applied on next tick
  - Theme colors for semantic highlights exist (`theme.rs` semantic colours)
  - `f` key opens board search, `Ctrl+F` opens detail find — both paths support `~`
- **Config** (`src/model/config.rs`): `SemanticSearchConfig` with `enabled`, `provider`, `model`, `base_url`, `dimensions`, `input_type`. API key via `KANBAN_EMBED_API_KEY` env var.

### What's missing / incomplete

1. **No mock/fake `Embedder`** — all unit tests stop at the `Manager` boundary because `embed()` requires a real API call. There is no `MockEmbedder` implementing the `Embedder` trait for testing.

2. **`SearchFilter::parse()` ignores `~` prefix** (`src/tui/search.rs:58-76`) — tokens starting with `~` fall through to free-text matching. The `SearchFilter` struct has no `semantic_query: Option<String>` field. The TUI `App` works around this by calling `is_semantic_query()` / `sem_query_text()` directly on the raw query string, so the actual filtering works, but the DSL parser is incomplete.

3. **No integration tests anywhere** for the semantic search flow:
   - No test for `embed sync` → `embed status` → `find <query>` CLI pipeline
   - No test for `embed sync` → `embed clear` idempotency
   - No test for `find` with an empty/missing index (error path)
   - No test for `find` with a stale index (auto-resync on find)
   - No TUI behavioral test for `~` prefix search (debounce, async result display)

4. **No e2e test infrastructure for embedding-dependent features** — the project has no e2e test directory at all (unlike the Go side which has `e2e/cli_test.go`). The Rust side does all testing via in-module `#[cfg(test)]` blocks.

5. **Task #69 is stale** — describes `find` as a stub, but it's fully implemented. Should be updated or closed.

## Implementation plan

### Phase 1: Test infrastructure (prerequisite for everything else)

**1a. Add `MockEmbedder` to sembed crate**

File: `sembed/src/mock.rs` (gated behind `#[cfg(test)]` or a `test-utils` feature flag)

```rust
/// A deterministic mock embedder that produces stable, distinguishable vectors.
/// Uses a simple hash of the input text to generate vectors, so identical text
/// always produces identical vectors and similar text produces related vectors.
pub struct MockEmbedder {
    pub dimensions: usize,
    pub calls: Arc<Mutex<Vec<Vec<String>>>>,  // record calls for assertions
}

impl Embedder for MockEmbedder {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vector>, EmbedError> {
        // Produce deterministic vectors from text content.
        // Strategy: hash each text, use hash bytes to seed vector values.
        // This gives stable, reproducible results without network calls.
    }
}
```

Key requirements:
- Deterministic: same text → same vector every time
- Distinguishable: "error handling" and "database migration" should produce different vectors
- No network calls
- Records call history for test assertions

**1b. Add `Manager::with_embedders()` constructor**

Currently `Manager::new()` creates embedders from config, requiring API keys. Add:

```rust
impl Manager {
    /// Test-only constructor that accepts pre-built embedders.
    #[cfg(test)]
    pub fn with_embedders(
        doc_embedder: Box<dyn Embedder>,
        query_embedder: Box<dyn Embedder>,
        index_path: PathBuf,
    ) -> Self { ... }
}
```

### Phase 2: Integration tests for embed manager

File: `src/embed/mod.rs` (extend existing `#[cfg(test)]` block)

Tests to add:
- `test_sync_embeds_all_tasks` — create tasks, sync, verify index doc count matches expected chunk count
- `test_sync_detects_stale` — sync, modify a task, re-sync, verify only changed task is re-embedded (check `MockEmbedder.calls`)
- `test_sync_prunes_deleted` — sync, remove a task, re-sync, verify old chunks are pruned
- `test_search_returns_ranked_results` — sync 5+ tasks, search, verify results sorted by score descending
- `test_find_returns_section_metadata` — sync tasks with sections, find, verify `header`, `line`, `chunk` fields populated
- `test_find_empty_index` — find on empty index returns empty vec (no error)
- `test_clear_removes_index_file` — sync, clear, verify file deleted and doc count is 0
- `test_save_load_roundtrip_with_chunks` — sync, save, create new Manager loading same file, verify same results

### Phase 3: CLI integration tests

Create `tests/cli_embed.rs` (or extend an existing integration test file if one exists).

Strategy: build the binary, run it against a temp kanban directory with `semantic_search.enabled: true` and `MockEmbedder` (or use Ollama if available, with a skip-if-unavailable guard).

Alternatively, test at the function level by calling `find::run()` and `embed::run()` directly with constructed `Cli` args and a temp directory.

Tests:
- `test_find_without_config_errors` — `find` with `semantic_search.enabled: false` → exit code 1, error message
- `test_embed_sync_creates_index` — sync → `.embeddings.json` exists
- `test_embed_status_reports_counts` — sync → status shows correct doc count
- `test_embed_clear_removes_index` — sync → clear → status shows "not created"
- `test_find_renders_table` — sync → find → output contains expected columns (ID, SCORE, HEADER, LINE, TITLE)
- `test_find_renders_json` — sync → find --json → valid JSON with expected keys
- `test_find_renders_compact` — sync → find --compact → one-line-per-result format

### Phase 4: TUI behavioral tests for `~` prefix

File: `src/tui/search.rs` (extend `#[cfg(test)]` block) and potentially `src/tui/app.rs`.

Tests:
- `test_is_semantic_query` — `"~error handling"` → true, `"plain text"` → false, `"p:high ~error"` → true
- `test_sem_query_text_extraction` — `"p:high ~error handling"` → `"error handling"`
- `test_dsl_portion_extraction` — `"p:high @48h ~error"` → `"p:high @48h"`
- `test_search_filter_with_semantic` — (once SearchFilter is updated) verify `semantic_query` field is populated

### Phase 5: Fix `SearchFilter` to formally parse `~` (optional but recommended)

Update `SearchFilter` to extract the semantic query text:

```rust
pub struct SearchFilter {
    // ... existing fields ...
    /// Semantic search query (text after `~`), if present.
    pub semantic_query: Option<String>,
}
```

Update `parse()` to split on `~` before tokenizing DSL:
```rust
pub fn parse(query: &str) -> Self {
    let (dsl_part, semantic_part) = match query.find('~') {
        Some(pos) => (&query[..pos], Some(query[pos+1..].trim().to_string())),
        None => (query, None),
    };
    // ... parse dsl_part as before ...
    filter.semantic_query = semantic_part.filter(|s| !s.is_empty());
    filter
}
```

This would let the TUI `App` use `SearchFilter` directly instead of calling `is_semantic_query()` / `sem_query_text()` as separate functions.

## Key files to modify

| File | Change |
|------|--------|
| `sembed/src/lib.rs` | Re-export `MockEmbedder` under `#[cfg(test)]` |
| `sembed/src/mock.rs` | New file: `MockEmbedder` implementation |
| `src/embed/mod.rs` | Add `Manager::with_embedders()`, add integration tests |
| `src/embed/chunk.rs` | No changes expected (already well-tested) |
| `src/cli/find.rs` | No production changes; add function-level tests |
| `src/cli/embed.rs` | No production changes; add function-level tests |
| `src/tui/search.rs` | Optional: add `semantic_query` to `SearchFilter`, add tests |
| `src/tui/app.rs` | Optional: use `SearchFilter.semantic_query` instead of raw string parsing |
| `tests/cli_embed.rs` | New file: CLI integration tests (if integration test pattern is adopted) |

## Acceptance criteria

- [ ] `MockEmbedder` exists in sembed crate, produces deterministic vectors without network
- [ ] `Manager::with_embedders()` allows test construction without API keys
- [ ] Integration tests cover: sync (fresh + stale + pruned), search ranking, find with section metadata, clear, save/load roundtrip
- [ ] CLI tests cover: `find` error paths, output formats (table/compact/json), `embed sync/status/clear` pipeline
- [ ] TUI tests cover: `is_semantic_query`, `sem_query_text`, `dsl_portion` helpers
- [ ] All tests pass with `cargo test` (no network/API key required)
- [ ] Task #69 updated or closed (find command is already implemented)

## Related tasks

- #2 (done): sembed library implementation
- #3 (done): sembed integration into kanban-md config and search
- #35 (done): TUI semantic search integration — async dispatch is implemented
- #63 (archived): TUI inline ~prefix — documented but `SearchFilter` doesn't formally parse it
- #69 (todo, stale): CLI find command — actually already implemented, needs tests not code

Completed via 2-wave orchestration (4 agents). 33 new tests added (347→380). Wave 1: MockEmbedder + SearchFilter ~ parsing. Wave 2: Manager integration tests + CLI tests. All pass without network/API keys.
