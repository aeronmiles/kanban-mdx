---
id: 82
title: Add brightness/saturation theme adjustments to TUI
status: done
priority: high
created: 2026-03-13T16:00:14.453156Z
updated: 2026-03-13T16:51:56.903829Z
started: 2026-03-13T16:00:21.313964Z
completed: 2026-03-13T16:51:56.903828Z
tags:
    - layer-3
class: standard
---

Port brightness/saturation color adjustment from Rust kanban-mdx to Go TUI. Shortcuts: . , for brightness, ctrl+. ctrl+, for saturation, T for reset. Requires HSL color space conversion, config persistence, and applying adjustments to all lipgloss styles at render time.
