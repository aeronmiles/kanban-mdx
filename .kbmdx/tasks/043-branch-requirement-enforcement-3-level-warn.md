---
id: 43
title: 'Branch requirement enforcement (3-level: warn, enforce, --force override)'
status: done
priority: critical
created: 2026-03-11T08:21:12.784617Z
updated: 2026-03-11T08:36:43.518075Z
started: 2026-03-11T08:36:43.518074Z
completed: 2026-03-11T08:36:43.518074Z
tags:
    - parity
class: standard
---

Go version has 3-level branch enforcement: Level 1 warns if branch mismatch on edit/move, Level 2 (require_branch on status) blocks operations without branch set, Level 3 (--force flag) overrides Level 2. kanban-mdx has require_claim on statuses but branch enforcement levels are not implemented. Note: --force flag itself is tracked in #16.
