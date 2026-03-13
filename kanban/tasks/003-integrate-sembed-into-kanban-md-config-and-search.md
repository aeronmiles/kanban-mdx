---
id: 3
title: Integrate sembed into kanban-md config and search
status: done
priority: critical
created: 2026-03-10T10:17:11.052013Z
updated: 2026-03-11T07:13:32.080070Z
started: 2026-03-10T14:12:02.985673Z
completed: 2026-03-10T14:12:02.985673Z
tags:
- layer-4
class: standard
branch: main
---

# Integrate sembed into kanban-md config and search

Wire the sembed library (submodule at sembed/) into kanban-md as an opt-in semantic search feature. Requires config schema changes, migration, and integration into both CLI and TUI search paths.

## References

- Task #1: sembed design spec
- Task #2: sembed library implementation (complete)
- Submodule: sembed/ (github.com/aeronmiles/sembed)

## Implementation Plan

### Phase 1: Config schema (v13 → v14)

**File: internal/config/config.go**
- [ ] Add `SemanticSearchConfig` struct:
  ```go
  type SemanticSearchConfig struct {
      Enabled    bool   `yaml:"enabled"`
      Provider   string `yaml:"provider,omitempty"`   // voyage, openai, ollama, custom
      Model      string `yaml:"model,omitempty"`      // e.g. voyage-4-lite
      BaseURL    string `yaml:"base_url,omitempty"`   // custom endpoint
      Dimensions int    `yaml:"dimensions,omitempty"` // 0 = provider default
      InputType  string `yaml:"input_type,omitempty"` // query/document (Voyage AI)
  }
  ```
- [ ] Add field to Config: `SemanticSearch SemanticSearchConfig \`yaml:"semantic_search,omitempty"\``
- [ ] Add validation in Validate(): provider must be one of voyage/openai/ollama/custom if enabled; model required if enabled; base_url required if provider=custom

**File: internal/config/defaults.go**
- [ ] Bump `CurrentVersion` from 13 to 14
- [ ] Add `DefaultSemanticSearch` (enabled: false, zero values)
- [ ] Include in `NewDefault()`

**File: internal/config/migrate.go**
- [ ] Add `migrateV13ToV14(cfg *Config) error` — sets `SemanticSearch` to disabled defaults, bumps version to 14
- [ ] Register in `migrations` map: `13: migrateV13ToV14`

**File: internal/config/testdata/compat/v13/**
- [ ] Create fixture directory, copy v12 fixture and apply v12→v13 migration result as the v13 fixture config.yml
- [ ] Include sample task files

**File: internal/config/compat_test.go**
- [ ] Add `TestCompatV13ConfigLoads` — loads v13 fixture, verifies all fields
- [ ] Add `TestCompatV13ConfigMigratesToV14` — loads v13 fixture, verifies migration adds SemanticSearch with enabled=false, version=14

### Phase 2: Embedding index management

**File: internal/embed/embed.go** (new package)
- [ ] `Manager` struct wrapping sembed.Index + sembed.Embedder
- [ ] `NewManager(cfg *config.Config) (*Manager, error)` — constructs Embedder from config, loads index from `kanban/.embeddings.json`
- [ ] `Index(ctx, tasks []*task.Task) error` — embeds tasks (title+body+tags concatenated), upserts into index, saves
- [ ] `Search(ctx, query string, k int) ([]sembed.Result, error)` — embeds query, searches index
- [ ] `Sync(ctx, tasks []*task.Task) error` — detects stale embeddings via ContentHash, re-embeds only changed tasks
- [ ] `Close() error` — saves index to disk
- [ ] API key read from `KANBAN_EMBED_API_KEY` env var (following KANBAN_ prefix pattern)
- [ ] Graceful degradation: if API unavailable, return error (caller falls back to substring)

**File: internal/embed/embed_test.go**
- [ ] Tests with httptest-mocked provider
- [ ] Test stale detection + selective re-embedding
- [ ] Test graceful failure when no API key set

### Phase 3: CLI integration

**File: cmd/list.go**
- [ ] Add `--semantic` / `-S` string flag: "semantic search query"
- [ ] When `--semantic` is set and config has semantic_search.enabled:
  1. Load Manager
  2. Sync embeddings for current tasks
  3. Search with query, get ranked IDs
  4. Pass ranked IDs as a pre-filter to FilterOptions (new field: `SemanticIDs []int`)
  5. Output results in normal format (table/compact/json)
- [ ] When `--semantic` is set but semantic_search not enabled, print helpful error with setup instructions
- [ ] When API is unreachable, fall back to substring search with a warning

**File: internal/board/filter.go**
- [ ] Add `SemanticIDs []int` to FilterOptions — when non-nil, only include tasks with these IDs (pre-filter before other filters)
- [ ] Add `SemanticOrder bool` to FilterOptions — when true, preserve SemanticIDs order instead of default sort

### Phase 4: TUI integration

**File: internal/tui/board.go**
- [ ] Add `/~` prefix syntax for semantic search (e.g. `/~performance issues`)
- [ ] In `taskMatchesSearch()`, detect `~` prefix, route to semantic search path
- [ ] Add `embedManager *embed.Manager` field on Board struct
- [ ] Lazy-initialize manager on first semantic search (don't load on startup if not configured)
- [ ] Show indicator in search bar when using semantic mode (e.g. "~" prefix in prompt)
- [ ] On API failure, show error in status bar and fall back to substring

### Phase 5: Index lifecycle

**File: cmd/root.go or new cmd/embed.go**
- [ ] Add `kanban-md embed sync` subcommand — force full re-index of all tasks
- [ ] Add `kanban-md embed status` subcommand — show index stats (doc count, stale count, last sync)
- [ ] Add `kanban-md embed clear` subcommand — delete embeddings.json

**Embedding index file:**
- [ ] Location: `kanban/.embeddings.json`
- [ ] Add `.embeddings.json` to default .gitignore template in init command
- [ ] Content hash = SHA-256 of (title + "\n" + body + "\n" + tags joined by ",")

### Phase 6: Documentation

- [ ] Update README.md: new "Semantic Search" section with setup instructions
- [ ] Document env var: `KANBAN_EMBED_API_KEY`
- [ ] Document config.yml semantic_search section
- [ ] Document CLI --semantic flag
- [ ] Document TUI /~ syntax
- [ ] Add examples for Voyage AI, OpenAI, Ollama providers

## Key Design Decisions

1. **API key in env var, not config** — secrets never written to disk. Follows KANBAN_ prefix convention.
2. **Lazy init in TUI** — don't penalize startup for a feature that may not be used in a given session.
3. **Sync on search** — auto-detect and re-embed stale tasks before searching. No manual re-index needed for normal use.
4. **Separate embed package** — keeps sembed dependency isolated; easy to feature-flag via build tags if needed later.
5. **SemanticIDs pre-filter** — semantic search produces a ranked ID list, then all other filters (status, priority, tag, etc.) apply on top. This composes cleanly.
6. **`/~` TUI prefix** — distinct from `/` substring search, user explicitly opts into semantic mode per query.

## Acceptance Criteria

- [ ] Config v14 migration passes compat tests for all previous versions (v1–v13)
- [ ] `go test ./...` green across all packages
- [ ] `golangci-lint run ./...` clean
- [ ] `kanban-md list --semantic "query"` works end-to-end with Voyage AI
- [ ] TUI `/~query` triggers semantic search and displays ranked results
- [ ] Graceful fallback when API key missing or API unreachable
- [ ] README documents the feature
- [ ] Embedding index excluded from git by default

## Estimated Scope

- Config + migration: ~150 lines
- internal/embed package: ~250 lines
- CLI integration: ~50 lines
- TUI integration: ~80 lines
- embed subcommand: ~100 lines
- Tests: ~400 lines
- Docs: ~100 lines README
- **Total: ~1,100 lines**
