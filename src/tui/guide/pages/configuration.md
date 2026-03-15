# Configuration

Configuration lives in `.kbmdx/config.toml`.

## Statuses

Statuses define the columns of your board. Each can be a simple
name or a mapping with enforcement rules:

```toml
[[statuses]]
name = "backlog"

[[statuses]]
name = "in-progress"
require_claim = true
show_duration = true

[[statuses]]
name = "done"
show_duration = true
```

## Enforcement rules

| Rule | Enforced by | `--force` bypasses? |
|------|-------------|:---:|
| require_claim | `move`, `edit` | |
| require_branch | `move`, `edit` | yes |
| WIP limit (column) | `move` | yes |
| WIP limit (class) | `move` | yes |
| Dependency check | `pick` | |
| Blocked flag | `pick` | |

## Priorities

```toml
[priorities]
levels = ["low", "medium", "high", "critical"]
```

## Classes of service

```toml
[[classes]]
name = "expedite"
wip_limit = 1

[[classes]]
name = "standard"
```

## TUI settings

```toml
[tui]
title_lines = 2              # card title display lines
theme = "dark"               # color theme
hide_empty_columns = false   # hide columns with no tasks
reader_max_width = 120       # max content width in detail/reader
reader_width_pct = 40        # reader panel width as % of terminal
list_mode = false            # default to card view
sort_mode = 0                # 0=priority, 1=newest, 2=oldest
time_mode = 0                # 0=created, 1=updated
brightness = 0.0             # brightness adjustment (-1.0 to 1.0)
saturation = 0.0             # saturation adjustment (-1.0 to 1.0)
collapsed_columns = []       # columns collapsed by default
```

## Semantic search

```toml
[semantic_search]
# Embeddings database configuration
```

When configured, prefix search queries with `~` to use semantic
search, or use `kbmdx find` from the CLI.
