---
id: 70
title: 'kanban-mdx: Add edit --status flag'
status: done
priority: critical
created: 2026-03-12T09:01:12.25028Z
updated: 2026-03-12T09:19:23.77151Z
started: 2026-03-12T09:19:23.771509Z
completed: 2026-03-12T09:19:23.771509Z
tags:
    - kanban-mdx
class: standard
---

Go's `edit` command accepts `--status` to change task status inline with other field edits. kanban-mdx requires a separate `move` command to change status.

## What Go does
- `cmd/edit.go` accepts `--status STATUS` flag
- Allows changing status alongside other fields in a single command: `edit 42 --status in-progress --priority high --claim agent-1`
- Runs the same enforcement checks as `move` (WIP limits, claim requirements, branch requirements)

## Why this matters
- Batch workflows need to change multiple fields atomically: `edit 1,2,3 --status done --completed 2026-03-12`
- Reduces command count in scripts and agent workflows
- Consistent with `create --status` which already exists

## What to implement
- Add `--status` arg to `EditArgs` in `src/cli/edit.rs`
- Apply the same move logic (WIP limits, claim enforcement, branch requirements, `--force` override)
- Record in undo log as a combined edit+move operation

[[2026-03-12]] Thu 09:19
[2026-03-12 12:30] Implemented: --status flag with full enforcement (WIP limits, claim, branch, timestamps)
