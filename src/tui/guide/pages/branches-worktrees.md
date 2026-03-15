# Branches & Worktrees

Three orthogonal features that compose into a single workflow:

| Feature | Role | Stored | Question it answers |
|---------|------|--------|---------------------|
| **Branch** | Identity | `task.branch` | *Which code stream?* |
| **Worktree** | Isolation | `task.worktree` | *Where is the code?* |
| **Context** | Attention | Ephemeral (TUI state) | *What's relevant now?* |

Each works independently. Together they form a resolution chain:

```
 worktree checkout → git branch → resolve to task → expand context → filtered board
      (physical)      (identity)    (lookup)          (graph walk)     (view)
```

## Board home vs worktree

Board home and worktrees serve different purposes. Never edit code
in board home; never run `kbmdx` from a worktree.

```
 Board Home (canonical repo)         Worktree (isolated copy)
 ───────────────────────────         ───────────────────────
 .kbmdx/                            src/
   config.toml                      tests/
   tasks/                           ...
     001-my-task.md
 src/                  ◄── merge ─── (same repo, different branch)
 tests/

 Runs:                               Runs:
   kbmdx <any command>                 code changes
   git merge                           git add / git commit
   git worktree add/remove             tests / linter
```

A branch can exist without a worktree (work in main checkout).
A worktree can exist without a task branch (experiments).
The separation is intentional: branch is the logical link,
worktree is the physical workspace.

## Branch naming convention

```
task/<ID>-<kebab-description>
 │     │         │
 │     │         └─ human-readable slug
 │     └─ task ID for automatic matching
 └─ prefix for convention-based resolution
```

The regex `^task/(\d+)(?:-|$)` parses the ID. This convention
is the zero-config glue: even without explicit `--branch`
metadata, context resolution works via convention match.

## Lifecycle

```
 1. CLAIM     pick --claim <agent>
                sets claimed_by

 2. ISOLATE   git worktree add ../kbmdx-task-<ID> -b task/<ID>-desc
              kbmdx edit <ID> --branch ... --worktree ...
                creates branch + worktree, records both

 3. FOCUS     C in TUI (or kbmdx context)
                reads branch → resolves task → expands context

 4. WORK      worktree: implement, test, commit
              board-home: progress notes via --append-body

 5. MERGE     cd board-home && git merge task/<ID>-desc

 6. CLEANUP   kbmdx edit <ID> --clear-branch --clear-worktree
              git worktree remove / git branch -d
```

Branch and worktree are established early (steps 1-2), cleaned
up late (step 6). Context is the focusing lens used during
active work (steps 3-4).

## Context resolution

Given a branch name, find its task:

```
 Branch name
       │
       v
 ┌──────────────────┐     ┌──────────────┐
 │ Exact match?     │─yes─│ Return task  │
 │ task.branch ==   │     └──────────────┘
 │ branch name      │
 └────────┬─────────┘
          │ no
          v
 ┌──────────────────┐     ┌──────────────┐
 │ Convention match?│─yes─│ Return task  │
 │ parse task/<ID>  │     └──────────────┘
 │ from branch name │
 └────────┬─────────┘
          │ no
          v
      Not found
```

Exact match wins over convention. This lets you override the
convention by setting `task.branch` to any arbitrary branch name.

## Context expansion

Once a root task is resolved, the context expands to include
all related work:

```
 Root task (#10)
   │
   ├── Parent (#1) + siblings (#11, #12)     ← task hierarchy
   ├── Upstream deps (#20)                    ← tasks #10 depends on
   ├── Downstream dependents (#30)            ← tasks that depend on #10
   └── Same-claimant tasks (#40, #41)         ← all work by this agent
```

The result is deduplicated and sorted. This means pressing `C`
doesn't just show one task — it shows the full neighbourhood
of related work.

## TUI keys

### Worktree filter (`w`)

Toggle: show only tasks with a non-empty `worktree` field.
Press `w` to toggle, `Esc` to clear. Simple binary filter.

### Context picker (`C` / `W`)

Expands a branch into its full context set, then filters the
board to only those tasks.

- `C` lists all local git branches
- `W` lists only branches with active worktrees
- Type to filter, `Enter` to select, `Esc` to cancel
- "Auto-detect" option reads the current branch automatically

### Branch assignment (`b`)

Select a task, press `b`, pick a branch (or type a new name).
Sets `task.branch` — the identity link used by context resolution.

## Staleness

Branch and worktree metadata can go stale (branch merged,
worktree deleted). Context cannot — it's computed live from
the current task graph.

Audit worktree metadata:

```bash
kbmdx worktrees --check
```

Reports two conditions:
- **STALE**: task references a worktree path that no longer exists
- **ORPHAN**: git worktree exists but no task claims it

## Esc/q layering

The TUI clears filters in order before quitting:

```
 q/Esc pressed
    │
    ├── text selection active?  → clear selection
    ├── search active?          → clear search (incl. @blocked)
    ├── worktree filter ON?     → turn off worktree filter
    ├── context filter ON?      → clear context
    └── nothing active?         → quit TUI
```
