---
id: 27
title: 'Undo/redo: Full implementation with file snapshots and --dry-run'
status: done
priority: critical
created: 2026-03-11T08:17:17.939314Z
updated: 2026-03-11T08:26:09.976861Z
started: 2026-03-11T08:26:09.97686Z
completed: 2026-03-11T08:26:09.97686Z
tags:
    - parity
class: standard
---

Go version has robust undo/redo with .undo/.redo file snapshot stacks and --dry-run preview. kanban-mdx has limited undo/redo (described as 'mainly for testing/demos'). Implement full file-snapshot-based undo/redo with dry-run support.
