---
id: 81
title: Workflows & Onboarding Reference
status: references
priority: low
created: '2026-03-13T14:28:57.140246Z'
updated: '2026-03-13T15:55:06.443586Z'
tags:
- reference
class: standard
---

Comprehensive visual reference for kbmdx workflows, state machines, and coordination mechanics. Covers all major subsystems with ASCII diagrams and quick-reference tables.

**File:** `kanban-mdx/src/skill/skills/kbmdx/references/workflows.md`

## Sections
1. **Task Lifecycle State Machine** — status flow with enforcement annotations (require_claim, require_branch, auto-timestamps, WIP limits, --force bypass)
2. **Agent Session Lifecycle** — full mandatory loop: agent-name → pick → worktree → implement → merge → done → cleanup, plus parking/handoff/resume branches
3. **Worktree Workflow** — board-home vs worktree relationship, field tracking, branch naming convention, stale/orphan detection
4. **Claim Coordination Model** — atomic pick, timeout/renewal, never-steal rule, KANBAN_AGENT env var
5. **Context Resolution** — branch-to-task mapping (exact + convention match) and context expansion (parent, siblings, deps, dependents, same-claimant)
6. **TUI Interface Guide** — comprehensive TUI documentation:
   - Context, worktree & branch management (worktree filter, context picker, branch assignment)
   - Esc/q layering behavior (how filters stack and clear)
   - Board view keybindings (navigation, task actions, filtering, display, reader panel, theme, undo)
   - Detail view keybindings (scrolling, heading navigation, find-in-document)
   - Move dialog, search, and create/edit wizard keybindings
   - TUI workflow recipes (worktree focus, context focus, branch assignment, triage, quick navigation)
7. **Output Format Decision Tree** — table vs compact vs json vs --prompt with token cost comparison
8. **Configuration Enforcement Rules** — per-status rules table with --force interaction matrix
9. **Class of Service & Priority Sorting** — pick algorithm: class order → priority index → ID, with worked example
10. **Dependency & Blocking** — manual vs dependency blocking, how pick filters both
11. **Quick Reference** — common workflow command sequences (session start, working, finishing, parking, awareness)

## Related docs
- CLI command reference: `kanban-mdx/src/skill/skills/kbmdx/SKILL.md`
- JSON schemas: `kanban-mdx/src/skill/skills/kbmdx/references/json-schemas.md`
- Development workflow: `kanban-mdx/src/skill/skills/kanban-based-development/SKILL.md`

# Workflows & Onboarding Reference

Visual reference for kbmdx workflows, state machines, and coordination
mechanics. Intended for AI agents being onboarded to a kanban-mdx project
and human developers setting up multi-agent workflows.

For the CLI command reference, see [../SKILL.md](../SKILL.md).
For JSON output schemas, see [json-schemas.md](json-schemas.md).

---

## 1. Task Lifecycle State Machine

Tasks flow through statuses left-to-right. Some statuses enforce rules
(claims, branches) that must be satisfied before a task can enter them.

```
                       require_claim
                       ┌──────┴──────┐
  ┌─────────┐  ┌──────┐  ┌───────────┐  ┌────────┐  ┌──────┐  ┌──────────┐
  │ backlog │─→│ todo │─→│in-progress│─→│ review │─→│ done │─→│ archived │
  └─────────┘  └──────┘  └───────────┘  └────────┘  └──────┘  └──────────┘
   initial                                            terminal   terminal
```

### Status rules (defaults)

| Status        | require_claim | require_branch | Terminal? | Notes                       |
|---------------|:---:|:---:|:---:|--------------------------------------------|
| backlog       |     |     |     | Initial status for new tasks               |
| todo          |     |     |     | Ready to work on                           |
| in-progress   |  ✓  |     |     | Actively being worked                      |
| references    |     |     |     | Reference material (not workflow)          |
| review        |  ✓  |     |     | Waiting room: merges, decisions, handoffs  |
| done          |     |     |  ✓  | Merged to main, checks pass               |
| archived      |     |     |  ✓  | Soft-deleted                               |

### Auto-timestamps

```
                ┌── started ──┐                    ┌── completed ──┐
  backlog → todo → in-progress → review → done
                  ▲ first move from               ▲ move to any
                    initial status                  terminal status
```

- `started`: set automatically on the **first** move away from the initial
  status (backlog). Not reset on subsequent moves.
- `completed`: set automatically on move to any **terminal** status
  (done, archived).
- `updated`: set on every mutation (move, edit, etc.).

### Enforcement & bypass

- **require_claim**: task must have a non-empty `claimed_by` to enter
  the status. Use `--claim <agent>` on the move command to satisfy this.
- **require_branch**: task must have a `branch` field matching the current
  git branch. Currently not set by default; boards can enable it per-status
  in `config.yml`.
- **WIP limits**: per-column limits configured in `config.yml`. Move fails
  if the target column is at capacity.
- **`--force`**: bypasses `require_branch` and WIP limit enforcement.
  Does *not* bypass `require_claim`.

---

## 2. Agent Session Lifecycle

Every agent session follows this loop. The board is shared — multiple
agents may work concurrently.

```
┌─────────────────────────────────────────────────────────┐
│                    SESSION START                         │
│                                                         │
│  kbmdx agent-name          → e.g. "frost-maple"        │
│  kbmdx board --compact     → orient: active/blocked    │
└───────────────────────┬─────────────────────────────────┘
                        │
                        ▼
          ┌─────────────────────────────┐
          │     PICK & CLAIM            │
          │                             │
          │  kbmdx pick --claim <agent> │◄──────────────┐
          │    --status todo            │               │
          │    --move in-progress       │               │
          │                             │               │
          │  kbmdx show <ID>            │               │
          └─────────────┬───────────────┘               │
                        │                               │
                        ▼                               │
          ┌─────────────────────────────┐               │
          │     CREATE WORKTREE         │               │
          │                             │               │
          │  git worktree add           │               │
          │    ../kbmdx-task-<ID>       │               │
          │    -b task/<ID>-kebab       │               │
          │                             │               │
          │  kbmdx edit <ID>            │               │
          │    --branch "task/<ID>-…"   │               │
          │    --worktree "../kbmdx-…"  │               │
          │    --claim <agent>          │               │
          └─────────────┬───────────────┘               │
                        │                               │
                        ▼                               │
          ┌─────────────────────────────┐               │
          │     IMPLEMENT & TEST        │               │
          │     (in worktree)           │               │
          │                             │               │
          │  Write code, run tests      │               │
          │  git commit -m "feat: …"    │               │
          │                             │               │
          │  Progress notes (optional): │               │
          │  kbmdx edit <ID>            │               │
          │    -a "status…" -t          │               │
          │    --claim <agent>          │               │
          └────────┬──────────┬─────────┘               │
                   │          │                         │
              ┌────┘          └────┐                    │
              ▼ success            ▼ blocked            │
   ┌──────────────────┐  ┌─────────────────────┐       │
   │  MERGE TO MAIN   │  │  HANDOFF / PARK     │       │
   │  (board home)    │  │  (board home)        │       │
   │                  │  │                      │       │
   │  git switch main │  │  kbmdx handoff <ID>  │       │
   │  git merge       │  │    --claim <agent>   │       │
   │    task/<ID>-…   │  │    --block "reason"  │       │
   │  run tests       │  │    --note "…"        │       │
   └────────┬─────────┘  │    -t --release      │       │
            │             └──────────┬──────────┘       │
            ▼                        │                  │
   ┌──────────────────┐              │                  │
   │  MARK DONE       │              │                  │
   │                  │              │ task parked       │
   │  kbmdx edit <ID> │              │ in review        │
   │    --release     │              │                  │
   │    --clear-branch│              └──────────────────►│
   │    --clear-wt    │                 pick next       │
   │                  │                                 │
   │  kbmdx move <ID> │                                 │
   │    done          │                                 │
   └────────┬─────────┘                                 │
            │                                           │
            ▼                                           │
   ┌──────────────────┐                                 │
   │  CLEANUP         │                                 │
   │                  │                                 │
   │  git worktree    │                                 │
   │    remove …      │                                 │
   │  git branch -d … │                                 │
   └────────┬─────────┘                                 │
            └───────────────────────────────────────────┘
                           pick next task
```

### Resuming a parked task

When the blocker is resolved and you want to continue:

```
kbmdx edit <ID> --claim <agent>                 # re-claim
kbmdx edit <ID> --unblock --claim <agent>       # if it was blocked
kbmdx move <ID> in-progress --claim <agent>     # back to work
```

---

## 3. Worktree Workflow

Board home and worktrees serve different purposes. **Never edit code in
board home**; never run `kbmdx` from a worktree (it won't find the board).

```
 Board Home (canonical repo)              Worktree (isolated copy)
 ─────────────────────────────            ────────────────────────
 kanban/                                  src/
   config.yml                             tests/
   tasks/                                 ...
     001-my-task.md                        (same repo, different branch)
 src/                     ◄── merge ───
 tests/

 Runs:                                    Runs:
   kbmdx <any command>                      code changes
   git merge                                git add / git commit
   git worktree add/remove                  tests / linter
```

### Branch naming convention

```
task/<ID>-<kebab-description>
 │     │         │
 │     │         └─ human-readable slug (e.g. "add-wip-limits")
 │     └─ task ID for automatic matching
 └─ prefix for convention-based resolution
```

The branch name is parsed by the regex `^task/(\d+)(?:-|$)` for automatic
task-to-branch matching.

### Task fields tracking worktrees

| Field      | Set by                     | Purpose                         |
|------------|----------------------------|----------------------------------|
| `branch`   | `kbmdx edit --branch`      | Git branch name for the task    |
| `worktree` | `kbmdx edit --worktree`    | Absolute/relative worktree path |

Both are auto-populated by `pick` when git context is available.

### Stale & orphan detection

```bash
kbmdx worktrees --check
```

Detects two problems:

- **Stale metadata**: tasks referencing worktrees/branches that no longer
  exist on disk
- **Orphan worktrees**: active git worktrees not referenced by any task

---

## 4. Claim Coordination Model

Claims are the primary coordination primitive. They prevent two agents
from working on the same task simultaneously.

```
  Agent A                     Board                      Agent B
  ─────────                   ─────                      ─────────
  pick --claim A              │
    ├─ filter unclaimed  ───► │ task #5 unclaimed
    ├─ set claimed_by="A" ──► │ task #5 claimed by A
    └─ return #5              │                          pick --claim B
                              │                            ├─ filter unclaimed
                              │ #5 already claimed    ◄────┤
                              │                            └─ return #6 instead
```

### Claim lifecycle

```
  unclaimed ──► claimed ──► expired ──► unclaimed
                  │                       ▲
                  ├── --release ───────────┘
                  └── timeout (default 1h) ┘
```

### Key rules

| Rule                  | Mechanism                              |
|-----------------------|----------------------------------------|
| Atomic pick + claim   | `pick --claim` is a single operation   |
| No stealing           | Never claim a task claimed by another  |
| Timeout expiration    | Claims expire after `claim_timeout`    |
| Renewal               | `edit --claim <agent>` renews timestamp|
| Release               | `edit --release` clears the claim      |
| Env var fallback      | `$KANBAN_AGENT` used if `--claim` omitted |

### Claim timeout

The `claim_timeout` config value (default: `"1h"`) determines when an
unreleased claim expires. After expiration, the task becomes pickable
again by any agent.

---

## 5. Context Resolution

Context resolution maps the current git branch to a task, then expands
to show related work. Used by `list --context` and TUI context filtering.

### Branch → task resolution

```
  Current git branch
        │
        ▼
  ┌──────────────────┐     ┌──────────────┐
  │ Exact match?     │─yes─│ Return task  │
  │ task.branch ==   │     └──────────────┘
  │ current branch   │
  └────────┬─────────┘
           │ no
           ▼
  ┌──────────────────┐     ┌──────────────┐
  │ Convention match?│─yes─│ Return task  │
  │ parse task/<ID>  │     └──────────────┘
  │ from branch name │
  └────────┬─────────┘
           │ no
           ▼
       Not found
```

### Context expansion

Once a root task is resolved, `--context` expands to related tasks:

```
                    ┌── upstream dependencies ──┐
                    │    (root.depends_on)       │
                    │                            │
  ┌─────────────┐  │  ┌──────────────────────┐  │  ┌─────────────┐
  │   parent    │──┼──│     root task        │──┼──│ downstream  │
  │  + siblings │  │  │  (current branch)    │  │  │ dependents  │
  └─────────────┘  │  └──────────────────────┘  │  └─────────────┘
                    │            │               │
                    └────────────┼───────────────┘
                                 │
                    ┌────────────▼───────────────┐
                    │  same-claimant tasks       │
                    │  (tasks claimed by same    │
                    │   agent as root task)      │
                    └────────────────────────────┘
```

**Included in context:**
- The root task itself
- Parent task and its other children (siblings)
- Upstream dependencies (tasks the root `depends_on`)
- Downstream dependents (tasks that `depend_on` the root)
- Tasks claimed by the same agent

---

## 6. TUI Interface Guide

Launch the TUI with `kbmdx tui`. The interface is a full board manager
with vim-style navigation, context/worktree awareness, and inline editing.

### View state machine

```
                    ┌─────────────────┐
                    │                 │
            ┌──────│     Board       │──────┐
            │      │  (default view) │      │
            │      └──┬──┬──┬──┬──┬──┘      │
            │         │  │  │  │  │         │
   Enter    │    /,f  │  m  │  d  │  c      │  ?,q
            │         │     │     │         │
            ▼         ▼     ▼     ▼         ▼
     ┌──────────┐ ┌──────┐ ┌──────────┐ ┌────────┐ ┌──────┐
     │  Detail  │ │Search│ │ConfirmDel│ │CreateWiz│ │ Help │
     │          │ │      │ │          │ │         │ │      │
     │  Esc ◄───│ │Esc ◄─│ │  y/n ◄──│ │ 4 steps │ │Esc ◄─│
     └──┬───────┘ └──────┘ └──────────┘ └─────────┘ └──────┘
        │
        │  m
        ▼
     ┌──────────┐
     │ MoveTask │
     │  (picker)│
     │  Esc ◄───│
     └──────────┘

  Additional overlays (from Board or Detail):
    C  → ContextPicker (all branches)
    W  → ContextPicker (worktree branches only)
    b  → ContextPicker (assign branch mode)
    :  → Goto (task ID input)
```

### Context, worktree & branch management

The TUI provides three related but distinct features for working with
git branches and worktrees. Understanding the difference is key.

#### Worktree filter (`w` — instant toggle)

Filters the board to show **only tasks that have an active worktree**.
Useful when you have many tasks but want to focus on the ones with
active development in progress.

```
  Board (all tasks)                Board (worktree filter ON)
  ┌────────┬────────┐             ┌────────┬────────┐
  │ todo   │ in-prog│             │ todo   │ in-prog│
  │        │        │             │        │        │
  │ #10    │ #12 ◄──│─ worktree   │        │ #12 ◄──│─ shown (has worktree)
  │ #11    │ #15    │             │        │        │
  │ #13    │ #18 ◄──│─ worktree   │        │ #18 ◄──│─ shown (has worktree)
  └────────┴────────┘             └────────┴────────┘
```

- Press `w` to toggle on/off
- Press `Esc` or `q` to clear (clears worktree filter before quitting)
- Status bar shows "Worktree filter: ON/OFF"

#### Context picker (`C` / `W` — filter by branch)

Opens a picker to filter the entire board to tasks **related to a
specific branch**. Uses the context expansion algorithm (section 5)
to show the root task plus its parent, siblings, dependencies,
dependents, and same-claimant tasks.

```
  ┌─────────────────────────────────────┐
  │ Context Picker                       │
  │                                      │
  │ > Auto-detect (current branch)       │
  │   Clear context                      │
  │   #12 task/12-add-wip-limits         │
  │   #18 task/18-fix-search             │
  │   main                               │
  │   feature/refactor                   │
  │                                      │
  │  j/k:navigate  Enter:select  Esc:cancel
  └─────────────────────────────────────┘
```

- `C` opens the picker showing **all local git branches**
- `W` opens the picker showing **only branches with active worktrees**
- Type to filter the list (fuzzy matching on branch name)
- Select "Auto-detect" to track the current git branch automatically
- Select "Clear context" to remove the filter
- Press `Esc` or `q` to clear context (clears before quitting)

**Items in the picker:**
- **Auto-detect**: filters by whatever branch is currently checked out
- **Clear context**: removes the branch filter entirely
- **Task branches** (e.g. `#12 task/12-desc`): branches with associated tasks
- **Orphan branches** (e.g. `feature/refactor`): git branches not linked
  to any task
- **Create** (appears when typing a name that doesn't match): creates a
  new branch (or worktree if opened with `W`) and selects it

#### Branch assignment (`b` — assign branch to task)

Opens the same picker but in **assign mode**: instead of filtering the
board, it sets the selected task's `branch` field.

```
  Select a task → press b → pick a branch → task.branch is set
```

- `b` opens the picker in assign-branch mode (all branches)
- Type to filter; if no match exists, "Create: <name>" appears
- `Enter` assigns the branch to the currently selected task
- `Backspace` on empty filter clears the branch from the task
- Selecting "Clear branch" removes the branch assignment

#### How Esc/q layering works

The TUI clears filters in order before quitting:

```
  q/Esc pressed
     │
     ├── text selection active?  → clear selection
     ├── search query active?    → clear search
     ├── worktree filter ON?     → turn off worktree filter
     ├── context filter ON?      → clear context
     └── nothing active?         → quit TUI
```

This means you can layer multiple filters (e.g. context + search) and
peel them off one at a time with `Esc`.

### Board view keybindings

#### Navigation

| Key             | Action                                    |
|-----------------|-------------------------------------------|
| `h`/`←`/`{`    | Move to previous column                   |
| `l`/`→`/`}`    | Move to next column                       |
| `j`/`↓`/`]`    | Move cursor down                          |
| `k`/`↑`/`[`    | Move cursor up                            |
| `g`/`Home`      | Jump to top of column                     |
| `G`/`End`       | Jump to bottom of column                  |
| `J`/`K`         | Half-page down/up                         |
| `Ctrl+j`/`k`   | Full page down/up (or scroll reader)      |
| `:`             | Go to task by ID                          |

#### Task actions

| Key             | Action                                    |
|-----------------|-------------------------------------------|
| `Enter`         | Open task detail                          |
| `c`             | Create new task (4-step wizard)           |
| `e`             | Edit selected task (same wizard)          |
| `d`             | Delete task (confirmation dialog)         |
| `m`             | Move task (status picker)                 |
| `+`/`=`         | Raise priority                            |
| `-`/`_`         | Lower priority                            |
| `o`             | Open task in `$EDITOR`                    |
| `y`             | Yank task content to clipboard            |
| `Y`             | Yank task file path to clipboard          |

#### Filtering & search

| Key             | Action                                    |
|-----------------|-------------------------------------------|
| `/` or `f`      | Open search (filters board live)          |
| `w`             | Toggle worktree filter                    |
| `C`             | Context picker (all branches)             |
| `W`             | Context picker (worktree branches)        |
| `b`             | Assign branch to selected task            |

#### Display

| Key             | Action                                    |
|-----------------|-------------------------------------------|
| `s`/`S`         | Cycle sort mode forward/reverse           |
| `a`             | Toggle age display (created/updated)      |
| `v`             | Toggle text selection mode                |
| `V`             | Toggle cards/list view                    |
| `x`             | Collapse/expand current column            |
| `X`             | Expand all columns                        |
| `1`-`9`         | Solo-expand column N                      |
| `Shift+1`-`9`   | Toggle-collapse column N                  |

#### Reader panel

| Key             | Action                                    |
|-----------------|-------------------------------------------|
| `R`             | Toggle reader panel                       |
| `<`/`>`         | Narrow/widen reader panel                 |
| `z`/`Z`         | Fold deeper/shallower (headings)          |
| `(`/`)`         | Previous/next heading (any level)         |
| `'`/`"`         | Next/previous `##` heading               |
| `Alt+[`/`]`    | Previous/next `##` heading               |

#### Theme & display

| Key             | Action                                    |
|-----------------|-------------------------------------------|
| `t`             | Cycle color theme                         |
| `T`             | Reset theme adjustments                   |
| `.`/`,`         | Brightness up/down                        |
| `Ctrl+.`/`,`   | Saturation up/down                        |

#### Undo & utility

| Key             | Action                                    |
|-----------------|-------------------------------------------|
| `u` / `Ctrl+z`  | Undo last change                          |
| `Ctrl+r`        | Redo last undo                            |
| `r`             | Refresh board from disk                   |
| `?`             | Help overlay                              |
| `q`/`Esc`       | Quit (or clear active filter first)       |

### Detail view keybindings

Opened with `Enter` from the board. Shows the full task body with
rendered markdown.

| Key             | Action                                    |
|-----------------|-------------------------------------------|
| `j`/`↓`         | Scroll down                               |
| `k`/`↑`         | Scroll up                                 |
| `]`/`[`         | Scroll down/up by 3 lines                 |
| `J`/`K` or `d`/`u` | Half-page down/up                     |
| `Ctrl+j`/`k`   | Full page down/up                         |
| `g`/`Home`      | Jump to top                               |
| `G`/`End`       | Jump to bottom                            |
| `)`/`}`         | Next heading (any level)                  |
| `(`/`{`         | Previous heading (any level)              |
| `'`/`"`         | Next/previous `##` heading               |
| `1`-`9`         | Jump to Nth heading                       |
| `/` or `Ctrl+f` | Find in document                          |
| `n`/`N`         | Next/previous find match                  |
| `m`             | Move task (status picker)                 |
| `v`             | Toggle text selection mode                |
| `y`/`Y`         | Yank content / yank file path             |
| `o`             | Open in `$EDITOR`                         |
| `z`/`Z`         | Fold deeper/shallower (headings)          |
| `<`/`>`         | Narrow/widen content width                |
| `:`             | Go to task by ID                          |
| `Esc`/`q`       | Back to board                             |

### Move dialog keybindings

Opened with `m` from the board or detail view.

| Key             | Action                                    |
|-----------------|-------------------------------------------|
| `j`/`k`/`↓`/`↑` | Navigate status list                    |
| `Enter`         | Move task to selected status              |
| `1`-`9`         | Move to status N directly                 |
| Letter          | Jump to status starting with that letter  |
| `/`             | Open filter (type to narrow status list)  |
| `Esc`/`q`       | Cancel                                    |

### Search keybindings

Opened with `/` or `f` from the board.

| Key             | Action                                    |
|-----------------|-------------------------------------------|
| Type            | Filter tasks live (title, body, tags)     |
| `Enter`         | Accept filter and return to board         |
| `Esc`           | Clear search and return to board          |
| `Ctrl+n`/`p`   | Jump to next/previous match on board      |
| `↑`/`↓`         | Browse search history                     |
| `Tab`           | Autocomplete from search history          |
| `Ctrl+w`        | Delete last word                          |
| `Ctrl+u`        | Clear entire query                        |
| `?` (empty)     | Show search syntax help                   |

### Create/edit wizard

Opened with `c` (create) or `e` (edit) from the board.

```
  ┌─────────┐    ┌──────┐    ┌──────────┐    ┌──────┐
  │ 1.Title │───►│2.Body│───►│3.Priority│───►│4.Tags│───► save
  └─────────┘    └──────┘    └──────────┘    └──────┘
```

| Key             | Action                                    |
|-----------------|-------------------------------------------|
| `Tab`           | Advance to next step                      |
| `Shift+Tab`    | Go back to previous step                  |
| `Enter`         | Submit (from Title, Priority, or Tags)    |
| `Ctrl+j`        | Submit from any step (including Body)     |
| `Esc`           | Cancel and return to board                |
| `←`/`→`         | Select priority (in Priority step)        |

### TUI workflow recipes

#### See only your active worktree work

Press `w` to toggle the worktree filter. The board shows only tasks
that have an active worktree, hiding everything else. Press `w` or
`Esc` to clear.

#### Focus on a specific task's context

Press `C`, select the branch for the task you're working on. The board
filters to that task plus its parent, siblings, dependencies, and
dependents. Press `Esc` to peel off the context filter.

#### Focus on worktree branches only

Press `W` (shift). Like `C` but only shows branches that have active
git worktrees — useful in multi-agent environments where several
worktrees may be active simultaneously.

#### Assign a branch to a task before starting work

Select the task, press `b`, then either pick an existing branch or type
a new name and press Enter to create it. The task's `branch` field is
set. If you opened with `W` instead, a git worktree is created too.

#### Quick status change

Press `m` to open the move dialog, then press the number key (1-7) for
the target column, or type the first letter of the status name to jump
to it. `Enter` confirms.

#### Triage with column soloing

Press `1` to solo-expand the backlog column. Review each task with
`Enter` (detail view) or `R` (reader panel). Press `m` to promote to
todo. Press `2` to solo the todo column next. Press `X` to restore all
columns.

#### Find a task fast

Press `:` and type the task ID number, then `Enter`. The TUI jumps
directly to that task, even expanding a collapsed column if needed.

Or press `/`, type a search term — the board filters live. Use
`Ctrl+n`/`Ctrl+p` to cycle through matches across columns.

---

## 7. Output Format Decision Tree

```
  What do you need?
        │
        ├── Structured data for scripting/jq?
        │       └── --json
        │
        ├── AI agent reading a task list?
        │       └── --compact (70% fewer tokens than JSON)
        │
        ├── AI agent reading a single task?
        │       └── show --prompt [--fields title,status,body]
        │
        ├── Human reading at a terminal?
        │       └── (default) table format
        │
        └── Embedding board state in docs?
                └── kbmdx context [--write-to FILE]
```

| Format     | Flag                    | Best for                     | Token cost |
|------------|-------------------------|------------------------------|------------|
| table      | (default)               | Human terminal reading       | Highest    |
| compact    | `--compact`/`--oneline` | Agent list/board consumption | ~30% of JSON |
| json       | `--json`                | Scripting, `jq`, piping      | High       |
| prompt     | `show --prompt`         | Single-task LLM context      | Lowest     |

---

## 8. Configuration Enforcement Rules

Per-status enforcement rules control what conditions must be met before a
task can enter or remain in a status.

### Default status rules

| Status        | require_claim | require_branch | show_duration | Terminal |
|---------------|:---:|:---:|:---:|:---:|
| backlog       |     |     | ✓   |     |
| todo          |     |     | ✓   |     |
| in-progress   | ✓   |     | ✓   |     |
| references    |     |     |     |     |
| review        | ✓   |     | ✓   |     |
| done          |     |     | ✓   | ✓   |
| archived      |     |     |     | ✓   |

### Enforcement interaction with `--force`

| Check              | Enforced by           | `--force` bypasses? |
|--------------------|-----------------------|:---:|
| require_claim      | `move`, `edit`        |     |
| require_branch     | `move`, `edit`        | ✓   |
| WIP limit (column) | `move`                | ✓   |
| WIP limit (class)  | `move`                | ✓   |
| Dependency check   | `pick`                |     |
| Blocked flag       | `pick`                |     |

### Customizing rules

In `config.yml`, statuses can be strings or mappings:

```yaml
statuses:
  - backlog
  - todo
  - name: in-progress
    require_claim: true
    require_branch: true    # enable branch enforcement
    show_duration: true
  - name: review
    require_claim: true
  - done
```

---

## 9. Class of Service & Priority Sorting

The `pick` command uses a two-level sort to determine which task to
assign next.

### Class priority (level 1)

Classes are sorted by their position in the config list. Lower index =
picked first.

```
  ┌──────────────────────────────────────────────────────┐
  │  Class sort order (default config)                    │
  │                                                       │
  │  0: expedite    ← picked first (WIP limit: 1,        │
  │                    bypasses column WIP)                │
  │  1: fixed-date  ← sorted by due date within class    │
  │  2: standard    ← default class for all tasks        │
  │  3: intangible  ← picked last                        │
  └──────────────────────────────────────────────────────┘
```

### Task priority (level 2)

Within the same class, tasks are sorted by priority index (higher =
picked first), then by task ID (lower = older = picked first).

```
  Priorities (default):
    0: low         ← picked last
    1: medium
    2: high
    3: critical    ← picked first
```

### Combined sort example

```
  Candidates:
    #10  standard/high
    #12  expedite/medium
    #8   standard/critical
    #15  fixed-date/high    (due: Mar 20)
    #14  fixed-date/medium  (due: Mar 15)

  Pick order:
    1. #12  expedite/medium     ← expedite class wins (index 0)
    2. #14  fixed-date/medium   ← fixed-date class (index 1), earlier due
    3. #15  fixed-date/high     ← fixed-date class (index 1), later due
    4. #8   standard/critical   ← standard class (index 2), higher priority
    5. #10  standard/high       ← standard class (index 2), lower priority
```

### Class WIP limits

| Class      | Default WIP | Bypass column WIP? |
|------------|:-----------:|:------------------:|
| expedite   | 1           | ✓                  |
| fixed-date | 0 (none)    |                    |
| standard   | 0 (none)    |                    |
| intangible | 0 (none)    |                    |

---

## 10. Dependency & Blocking

Two independent mechanisms can prevent work on a task.

### Manual blocking

Set by a human or agent when an external condition prevents work.

```bash
kbmdx edit <ID> --block "Waiting on API keys"   # block
kbmdx edit <ID> --unblock                        # unblock
```

### Dependency blocking

Automatic: a task is considered blocked if any task in its `depends_on`
list is not at a terminal status (done or archived).

```bash
kbmdx edit <ID> --add-dep <DEP_ID>    # add dependency
kbmdx edit <ID> --remove-dep <DEP_ID> # remove dependency
kbmdx deps <ID> --transitive          # show full dependency chain
```

### How pick handles both

```
  All tasks
    │
    ├── filter: unclaimed
    ├── filter: matching status/tags
    ├── filter: blocked == false      ← manual blocking
    ├── filter: all deps at terminal  ← dependency blocking
    ├── sort: class → priority → ID
    │
    └── return first candidate
```

Both can coexist on the same task. A task that is manually blocked *and*
has unmet dependencies needs both to be resolved before it becomes
pickable.

---

## Quick Reference: Common Workflows

### Start of session

```bash
kbmdx agent-name                                  # get identity
kbmdx board --compact                             # orient
kbmdx pick --claim <agent> --status todo \
  --move in-progress                              # claim next task
kbmdx show <ID>                                   # read details
```

### Working on a task

```bash
git worktree add ../kbmdx-task-<ID> -b task/<ID>-desc
kbmdx edit <ID> --branch "task/<ID>-desc" \
  --worktree "../kbmdx-task-<ID>" --claim <agent>
cd ../kbmdx-task-<ID>
# ... implement, test, commit ...
```

### Finishing a task

```bash
cd <board-home>
git switch main && git merge task/<ID>-desc
kbmdx edit <ID> --release --clear-branch --clear-worktree
kbmdx move <ID> done
git worktree remove --force ../kbmdx-task-<ID>
git branch -d task/<ID>-desc
```

### Parking a blocked task

```bash
kbmdx handoff <ID> --claim <agent> \
  --block "Waiting on: ..." \
  --note "Done: X. Remaining: Y. Branch: task/<ID>-desc" \
  -t --release
# then: pick next task
```

### Situational awareness

```bash
kbmdx worktrees                              # active worktrees
kbmdx worktrees --check                      # stale/orphan detection
kbmdx list --compact --context               # tasks for current branch
kbmdx list --compact --claimed-by <agent>    # your claimed tasks
kbmdx list --compact --blocked               # all blocked work
kbmdx list --compact --status review         # all parked work
kbmdx branch-check                           # validate branch setup
```
