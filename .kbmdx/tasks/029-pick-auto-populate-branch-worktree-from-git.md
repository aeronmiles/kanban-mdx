---
id: 29
title: 'pick: Auto-populate branch/worktree from git context'
status: done
priority: critical
created: 2026-03-11T08:20:41.520435Z
updated: 2026-03-11T08:36:40.097516Z
started: 2026-03-11T08:36:40.097515Z
completed: 2026-03-11T08:36:40.097515Z
tags:
    - parity
class: standard
---

Go version auto-populates branch and worktree fields from current git context when a task is picked (Level 1 enforcement). Also warns if task has existing worktree/branch from a previous claim. kanban-mdx does not auto-populate these fields on pick.
