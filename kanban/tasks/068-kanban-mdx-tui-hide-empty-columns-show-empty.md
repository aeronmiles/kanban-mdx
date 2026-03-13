---
id: 68
title: 'kanban-mdx: TUI --hide-empty-columns / --show-empty-columns CLI flags'
status: done
priority: critical
created: 2026-03-12T09:00:59.280614Z
updated: 2026-03-12T09:19:22.242998Z
started: 2026-03-12T09:19:22.242998Z
completed: 2026-03-12T09:19:22.242998Z
tags:
    - kanban-mdx
class: standard
---

Go's `tui` command accepts `--hide-empty-columns` and `--show-empty-columns` flags to override the config setting at launch time. kanban-mdx has the `tui.hide_empty_columns` config option but no CLI flags to override it.

## What Go does
- `cmd/tui.go` registers `--hide-empty-columns` and `--show-empty-columns` flags
- These override `config.tui.hide_empty_columns` for the current session
- Useful for quickly toggling behavior without editing config

## What to implement
- Add `--hide-empty-columns` and `--show-empty-columns` flags to `TuiArgs` in `src/cli/tui.rs`
- Pass the override to the TUI initialization
- Flags should conflict with each other (mutually exclusive)

[[2026-03-12]] Thu 09:19
[2026-03-12 12:30] Implemented: --hide-empty-columns / --show-empty-columns CLI flags + TUI rendering behavior with fallback
