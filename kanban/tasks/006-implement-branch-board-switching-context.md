---
id: 6
title: Implement branch board switching (context, enforcement, TUI)
status: done
priority: medium
created: 2026-03-10T14:17:34.894059Z
updated: 2026-03-11T06:53:13.861644Z
started: 2026-03-10T14:59:37.655237Z
completed: 2026-03-10T14:59:37.655237Z
tags:
- layer-3
class: standard
branch: main
---

## Summary

Implement the branch board switching design from task #5 (references). Adds an `internal/git` package, branch/worktree filters, `--context`/`-C` auto-filter, enforcement stack, `worktrees` command, `KANBAN_AGENT` env var, and TUI context toggle.

**Design reference:** Task #5, `docs/research/2026-03-10-branch-board-switching.md`
**Depends on:** Task #4 (worktree-transparent board access) — merged

## Async Wave Implementation Plan

Six waves, maximizing parallelism. ~550 new lines across all waves.

---

### Wave 1 — Foundation: `internal/git` package [sequential, prerequisite]

**Agent A: `internal/git` package** (worktree, ~150 lines code + ~200 lines tests)

Create `internal/git/git.go` and `internal/git/git_test.go`:

```go
package git

func CurrentBranch(dir string) (string, error)
// Read .git/HEAD → parse "ref: refs/heads/<branch>".
// For worktrees: follow .git file → gitdir path → <gitdir>/HEAD.
// Return "" on detached HEAD.

func ListWorktrees(repoRoot string) ([]Worktree, error)
// Shell out to: git -C <dir> worktree list --porcelain
// Parse: worktree <path> / HEAD <sha> / branch refs/heads/<name> / blank separator.

func IsWorktree(dir string) (bool, error)
// Walk upward for .git. If it's a file (not dir), return true.

func ParseTaskIDBranch(branch string) (int, bool)
// Parse "task/<ID>-<description>" → extract ID.
// Return 0, false on no match.
```

Tests: synthetic `.git` file + directory structures in t.TempDir(). No real git repos needed for CurrentBranch/IsWorktree/ParseTaskIDBranch. Real `git init` + `git worktree add` for ListWorktrees integration test.

**Validation:** `go test ./internal/git/ && golangci-lint run ./internal/git/`

---

### Wave 2 — Parallel: Filters + Worktrees Command + KANBAN_AGENT [3 agents, after Wave 1]

**Agent B: Branch/worktree filters** (worktree, ~55 lines)

Files: `internal/board/filter.go`, `internal/board/filter_test.go`, `cmd/list.go`

1. Add to `FilterOptions` (after `Class` field, line 26):
   - `Branch string` — glob match via `filepath.Match()`
   - `HasWorktree *bool`
2. Add `matchesBranchGlob()` helper
3. Add filter checks in `matchesCoreFilter()` (before `return true`, line 74)
4. Add CLI flags on list: `--branch`, `--has-worktree`, `--no-worktree`
5. Populate filter from flags in `runList()` (after line 75)
6. Add table-driven tests for glob matching edge cases

**Validation:** `go test ./internal/board/ ./cmd/ -run "TestFilter|TestList"`

---

**Agent C: `kanban-md worktrees` command** (worktree, ~120 lines)

Files: NEW `cmd/worktrees.go`, `cmd/worktrees_test.go`

1. New `worktreesCmd` registered on root
2. Calls `git.ListWorktrees(repoRoot)` to get active worktrees
3. Loads all tasks, matches `task.Branch` against worktree branches
4. Detects stale metadata: task has `worktree` field but path not in git list
5. Detects orphans: worktree exists but no task has matching branch
6. Output formats: table (default), compact, JSON
7. Summary line: "N active worktrees, M tasks in progress"

Table output format:
```
WORKTREE                  BRANCH                   TASK                                 STATUS
../kanban-md-task-4       task/4-worktree-access   #4 Worktree-transparent board access done
(main)                    main                     —                                    —
```

**Validation:** `go test ./cmd/ -run TestWorktrees`

---

**Agent D: `KANBAN_AGENT` env var** (worktree, ~30 lines)

Files: `cmd/pick.go`, `cmd/root.go`

1. In `cmd/pick.go` `runPick()`: if `--claim` flag is empty, fall back to `os.Getenv("KANBAN_AGENT")`
2. Same fallback in any other command that accepts `--claim` (move, edit)
3. Document in `--help` text: "Falls back to $KANBAN_AGENT if set"
4. Add tests: with/without env var, flag takes precedence over env

**Validation:** `go test ./cmd/ -run "TestPick|TestMove|TestEdit"`

---

### Wave 3 — Parallel: Context Filter + Enforcement [2 agents, after Wave 2]

**Agent E: `--context`/`-C` auto-filter + context expansion** (worktree, ~130 lines)

Files: NEW `internal/board/context_filter.go`, `internal/board/context_filter_test.go`, `cmd/list.go`

1. Create `ExpandContext(cfg, rootTaskID int, allTasks []*task.Task, agentName string) []int`:
   - Returns IDs of related tasks in relatedness order
   - Resolution: direct match → same parent (siblings) → DependsOn upstream → downstream dependents → same claimant
2. Create `ResolveContextTask(branch string, tasks []*task.Task) *task.Task`:
   - Fallback chain: exact branch match → `ParseTaskIDBranch` → worktree path match
   - Returns nil if no match (caller shows full board)
3. Add `--context`/`-C` flag on `list` command (bool flag, not string)
4. In `runList()`: if context flag set, call `git.CurrentBranch()`, then `ResolveContextTask()`, then `ExpandContext()`, populate `filter.IDs` with result
5. If no match: print hint to stderr, show full board
6. Table-driven tests covering all fallback chain paths

**Validation:** `go test ./internal/board/ -run TestContext && go test ./cmd/ -run TestList`

---

**Agent F: Branch mismatch enforcement (Levels 0-2)** (worktree, ~50 lines)

Files: `cmd/edit.go`, `cmd/move.go`, `cmd/pick.go`

1. Create shared helper in `cmd/branch_check.go`:
   ```go
   func warnBranchMismatch(taskID int, taskBranch string) {
       branch, err := git.CurrentBranch(".")
       if err != nil || branch == "" { return }
       // Level 0: convention match
       if id, ok := git.ParseTaskIDBranch(branch); ok && id == taskID { return }
       // Exact match
       if taskBranch == branch { return }
       // Mismatch
       if taskBranch != "" {
           fmt.Fprintf(os.Stderr, "Warning: task #%d is on branch %s but you're on %s\n",
               taskID, taskBranch, branch)
       }
   }
   ```
2. Call from `executeEdit()` (after line 113, after claim validation)
3. Call from `executeMove()` (after line 100, only for non-terminal statuses)
4. In `executePick()`: auto-populate `task.Branch` and `task.Worktree` from git context (Level 1)
5. Tests: mock git.CurrentBranch via function variable for testability

**Validation:** `go test ./cmd/ -run "TestEdit|TestMove|TestPick"`

---

### Wave 4 — Sequential: TUI Context Mode [after Wave 3]

**Agent G: TUI `contextMode` toggle** (worktree, ~45 lines)

Files: `internal/tui/board.go`, `internal/tui/board_test.go`

1. Add to Board struct (after `listMode bool`, line 419):
   - `contextMode bool`
   - `contextTaskID int` (for status bar display)
   - `contextLabel string` (e.g. "#4 worktree-access")
2. Add `C` keypress handler in `handleBoardKey()`:
   - Toggle `contextMode`, call `loadTasks()` to refresh
3. In `loadTasks()` (after `visibleTasks := tasks`, line 2521):
   - If `contextMode`: call `git.CurrentBranch()`, `board.ResolveContextTask()`, `board.ExpandContext()`, filter `visibleTasks` to returned IDs
   - Store matched task info in `contextTaskID`/`contextLabel`
4. In status bar rendering: show `[ctx: #4 worktree-access]` when active
5. Add to help view keybindings list
6. Behavioral test: `sendKey(b, "C")`, verify View() output is filtered
7. Update golden files if snapshot tests affected: `go test ./internal/tui/ -run TestSnapshot -update`

**Validation:** `go test ./internal/tui/ && go test ./internal/tui/ -run TestSnapshot`

---

### Wave 5 — Sequential: Integration + Config [after Wave 4]

**Agent H: Integration tests + Level 3 enforcement** (worktree, ~80 lines)

Files: `e2e/cli_test.go`, `internal/config/config.go`, `internal/config/defaults.go`, `internal/config/migrate.go`, `internal/config/compat_test.go`

1. E2E test: full workflow — init board, create task, create real worktree, run `list -C` from worktree, verify context filtering works end-to-end
2. E2E test: `kanban-md worktrees` output with real worktrees
3. E2E test: `KANBAN_AGENT` env var with pick
4. Config schema change: add `require_branch: bool` to status entry
5. Bump `CurrentVersion`, add migration, add compat test + fixture
6. Enforcement in `cmd/edit.go` and `cmd/move.go`: if `require_branch` and mismatch, return error (not warning) unless `--force`
7. Add `--force` flag to edit/move if not already present
8. Update README.md with new features

**Validation:** `go test ./... && golangci-lint run ./...`

---

## Wave Dependency Graph

```
Wave 1: [A: internal/git]
            │
            ├──────────────────────┐
            │                      │
Wave 2: [B: filters]  [C: worktrees cmd]  [D: KANBAN_AGENT]
            │                      │               │
            ├──────────────────────┘               │
            │                                      │
Wave 3: [E: --context + expansion]  [F: enforcement L0-2]
            │                          │
            ├──────────────────────────┘
            │
Wave 4: [G: TUI contextMode]
            │
Wave 5: [H: integration + L3 enforcement + config migration]
```

## Estimated Scope Per Wave

| Wave | Agents | Parallel? | New Lines | Key Risk |
|------|--------|-----------|-----------|----------|
| 1    | 1      | No        | ~350      | .git file parsing edge cases |
| 2    | 3      | Yes       | ~205      | Glob matching semantics |
| 3    | 2      | Yes       | ~180      | Context expansion correctness |
| 4    | 1      | No        | ~45       | Golden file updates |
| 5    | 1      | No        | ~80       | Config migration + E2E reliability |
| **Total** | **8 agents** | | **~860 lines** | |

## Acceptance Criteria

- [ ] `kanban-md list --branch "task/4-*"` filters by branch glob
- [ ] `kanban-md list -C` from a worktree shows related tasks only
- [ ] `kanban-md list -C` from main shows full board
- [ ] `kanban-md pick --claim <agent>` auto-sets branch + worktree fields
- [ ] Branch mismatch warning on edit/move when on wrong branch
- [ ] `kanban-md worktrees` shows active worktrees mapped to tasks
- [ ] `KANBAN_AGENT=x kanban-md pick` uses env var for claim
- [ ] TUI: `C` toggles context mode, status bar shows indicator
- [ ] `require_branch: true` in config refuses mismatched mutations
- [ ] All existing tests pass, no regressions
- [ ] README updated with new features
