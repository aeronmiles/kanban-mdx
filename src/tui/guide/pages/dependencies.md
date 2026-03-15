# Dependencies & Blocking

Two independent mechanisms can prevent work on a task.

## Manual blocking

Set by a human or agent when an external condition prevents work.

**TUI**: Press `B` on any task. If unblocked, a reason prompt opens
(Enter to confirm, Esc to cancel — reason is optional). If already
blocked, `B` unblocks immediately. Works in both board and detail
views.

**CLI**:

```bash
kbmdx edit <ID> --block "Waiting on API keys"   # block
kbmdx edit <ID> --unblock                        # unblock
```

Blocked tasks show a red card border and "blocked" in detail view.
Use `@blocked` in the search bar (or `B` with no task selected)
to filter the board to blocked tasks only.

## Dependency blocking

Automatic: a task is considered blocked if any task in its
`depends_on` list is not at a terminal status (done or archived).

```bash
kbmdx edit <ID> --add-dep <DEP_ID>       # add dependency
kbmdx edit <ID> --remove-dep <DEP_ID>    # remove dependency
kbmdx deps <ID> --transitive             # full dependency chain
```

## How pick handles both

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

Both can coexist on the same task. A task that is manually blocked
*and* has unmet dependencies needs both resolved before it becomes
pickable.
