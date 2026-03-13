---
id: 59
title: 'TUI: Theme brightness and saturation controls'
status: archived
priority: critical
created: 2026-03-12T08:47:22.590802Z
updated: 2026-03-12T08:51:48.624898Z
started: 2026-03-12T08:51:48.624897Z
completed: 2026-03-12T08:51:48.624897Z
tags:
    - kanban-mdx
class: standard
---

Add per-session theme brightness and saturation adjustment keys from kanban-mdx parity.

## Keys to add
- `,` — Decrease brightness
- `.` — Increase brightness
- `Ctrl+,` — Decrease saturation
- `Ctrl+.` — Increase saturation
- `T` — Reset all theme adjustments (brightness + saturation)

## Context
kanban-mdx allows real-time per-session tuning of the Glamour markdown theme. The Go version currently only supports cycling themes (`t`) but has no brightness/saturation adjustment.
