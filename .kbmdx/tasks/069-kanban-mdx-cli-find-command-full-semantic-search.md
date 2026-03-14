---
id: 69
title: 'kanban-mdx: CLI find command full semantic search'
status: done
priority: critical
created: 2026-03-12T09:01:07.930138Z
updated: 2026-03-12T17:26:15.570901Z
started: 2026-03-12T09:19:22.996538Z
completed: 2026-03-12T09:19:22.996538Z
tags:
    - kanban-mdx
class: standard
---

Go's `find <query>` command performs full semantic search across task sections using embeddings. kanban-mdx's CLI `find` command is a stub that falls back to substring matching.

## What Go does
- `cmd/find.go` runs embedding-based semantic search via the sembed library
- Returns sections ranked by relevance with similarity scores
- Supports `-n/--limit` for result count
- Requires embedding index to be synced (`embed sync`)

## Current kanban-mdx state
- `src/cli/find.rs` is marked as a stub
- Falls back to substring search (no embedding support)
- TUI search supports `~query` for semantic search, but CLI does not

## What to implement
- Wire up the embedding provider in the CLI find command
- Query the embedding index with the user's search query
- Rank and display results by similarity score
- Support `-n/--limit` flag (already may exist)
- Graceful error if embeddings not synced

[[2026-03-12]] Thu 09:19
[2026-03-12 12:30] Implemented: CLI find command uses embed::Manager for semantic search with json/compact/table output

Task #73 audit confirmed: CLI find command is fully implemented with semantic search, not a stub. Production code complete with table/compact/json output. Needs integration tests (covered by #73).
