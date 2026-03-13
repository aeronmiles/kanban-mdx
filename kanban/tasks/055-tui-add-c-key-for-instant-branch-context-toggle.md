---
id: 55
title: 'TUI: Add C key for instant branch context toggle'
status: done
priority: critical
created: 2026-03-11T09:51:48.173743Z
updated: 2026-03-12T17:05:01.87092Z
started: 2026-03-12T17:04:47.186446Z
completed: 2026-03-12T17:04:47.186446Z
tags:
    - kanban-mdx
class: standard
branch: main
---

In Go kanban-md, pressing `C` in the TUI instantly scopes the board to tasks related to the current git branch/worktree — no picker needed. This is a quick toggle: press once to filter, press again to clear.

kanban-mdx has `--context/-C` on the `list` CLI command, but the TUI has no equivalent keybinding. Users must open the branch picker (`b`) and manually select branches to achieve the same filtering.

**What to implement:**
- Bind `C` (Shift+c) in the TUI board view
- On press, toggle a context filter that shows only tasks matching the current git branch (using `task/<ID>-*` convention)
- Show a visual indicator in the status bar when context filter is active
- Press again to clear the filter

Superseded by task #72 which covers the full context system port, not just the C key toggle.

Superseded by #72 — C key context picker implemented as part of full context system port.
