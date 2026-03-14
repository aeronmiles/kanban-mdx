---
id: 38
title: 'TUI: Live refresh on file changes (file watcher)'
status: done
priority: critical
created: 2026-03-11T08:20:58.843126Z
updated: 2026-03-11T08:26:10.752569Z
started: 2026-03-11T08:26:10.752568Z
completed: 2026-03-11T08:26:10.752568Z
tags:
    - parity
class: standard
---

Go TUI live-reloads when task files change on disk (enables multi-agent workflows where one agent modifies tasks while another has TUI open). kanban-mdx has the notify crate as a dependency but board --watch is listed as not yet implemented, and TUI file watching status is unclear.
