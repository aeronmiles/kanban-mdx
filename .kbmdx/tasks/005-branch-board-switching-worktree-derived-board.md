---
id: 5
title: 'Branch board switching: worktree-derived board views'
status: done
priority: medium
created: 2026-03-10T11:08:13.060367Z
updated: 2026-03-12T17:32:29.714530Z
tags:
- layer-3
class: standard
branch: dev
---

## Summary

Design proposal for branch board switching — worktree-derived, context-aware board views with graduated branch-task enforcement. Keeps the shared board model (`.gitignore`-d) but adds enforcement layers to prevent wrong-branch work, plus TUI context switching support.

## Research

Full report: `docs/research/2026-03-10-branch-board-switching.md`
Worktree access report: `docs/research/2026-03-10-worktree-context-switching.md`

## Core Insight

The git worktree graph IS the active board:
```
git worktree list → branches → task.Branch fields → filtered board view
```
"Switching" is `cd ../other-worktree`. The board view follows you.

## Architectural Decision: Shared Board with Enforcement

Three models evaluated:
1. **Committed board** (rejected) — board travels with branches. Destroys real-time coordination, causes `next_id` merge conflicts, creates stale snapshots on long-lived branches.
2. **Hybrid split** (deferred to v2) — committed task definitions + shared coordination state. Architecturally cleanest but high migration cost and dual-consistency complexity.
3. **Shared board + graduated enforcement** (chosen) — board stays `.gitignore`-d. Branch-task binding enforced at runtime through 4 levels.

## Branch-Task Enforcement Stack

```
Level 0 (always on):  Branch-name → task ID convention
                       task/4-worktree-access → #4
                       Zero config, ~5 lines, covers 90% of cases

Level 1 (always on):  Auto-populate on pick
                       pick --claim detects branch + worktree via git
                       Auto-sets task.Branch and task.Worktree

Level 2 (default on): Warn on branch mismatch
                       "Warning: task #7 is on task/7-foo but you're on task/4-bar"
                       Non-blocking stderr warning

Level 3 (opt-in):     require_branch per status column
                       Like require_claim — refuses mutations on mismatch
                       Configured in config.yml, --force to override
```

## Design Layers

### Layer 1: `internal/git` package (prerequisite)
New package for git context detection:
- `CurrentBranch(dir)` — reads `.git/HEAD` (follows worktree `.git` file)
- `ListWorktrees(repoRoot)` — parses `git worktree list --porcelain`
- `IsWorktree(dir)` — detects linked worktrees
- `ParseTaskIDBranch(branch)` — extracts task ID from `task/<ID>-*` convention
- Pure file reads where possible, `git` subprocess for `ListWorktrees`

### Layer 2: `--branch` and `--has-worktree` filters
New fields in `FilterOptions`:
- `Branch string` — glob match against `task.Branch` (e.g. `"task/4-*"`, `"feature/*"`)
- `HasWorktree *bool` — filter by worktree presence
- ~30 lines of filter logic using `filepath.Match()` for globs

### Layer 3: `--context` / `-C` auto-filter (signature feature)
Auto-detects current branch, finds matching task, expands to related tasks:
1. Direct match: task whose `branch` == current branch
2. Convention fallback: `parseTaskIDBranch(currentBranch)` → look up by ID
3. Worktree fallback: match `task.Worktree` against cwd
4. Same parent: tasks sharing the same `Parent` ID (siblings)
5. Dependency graph: `DependsOn` upstream + downstream dependents
6. Same claimant: tasks claimed by `$KANBAN_AGENT` (if set)

When not in a worktree: no-op (shows full board). Safe to alias.

### Layer 4: `kbmdx worktrees` command
Cross-references `git worktree list` with task `branch` fields:
- Shows all active worktrees mapped to their tasks
- Detects stale worktree metadata + orphaned worktrees
- Table/compact/JSON output

### Layer 5: `KANBAN_AGENT` environment variable
Optional agent identity for the session:
- `pick --claim` uses `$KANBAN_AGENT` when `--claim` flag omitted
- `--context` includes "my other claimed tasks" in related set

### Layer 6: TUI context switching
- `contextMode bool` toggle on Board, activated with `C` keypress
- Applies same related-task resolution as CLI `--context` in `loadTasks()`
- Status bar shows `[ctx: #4 worktree-access]` when active
- Orthogonal to search/sort — context filter runs before search filter
- Future: `W` keypress opens worktree picker overlay (depends on Layer 1)

## Feature Grouping ("Epic Boards")

Three orthogonal approaches:
- **Parent-based** (recommended): `--parent 7` shows epic + children. `--context` auto-includes siblings.
- **Tag-based**: `--tag semantic-search` for ad-hoc grouping
- **Branch prefix**: `--branch "feature/search-*"` for git-native grouping

## Key Design Decisions

- **Shared board with enforcement** over committed board — real-time coordination is non-negotiable
- **Branch-name convention is load-bearing** — `task/<ID>-*` parsing is always on, not opt-in
- **Opt-in context** (`-C`) on CLI, toggle (`C`) in TUI — don't break existing behavior
- **Medium relatedness depth** by default (match + parent + siblings + direct deps)
- **Glob matching** for branch filters (path-like names; `feature/*` is natural)
- **Auto-populate on pick** closes the metadata gap without manual ceremony

## Dependencies

- Depends on task #4 (worktree-transparent board access) — merged

## Implementation Phases

Phase 1 (git package) → 2 (filters) → 3 (context + enforcement) → 4 (worktrees cmd) → 5 (KANBAN_AGENT) → 6 (TUI)

Each phase is independently shippable. Enforcement levels 0-1 ship with Phase 1+3. Level 2 ships with Phase 3. Level 3 is a separate config schema change (requires migration).
