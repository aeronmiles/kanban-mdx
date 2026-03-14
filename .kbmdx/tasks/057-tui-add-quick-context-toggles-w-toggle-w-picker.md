---
id: 57
title: 'TUI: Add quick context toggles (w=toggle, W=picker)'
status: done
priority: critical
created: 2026-03-11T09:51:48.829494Z
updated: 2026-03-11T19:01:18.702826Z
started: 2026-03-11T18:45:28.087572Z
completed: 2026-03-11T18:45:28.087572Z
tags:
- kanban-mdx
class: standard
branch: main
---

In Go kanban-mdx, the TUI has 4 context/branch keybindings with distinct behaviors:
- `C` — instant toggle: scope board to current branch's tasks (no picker)
- `b` — picker: open branch context picker (multi-select, all branches)
- `w` — instant toggle: scope to tasks that have worktrees set
- `W` — picker: open worktree context picker

In kanban-mdx, there are only 2 keybindings, both pickers:
- `b` — picker: open branch picker (all branches)
- `w` — picker: open branch picker (worktree branches only)

**What to implement:**
- Change `w` from a picker to an instant toggle that filters to tasks with worktrees set (no modal)
- Add `W` (Shift+w) as the worktree-specific picker (current `w` behavior moves here)
- Show visual indicator when worktree-only filter is active
- This pairs with task #55 which adds the `C` toggle

Note: The `b` picker behavior is already correct and matches Go.
