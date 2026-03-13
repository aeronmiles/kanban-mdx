---
id: 24
title: 'init: Add --statuses and --wip-limit flags'
status: done
priority: critical
created: 2026-03-11T07:21:53.279864Z
updated: 2026-03-11T08:25:42.771147Z
started: 2026-03-11T08:20:48.644222Z
completed: 2026-03-11T08:20:48.644222Z
class: standard
---

Go's init command has --statuses (comma-separated custom status list) and --wip-limit STATUS:N (repeatable, per-status WIP limit). kanban-mdx init only has --name and --path.
