# kanban-md

A file-based Kanban tool powered by Markdown. Tasks are individual `.md` files with YAML frontmatter, managed through a CLI and terminal UI. No database required — everything lives in your repo and works with git out of the box.

## Features

- **Markdown tasks** — Human-readable task files with YAML frontmatter and freeform body sections
- **CLI + TUI** — Full-featured command-line interface and interactive terminal board
- **Git-native** — Branch context, worktree support, and branch-to-task linking
- **WIP limits** — Per-status and per-class-of-service work-in-progress enforcement
- **Semantic search** — Optional vector search via Voyage AI, Ollama, or OpenAI (powered by [sembed-rs](https://github.com/aeronmiles/sembed-rs))
- **Agent workflows** — Task claiming with timeout, handoff, and random agent name generation
- **Dependencies** — Task blocking, parent-child hierarchy, and dependency graph visualization
- **Undo/redo** — File-level snapshots with full rollback support
- **Activity log** — JSONL mutation journal for auditability
- **Metrics** — Cycle time, throughput, and per-status/priority breakdowns
- **Auto-repair** — Detects and fixes ID collisions, filename mismatches, and config drift on load

## Installation

```sh
cargo install --path .
```

Binary name: `kanban-md`

## Quick start

```sh
# Initialize a board in the current directory
kanban-md init "My Project"

# Create tasks
kanban-md create "Design the API" --priority high --tags api,design
kanban-md create "Implement auth" --depends-on 1

# Move through the workflow
kanban-md move 1 in-progress --claim alice
kanban-md move 1 done

# View the board
kanban-md board
kanban-md list --status in-progress

# Launch the TUI
kanban-md tui
```

## Task format

Tasks are stored as markdown files in `kanban/tasks/`:

```
kanban/tasks/001-design-the-api.md
```

```markdown
---
id: 1
title: Design the API
status: in-progress
priority: high
created: 2026-03-10T09:59:19Z
updated: 2026-03-10T14:45:35Z
started: 2026-03-10T14:45:35Z
assignee: alice
tags: [api, design]
due: 2026-03-20
estimate: "3h"
depends_on: []
claimed_by: alice
claimed_at: 2026-03-10T14:45:35Z
branch: task/1-design-the-api
---

## Notes

API should follow REST conventions with versioned endpoints.

## Log

2026-03-10: Started initial design draft.
```

All fields except `id`, `title`, and `status` are optional.

## CLI commands

### Task lifecycle

| Command | Description |
|---|---|
| `init [NAME]` | Create a new board with default or custom statuses |
| `create TITLE` | Create a task with optional flags for all fields |
| `show <ID>` | Display task details |
| `edit <ID>` | Modify task fields (status, priority, tags, body, sections, etc.) |
| `delete <ID>` | Remove a task |
| `move <ID> <STATUS>` | Move to a status (enforces WIP limits, claims, branch checks) |
| `archive <ID>` | Move to archived status |
| `handoff <ID> --claim AGENT` | Reassign a task to another agent |
| `pick` | Auto-select the highest-priority unblocked unclaimed task |

### Queries and reports

| Command | Description |
|---|---|
| `list` | Query tasks with filters (status, priority, assignee, tag, branch, etc.) |
| `find QUERY` | Semantic search across task content |
| `board` | Board summary with card counts by status |
| `metrics` | Analytics: cycle time, throughput, status/priority breakdowns |
| `deps <ID>` | Show dependency graph for a task |
| `log` | View the activity log (filterable by date, action, task) |
| `context` | Generate markdown context for agents (in-progress, blocked, overdue) |

### System

| Command | Description |
|---|---|
| `undo` / `redo` | Roll back or replay the last mutation |
| `config` | Get or set config values (`--get KEY`, `--set KEY VALUE`) |
| `tui` | Launch the interactive terminal UI |
| `worktrees` | List git worktrees with optional stale/orphan detection |
| `branch-check` | Validate branch setup against task metadata |
| `embed sync` | Generate or update semantic embeddings |
| `import FILE` | Bulk-create tasks from JSON or YAML |
| `completion SHELL` | Generate shell completions (bash, zsh, fish) |
| `filepath <ID>` | Print the filesystem path of a task file |

### Global flags

```
-d, --dir <PATH>    Board directory (overrides auto-detection)
    --json          JSON output
    --compact       One-line output for scripting
    --table         Table output (default)
    --no-color      Disable ANSI colors (also respects NO_COLOR env)
```

## Terminal UI

Launch with `kanban-md tui`. The TUI provides:

- **Kanban board** — Columns by status with task cards, age-based coloring, and WIP indicators
- **Detail panel** — Full markdown rendering of task body via [tui-md](https://github.com/aeronmiles/tui-md) with syntax highlighting
- **Navigation** — Keyboard (hjkl/arrows) and mouse support
- **Collapsible columns** — Toggle column visibility, Tab to collapse/expand
- **Search** — Substring and semantic search with history
- **Dialogs** — Create, edit, move, delete, and filter tasks inline
- **Live reload** — File watcher auto-refreshes the board on external changes
- **Themes** — Configurable brightness, saturation, and color themes

## Configuration

Board configuration lives in `kanban/config.toml`:

```toml
version = 15

[board]
name = "My Project"

tasks_dir = "tasks"
next_id = 1

# Workflow statuses (order matters)
[[statuses]]
name = "backlog"

[[statuses]]
name = "in-progress"
require_claim = true       # Must claim before entering this status
show_duration = true       # Show task age in TUI

[[statuses]]
name = "review"

[[statuses]]
name = "done"

[[statuses]]
name = "archived"

# WIP limits per status
[wip_limits]
"in-progress" = 3
"review" = 2

# Priority levels (order defines sort rank)
[[priorities]]
name = "low"
[[priorities]]
name = "medium"
[[priorities]]
name = "high"
[[priorities]]
name = "critical"

# Default values for new tasks
[defaults]
status = "backlog"
priority = "medium"
class = "standard"

# Class of service
[[classes]]
name = "expedite"
wip_limit = 1
bypass_column_wip = true

[[classes]]
name = "standard"

# Claim timeout (Go-style duration)
claim_timeout = "1h"

# TUI settings
[tui]
title_lines = 2
theme = "default"
reader_max_width = 120
reader_width_pct = 40
brightness = 0.0
saturation = -0.2

# Semantic search (optional)
[semantic_search]
enabled = false
provider = "voyage"
model = "voyage-3"
```

## Board directory structure

```
project/
└── kanban/
    ├── config.toml          # Board configuration
    ├── tasks/               # Task markdown files
    │   ├── 001-task-slug.md
    │   ├── 002-another.md
    │   └── ...
    ├── activity.jsonl       # Mutation log (max 10,000 entries)
    ├── undo.jsonl           # Undo snapshots (max 100)
    ├── .embeddings.json     # Semantic search index
    └── .lock                # File lock
```

## Git integration

kanban-md is designed for git-based workflows:

- **Branch context** — Link tasks to branches via `branch` field or `task/ID-*` naming convention. The `list` and `tui` views can filter to the current branch's context, showing the linked task plus its parent, siblings, and dependencies.
- **Worktree support** — Tasks can reference worktree paths. `worktrees --check` detects stale metadata and orphaned worktrees.
- **Auto-discovery** — Config search walks upward from the current directory and resolves through linked worktrees to find the main board.

## Semantic search

Enable optional vector search for finding tasks by meaning rather than keyword:

```sh
# Configure in config.toml
kanban-md config --set semantic_search.enabled true
kanban-md config --set semantic_search.provider voyage
kanban-md config --set semantic_search.model voyage-3

# Build the index
kanban-md embed sync

# Search
kanban-md find "authentication flow"
```

Supported providers: Voyage AI, Ollama, OpenAI (via [sembed-rs](https://github.com/aeronmiles/sembed-rs)).

## Sub-crates

| Crate | Description |
|---|---|
| [sembed-rs](https://github.com/aeronmiles/sembed-rs) | Provider-agnostic semantic embeddings library |
| [tui-md](https://github.com/aeronmiles/tui-md) | Markdown-to-ratatui renderer with GFM tables, heading folding, and syntax highlighting |

## License

MIT
