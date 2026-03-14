---
id: 71
title: 'kanban-mdx: TUI ctrl+n/ctrl+p search match navigation in search bar'
status: done
priority: critical
created: 2026-03-12T09:01:17.998318Z
updated: 2026-03-12T09:19:24.486563Z
started: 2026-03-12T09:19:24.486563Z
completed: 2026-03-12T09:19:24.486563Z
tags:
    - kanban-mdx
class: standard
---

Go's TUI supports `ctrl+n` and `ctrl+p` for navigating between search matches while still in the search input bar. kanban-mdx only has these bindings in the detail view find mode, not in the board-level search bar.

## What Go does
- While typing in the search bar (`/`), `ctrl+n` jumps to the next match and `ctrl+p` to the previous
- This allows refining a search query while cycling through matches without leaving the input

## Current kanban-mdx state
- Board search bar only supports Up/Down arrows and Tab for history/completion
- `ctrl+n`/`ctrl+p` only work in detail view find (`ctrl+f`)

## What to implement
- Add `ctrl+n` and `ctrl+p` key handlers to the board search mode
- When pressed, highlight/navigate to the next/previous matching task while keeping the search input focused

[[2026-03-12]] Thu 09:19
[2026-03-12 12:30] Implemented: ctrl+n/ctrl+p navigate between matching tasks in board search bar
