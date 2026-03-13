---
id: 66
title: 'CLI: Add show --no-body flag'
status: archived
priority: critical
created: 2026-03-12T08:47:59.27953Z
updated: 2026-03-12T08:51:48.684654Z
started: 2026-03-12T08:51:48.684653Z
completed: 2026-03-12T08:51:48.684653Z
tags:
    - kanban-mdx
class: standard
---

Add `--no-body` flag to the `show` command to suppress the task body in output.

## Current state
Go `show` command always outputs the full task including body. kanban-mdx supports `--no-body` to suppress the markdown body and show only frontmatter fields.

## Flag to add
- `--no-body` — Suppress task body, show only metadata fields

## Use cases
- Quick metadata inspection: `show 42 --no-body` to see status/priority/tags without body clutter
- Scripting: `show 42 --no-body --json | jq .priority` for clean field extraction
- Complements existing `--prompt` and `--fields` flags for different output needs
