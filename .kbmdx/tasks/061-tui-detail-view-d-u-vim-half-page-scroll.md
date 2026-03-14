---
id: 61
title: 'TUI: Detail view d/u vim half-page scroll'
status: archived
priority: critical
created: 2026-03-12T08:47:29.358079Z
updated: 2026-03-12T08:51:48.643257Z
started: 2026-03-12T08:51:48.643256Z
completed: 2026-03-12T08:51:48.643256Z
tags:
    - kanban-mdx
class: standard
---

Add `d` and `u` as alternative vim-style half-page scroll keybindings in the detail view.

## Current state
Go version uses J/K for half-page in detail view. kanban-mdx additionally supports `d` (half-page down) and `u` (half-page up) matching vim's Ctrl+d/Ctrl+u convention.

## Keys to add
- `d` — Half-page scroll down (in detail view only)
- `u` — Half-page scroll up (in detail view only)

These are standard vim keybindings that users expect in keyboard-driven TUIs.
