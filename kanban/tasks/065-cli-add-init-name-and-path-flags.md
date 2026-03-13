---
id: 65
title: 'CLI: Add init --name and --path flags'
status: archived
priority: critical
created: 2026-03-12T08:47:55.403392Z
updated: 2026-03-12T08:51:48.677002Z
started: 2026-03-12T08:51:48.677001Z
completed: 2026-03-12T08:51:48.677001Z
tags:
    - kanban-mdx
class: standard
---

Add `--name` and `--path` flags to the `init` command.

## Current state
Go `init` command supports `--statuses` and `--wip-limit` but not `--name` or `--path`. kanban-mdx supports both.

## Flags to add
- `--name NAME` — Set the board name during initialization (goes into config.yml `board.name`)
- `--path PATH` — Create the board at a custom path instead of the default `./kanban` directory

## Use cases
- `init --name "Sprint 12" --path ./boards/sprint-12` — create a named board at a custom location
- `init --name "My Project"` — set the board name without interactive prompts
