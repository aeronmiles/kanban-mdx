---
id: 30
title: 'worktrees: Stale metadata and orphan worktree detection'
status: done
priority: critical
created: 2026-03-11T08:20:42.940993Z
updated: 2026-03-11T08:36:40.405562Z
started: 2026-03-11T08:36:40.405562Z
completed: 2026-03-11T08:36:40.405562Z
tags:
    - parity
class: standard
---

Go version cross-references git worktrees with task branch fields to detect stale metadata (task points to non-existent worktree) and orphan worktrees (worktree exists but no task references it). kanban-mdx just lists worktrees without validation.
