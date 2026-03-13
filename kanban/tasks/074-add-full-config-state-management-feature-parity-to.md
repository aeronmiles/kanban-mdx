---
id: 74
title: Add full config state management feature parity to kanban-mdx
status: done
priority: high
created: 2026-03-12T17:51:49.342824Z
updated: 2026-03-12T19:00:27.666919Z
started: 2026-03-12T19:00:27.666919Z
completed: 2026-03-12T19:00:27.666919Z
tags:
    - layer-4
class: standard
branch: main
---

Bring kanban-mdx (Rust) to full feature parity with kanban-md (Go) for all config state management features. This covers the complete config lifecycle: schema, migrations, validation, CLI commands, enforcement logic, and TUI state persistence.

## Background

kanban-mdx already mirrors most of the Go config system (v14 schema, 13 migrations, validation, config get/set, init flags, status enforcement). However, several gaps remain in enforcement logic, validation rigor, and TUI state persistence.

## Gap Analysis

### 1. Class-Level WIP Enforcement (Missing)

**Go**: `enforceWIPLimitForClass()` in `cmd/move.go:234` performs a two-level WIP check:
1. **Class WIP** (board-wide): counts ALL tasks with the same class across ALL statuses, rejects if count >= class.WIPLimit
2. **Column WIP** (per-status): checks per-column limit, skipped if class.BypassColumnWIP is true

**Rust**: Only checks column WIP + bypass_column_wip. Does NOT check class-level board-wide WIP limits.

**Files to change**: `src/cli/move_cmd.rs`, `src/cli/edit.rs`, `src/cli/create.rs` (if create enforces WIP)

**Implementation**:
- Add a `count_by_class()` helper function
- Before column WIP check, check class WIP limit: count all tasks with same class (excluding current task), reject if >= class.wip_limit
- If class bypasses column WIP, skip column check
- Apply in move, edit --status, and create commands

### 2. TUI State Persistence on Exit (Partially Missing)

**Go**: TUI saves sort_mode, time_mode, list_mode, and theme back to config.yml on exit.

**Rust**: TUI reads sort_mode, time_mode, list_mode from config on startup (app.rs:755-769), and persists collapsed_columns (app.rs:3690). But does NOT persist sort_mode, time_mode, list_mode, or theme_kind back to config on exit or on change.

**Files to change**: `src/tui/app.rs`

**Implementation**:
- After changing sort_mode (line 1350-1357), persist cfg.tui.sort_mode and save config
- After changing time_mode (line 1361-1362), persist cfg.tui.time_mode and save config
- After changing theme (line 1365-1369, 2801-2804), persist cfg.tui.theme and save config
- On switching between list/board view, persist cfg.tui.list_mode and save config
- Consider batching saves to avoid excessive disk I/O (debounce or save-on-exit)

### 3. Config Set Validation (Incomplete)

**Go**: `config set` validates before saving (e.g., claim_timeout is parsed as a duration).

**Rust**: `config set claim_timeout` stores the raw string without validating it's a valid Go-duration format. The save path (`io::config_file::save`) does NOT call `validate()`. Invalid values can be persisted and only fail on next load.

**Files to change**: `src/cli/config.rs`

**Implementation**:
- Add duration validation for `claim_timeout` in the set handler (parse with `parse_go_duration`)
- Call `cfg.validate()` before `config_file::save()` in the set handler
- Consider adding validation to `config_file::save()` itself as a safety net

### 4. Class WIP Enforcement on Create (Verify)

**Go**: `cmd/create.go:114` calls `enforceWIPLimitForClass()` when creating a new task if it has a class and the default status has WIP limits.

**Rust**: Check if `src/cli/create.rs` enforces WIP limits at creation time. If not, add it.

### 5. Create Command Class WIP Check (Verify/Add)

The create command should enforce both column WIP and class WIP when a task is created with a non-default status or class.

## Already Implemented (No Action Needed)

These features are confirmed present in kanban-mdx:

- ✅ Config schema v14 with all fields (version, board, statuses, priorities, defaults, wip_limits, claim_timeout, classes, tui, semantic_search, next_id)
- ✅ All 13 migrations (v1→v14) with auto-save on load
- ✅ Config validation (version, board.name, tasks_dir, statuses, priorities, defaults, wip_limits, classes, claim_timeout, tui.title_lines, age_thresholds, next_id)
- ✅ Config get/set CLI with writable keys: board.name, board.description, defaults.status/priority/class, claim_timeout, tui.title_lines/hide_empty_columns/theme/reader_max_width
- ✅ Init command with --statuses, --wip-limit, --name, --path flags
- ✅ StatusConfig backward compat deserialization (string or mapping)
- ✅ Status enforcement: require_claim on move/edit, require_branch with --force override
- ✅ Column WIP limit enforcement on move/edit with class bypass_column_wip
- ✅ Claim management: --claim/--release on edit, claim_timeout parsing (Go duration format)
- ✅ Branch enforcement: 3-level check (convention match, exact match, no-branch skip) with --force
- ✅ Terminal status detection (done, archived)
- ✅ Board/active/archived status helpers
- ✅ TUI collapsed column persistence
- ✅ Go-style duration parsing (ns, us, ms, s, m, h, compound "1h30m")
- ✅ File permissions (0o600 config, 0o750 dirs)
- ✅ Dir resolution with git worktree fallback
- ✅ Task consistency auto-repair (duplicate IDs, next_id drift)

## Acceptance Criteria

- [ ] Class-level board-wide WIP limits are enforced on move, edit --status, and create
- [ ] TUI persists sort_mode, time_mode, list_mode, and theme to config.yml
- [ ] `config set claim_timeout` validates the duration before saving
- [ ] `config set` calls validate() before save()
- [ ] All existing config tests continue to pass
- [ ] New tests cover class WIP enforcement (both class-level and column-level in same check)
- [ ] New tests cover TUI state persistence round-trip

[[2026-03-12]] Thu 19:00
Orchestrated completion: All 3 gaps implemented in parallel by 3 agents. Build passes, all 381 tests pass. Changes: (1) Class-level WIP enforcement via new src/cli/wip.rs shared module with two-level check. (2) TUI state persistence for sort_mode, time_mode, list_mode, theme on each toggle. (3) Config set validation with parse_go_duration and validate() before save.
