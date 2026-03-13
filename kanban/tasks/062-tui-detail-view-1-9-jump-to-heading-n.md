---
id: 62
title: 'TUI: Detail view 1-9 jump to heading N'
status: archived
priority: critical
created: 2026-03-12T08:47:33.65649Z
updated: 2026-03-12T08:51:48.65158Z
started: 2026-03-12T08:51:48.651579Z
completed: 2026-03-12T08:51:48.651579Z
tags:
    - kanban-mdx
class: standard
---

Add number keys 1-9 in the detail view to jump directly to the Nth heading in the rendered markdown.

## Current state
Go version supports heading navigation via }/{ (next/prev heading) and ⌥]/⌥[ (next/prev ## heading). kanban-mdx additionally supports `1-9` to jump directly to the Nth heading.

## Keys to add
- `1` through `9` — Jump to Nth heading in the task body

## Conflict note
Number keys 1-7 are used for column solo-expand in board view. This binding only applies in detail view where those keys are unused.
