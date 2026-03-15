# Task Lifecycle

Tasks flow through statuses left-to-right. Some statuses enforce
rules that must be satisfied before a task can enter them.

## Status flow

```
                     require_claim
                     ┌──────┴──────┐
 ┌─────────┐ ┌──────┐ ┌───────────┐ ┌────────┐ ┌──────┐ ┌──────────┐
 │ backlog │→│ todo │→│in-progress│→│ review │→│ done │→│ archived │
 └─────────┘ └──────┘ └───────────┘ └────────┘ └──────┘ └──────────┘
  initial                                        terminal  terminal
```

## Status rules (defaults)

| Status | require_claim | require_branch | Terminal |
|--------|:---:|:---:|:---:|
| backlog | | | |
| todo | | | |
| in-progress | yes | | |
| references | | | |
| review | yes | | |
| done | | | yes |
| archived | | | yes |

## Auto-timestamps

```
              ┌── started ──┐                  ┌── completed ──┐
 backlog → todo → in-progress → review → done
                ▲ first move from             ▲ move to any
                  initial status               terminal status
```

- **started**: set on the first move away from the initial status.
  Not reset on subsequent moves.
- **completed**: set on move to any terminal status (done, archived).
- **updated**: set on every mutation (move, edit, etc.).

## Enforcement & bypass

- **require_claim**: task must have a non-empty `claimed_by` to enter
  the status. Use `--claim <agent>` on the move command to satisfy.
- **require_branch**: task must have a `branch` field matching the
  current git branch. Not set by default.
- **WIP limits**: per-column limits configured in config. Move fails
  if the target column is at capacity.
- **--force**: bypasses `require_branch` and WIP limit enforcement.
  Does *not* bypass `require_claim`.

## Moving tasks in the TUI

Press `m` from board or detail view to open the move dialog:

| Key | Action |
|-----|--------|
| `j`/`k` | Navigate status list |
| `Enter` | Move to selected status |
| `1`-`9` | Move to status N directly |
| Letter | Jump to status starting with that letter |
| `/` | Filter the status list |
| `Esc` | Cancel |
