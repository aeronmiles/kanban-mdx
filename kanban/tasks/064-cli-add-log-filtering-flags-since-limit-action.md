---
id: 64
title: 'CLI: Add log filtering flags (--since, --limit, --action, --task)'
status: archived
priority: critical
created: 2026-03-12T08:47:51.224429Z
updated: 2026-03-12T08:51:48.669047Z
started: 2026-03-12T08:51:48.669046Z
completed: 2026-03-12T08:51:48.669046Z
tags:
    - kanban-mdx
class: standard
---

Add filtering flags to the `log` command for more useful mutation history queries.

## Current state
Go `log` command has no filtering flags. kanban-mdx supports:

## Flags to add
- `--since DATE` — Only show log entries after this date (YYYY-MM-DD)
- `--limit N` / `-n N` — Limit number of log entries shown
- `--action ACTION` — Filter by mutation type (create, edit, move, delete, etc.)
- `--task ID` — Filter to entries affecting a specific task ID

## Use cases
- `log --since 2026-03-01 --action move` — see all status changes this month
- `log --task 42 --limit 5` — last 5 mutations on task 42
- `log --action delete` — audit all deletions
