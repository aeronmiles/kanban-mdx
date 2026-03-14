---
id: 31
title: Task consistency checks on load (duplicate IDs, filename mismatch, next_id drift)
status: done
priority: critical
created: 2026-03-11T08:20:43.290031Z
updated: 2026-03-11T08:36:40.709214Z
started: 2026-03-11T08:36:40.709214Z
completed: 2026-03-11T08:36:40.709214Z
tags:
    - parity
class: standard
---

Go version runs EnsureConsistency() on config load: auto-detects and repairs duplicate IDs, detects filename/frontmatter ID mismatches, detects next_id drift, and does defense-in-depth scanning of actual task files on create. kanban-mdx lacks these integrity checks.
