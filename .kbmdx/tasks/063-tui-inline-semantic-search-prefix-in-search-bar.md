---
id: 63
title: 'TUI: Inline semantic search (~prefix) in search bar'
status: archived
priority: critical
created: 2026-03-12T08:47:45.844304Z
updated: 2026-03-12T08:51:48.6604Z
started: 2026-03-12T08:51:48.660399Z
completed: 2026-03-12T08:51:48.660399Z
tags:
    - kanban-mdx
class: standard
---

Allow `~query` prefix in the TUI search bar for embedding-based semantic search, combinable with DSL filters.

## Current state
Go version has semantic search via a separate `f` key (find). kanban-mdx allows `~query` directly in the `/` search bar, combinable with other DSL filters like `@<24h p:high ~error handling`.

## What to implement
- Parse `~` prefix in the search filter DSL
- When `~` is present, extract the semantic query portion and run embedding search
- Combine semantic results with other active DSL filters (AND logic)
- Requires embedding index to be synced (`embed sync`)

## Why this matters
Combining semantic search with time/priority/ID filters in a single query is more powerful than having them as separate modes.
