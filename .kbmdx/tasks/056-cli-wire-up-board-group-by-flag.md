---
id: 56
title: 'CLI: Wire up board --group-by flag'
status: done
priority: critical
created: 2026-03-11T09:51:48.507597Z
updated: 2026-03-12T19:23:30.998888Z
started: 2026-03-12T19:23:30.998888Z
completed: 2026-03-12T19:23:30.998888Z
tags:
    - kanban-mdx
class: standard
branch: main
---

In kanban-mdx, the `board` command defines a `--group-by` flag in `BoardArgs` (src/cli/board.rs line 20), but the implementation never uses it. The `render_board()` function always groups by status.

In Go kanban-mdx, `board --group-by assignee|tag|class|priority|status` fully works and shows the board grouped by the selected field.

**What to implement:**
- Wire up the existing `--group-by` field in `BoardArgs` to the board rendering logic
- Support grouping by: assignee, tag, class, priority, status
- Match the Go version's grouping behavior where tasks are organized into swim lanes by the grouped field

Wired up --group-by flag in board.rs. Validates field, calls group_by_summary(), routes to table/compact/json renderers. All 381 tests pass.
