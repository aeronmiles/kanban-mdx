---
id: 41
title: WIP limits enforcement on move and create operations
status: done
priority: critical
created: 2026-03-11T08:21:07.845765Z
updated: 2026-03-11T08:36:42.891621Z
started: 2026-03-11T08:36:42.89162Z
completed: 2026-03-11T08:36:42.89162Z
tags:
    - parity
class: standard
---

Go version enforces WIP limits on move and create: rejects operations that would exceed per-status WIP limits, with class-aware bypass for expedite class. kanban-mdx has WIP limits in config but unclear if they are enforced at runtime.
