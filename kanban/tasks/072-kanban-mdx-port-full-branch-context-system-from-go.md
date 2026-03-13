---
id: 72
title: 'kanban-mdx: Port full branch context system from Go (context expansion, board filtering, creation flow)'
status: done
priority: critical
created: 2026-03-12T09:01:42.825822Z
updated: 2026-03-12T17:05:01.619513Z
started: 2026-03-12T09:02:50.932418Z
completed: 2026-03-12T17:04:46.915543Z
tags:
    - kanban-mdx
    - layer-3
class: standard
---

## Problem

kanban-mdx has branch assignment (b/W keys) and worktree board filter (w toggle), but is missing the entire **context model** that makes Go kanban-md's branch/worktree features a workflow tool rather than just metadata bookkeeping.

The Go version has a rich context system built across ~1500 lines in `internal/tui/board.go` and `internal/board/branch_context.go`. The Rust version has ~100 lines of simple branch picker. The gap is substantial.

## Current State (kanban-mdx)

**What exists:**
- `b` key: opens branch picker → assigns branch to active task (flat `Vec<String>`, substring filter)
- `W` key: opens branch picker restricted to worktree branches → assigns branch
- `w` key: toggles board-level filter showing only tasks with worktree metadata
- `pick` command: auto-populates `task.branch` and `task.worktree` from git context
- `branch-check` command: validates branch match (convention + exact), 3-level enforcement
- `worktrees` command: lists worktrees, `--check` detects stale/orphan entries
- Git utilities: `current_branch()`, `list_branches()`, `list_worktree_branches()`, `is_worktree()`, `root_dir()`

**What's missing:**
1. **No context expansion** — no equivalent of Go's `ExpandContext()` (siblings, deps, dependents, same claimant)
2. **No board-level context filtering** — can't filter the TUI board to "tasks related to this branch"
3. **No auto-detect mode** — no "detect my branch → find matching task → show related work"
4. **No branch/worktree creation from TUI** — picker lists existing branches only
5. **No missing branch detection in TUI** — no red `(missing)` warnings for deleted branches
6. **No structured picker items** — Rust uses flat `Vec<String>` vs Go's `contextItem { kind, taskID, branch, missing, label }`
7. **No undo support for branch assignment**

## Implementation Plan

### Phase 1: Context Resolution (new module)

Create `src/board/branch_context.rs` (port from Go `internal/board/branch_context.go`):

```rust
/// Find the task matching a branch name.
/// Fallback chain: exact branch match → task/<ID>-* convention → None.
pub fn resolve_context_task(branch: &str, tasks: &[Task]) -> Option<&Task>

/// Expand a root task ID to related IDs.
/// Includes: root, siblings (same parent), upstream deps,
/// downstream dependents, same-claimant tasks.
pub fn expand_context(root_id: i32, all_tasks: &[Task], agent: &str) -> Vec<i32>
```

**Reference:** Go `internal/board/branch_context.go` (87 lines, clean port)

### Phase 2: Structured Context Items

Replace `branch_list: Vec<String>` with a richer model:

```rust
#[derive(Clone)]
pub struct ContextItem {
    pub kind: ContextKind,  // Auto, Clear, Task, Branch, New
    pub task_id: Option<i32>,
    pub branch: String,
    pub label: String,
    pub missing: bool,  // branch no longer exists in git
}

pub enum ContextKind { Auto, Clear, Task, Branch, New }
```

This enables:
- Showing task ID alongside branch name
- `(missing)` markers for deleted branches  
- "Auto-detect" and "Clear context" meta-options
- "Create: <name>" suggestions when filter doesn't match

### Phase 3: Dual-Purpose Picker

Split the current `AppView::BranchPicker` into two modes (or add a new `AppView::ContextPicker`):

**Context switching** (`C` key / `W` for worktree-only):
- Opens picker with Auto-detect, Clear, task branches, git branches
- Selection filters the board to the expanded context (Phase 1)
- Status bar shows `[ctx: branch-name]` or `[ctx: auto]`
- Pressing `C` again reopens picker (pre-selects current context)

**Branch assignment** (`b` key / `w` for worktree-only):
- Opens picker (minus Auto-detect option, plus "Clear branch")
- Selection sets `task.branch` on the active task
- Records undo entry

Both share the same picker rendering and filter logic, differ only in the execute action.

**Key bindings (matching Go):**
| Key | Action |
|-----|--------|
| `C` | Open context picker (all branches) |
| `W` | Open context picker (worktree branches only) |
| `b` | Open branch assignment picker (all branches) |
| `w` | Toggle worktree filter (existing, keep as-is) |

### Phase 4: Branch/Worktree Creation

Add creation flow to the picker:

1. When filter text doesn't match any existing branch, append `ContextItem { kind: New, branch: filter_text, label: "Create: ..." }`
2. On selection of `New` item → show confirmation dialog (new `AppView::ConfirmBranch`)
3. If worktree mode: `git worktree add ../kb-<name> [-b] <branch>`
4. If branch mode: `git branch <name>` (only if branch doesn't exist)
5. After creation, proceed with the original action (context switch or assignment)

**Reference:** Go `createBranchAndProceed()` at `internal/tui/board.go:3059-3107`

### Phase 5: Board Filtering Integration

Add context state to `App`:

```rust
pub context_mode: bool,        // is context filtering active?
pub context_task_id: i32,      // manually selected root task (0 = auto-detect)
pub context_label: String,     // display label for status bar
```

Modify the task loading/display path:
- When `context_mode` is true and `context_task_id > 0`: expand that task via `expand_context()`
- When `context_mode` is true and `context_task_id == 0`: auto-detect from current branch
- Filter visible tasks to only those in the expanded set
- Apply context filter **before** search filter (orthogonal)

### Phase 6: Missing Branch Detection

When building picker items, cross-reference task branches against `list_branches()`:

```rust
let git_branches: HashSet<String> = list_branches().into_iter().collect();
// For each task with a branch:
let missing = !git_branches.contains(&task.branch);
```

Render missing branches with red `(missing)` prefix in picker.

### Phase 7: Undo Support

Record undo entries for branch assignment changes (Phase 3). Requires the existing undo infrastructure in the Rust TUI to support file snapshots before/after branch writes.

## Key Files to Modify

| File | Changes |
|------|---------|
| `src/board/mod.rs` | Add `branch_context` module |
| `src/board/branch_context.rs` | **New**: `resolve_context_task()`, `expand_context()` |
| `src/tui/app.rs` | Add `ContextItem`, context state fields, `AppView::ContextPicker`, `AppView::ConfirmBranch`, key handlers |
| `src/tui/ui.rs` | Render context picker dialog, confirm dialog, status bar context indicator, missing branch styling |
| `src/util/git.rs` | Add `local_branches()` (for-each-ref, needed for missing detection — currently uses `list_branches()` which uses `git branch`) |

## Reference Files (Go source)

| Go file | What to port |
|---------|-------------|
| `internal/board/branch_context.go` | `ResolveContextTask()`, `ExpandContext()` — clean 87-line port |
| `internal/tui/board.go:2596-2667` | `openContextPicker()` — builds item list |
| `internal/tui/board.go:2669-2790` | `handleContextKey()`, `filteredContextItems()`, `executeContextSelect()` |
| `internal/tui/board.go:2792-2979` | `openAssignContext()`, `handleAssignContextKey()`, `executeAssignContext()` |
| `internal/tui/board.go:3033-3114` | `viewConfirmBranch()`, `handleConfirmBranchKey()`, `createBranchAndProceed()` |
| `internal/tui/board.go:5126-5188` | `viewContextDialog()` — picker rendering |

## Design Decisions to Preserve

1. **Convention matching is always on** — `task/<ID>-*` parsing happens without config
2. **Auto-detect is the default context mode** — `C` → Enter immediately scopes to current branch
3. **Context expansion includes siblings, deps, dependents, same claimant** — not just the single task
4. **Worktree-only mode is a flag on the same picker** — not a separate UI
5. **Branch creation requires y/n confirmation** — never silently create git state
6. **Context filter is orthogonal to search** — both can be active simultaneously

## Design Decisions to Reconsider

1. **Resolution priority**: Go does exact-match-first, Rust does convention-first — standardize on exact-match-first (Go's behavior) since explicit metadata should win
2. **Auto-population in `pick`**: currently auto-sets `branch: main` which is meaningless — consider only auto-populating when branch matches `task/<ID>-*` convention
3. **`w`/`W` key confusion**: both versions overload these differently — consider `W` for worktree context picker, keep `w` as worktree toggle (current Rust behavior)

## Acceptance Criteria

- [ ] `C` key opens context picker with Auto-detect, Clear, task branches, git branches
- [ ] Selecting a branch context filters the board to expanded related tasks
- [ ] Auto-detect mode works: detects current branch → finds task → expands context
- [ ] `b` key opens branch assignment picker (separate from context)
- [ ] `W` key opens context picker restricted to worktree branches
- [ ] Branch creation flow: type new name → "Create: ..." option → confirm → git creates
- [ ] Missing branches shown with `(missing)` marker in pickers
- [ ] Status bar shows `[ctx: branch-name]` when context is active
- [ ] Context filter applies before search filter
- [ ] Undo support for branch assignment changes

## Supersedes

- Task #55 (TUI: Add C key for instant branch context toggle) — this task is a superset
- Task #7 (Add context picker modal to TUI) — already done in Go, this is the Rust port

## Dependencies

- Task #46 (TUI: Assign branch/worktree to task) — done, provides the foundation
- Task #57 (TUI: Add quick context toggles) — done, provides w/W infrastructure

Completed via orchestrated parallel implementation. Two agents: (1) branch_context.rs + git.rs + mod.rs, (2) app.rs + ui.rs. All 353 tests pass, clean compile.
