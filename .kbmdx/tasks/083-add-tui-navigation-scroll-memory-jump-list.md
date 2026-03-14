---
id: 83
title: 'Add TUI navigation: scroll memory, jump list, breadcrumb'
status: done
priority: high
created: 2026-03-13T16:04:50.841602Z
updated: 2026-03-13T16:31:13.093883Z
started: 2026-03-13T16:04:57.79402Z
completed: 2026-03-13T16:31:13.093882Z
tags:
    - layer-3
class: standard
---

Implement three composing navigation layers for the kanban-mdx TUI:

1. **Per-task scroll memory** — HashMap<TaskId, ScrollState> that remembers scroll offset + fold level per task. Restore position when re-entering detail view instead of resetting to 0.

2. **Jump list** (Shift+Option+{ / Shift+Option+}) — Automatic navigation history stack recording significant transitions (opening detail, jumping to heading, switching tasks). Shift+Alt+{ goes back, Shift+Alt+} goes forward. Ring buffer capped at ~100 entries.

3. **Visual breadcrumb in status bar** — Rendered path like 'Board > #042 > ## Implementation Notes' in the footer showing current navigation context.

These compose: scroll memory handles the common case, jump list handles multi-step navigation, breadcrumb provides orientation.
