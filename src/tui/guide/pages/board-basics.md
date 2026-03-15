# Board Basics

kbmdx is a terminal kanban board. Tasks live in markdown files under
`.kbmdx/tasks/` and flow through status columns left-to-right.

## Layout

```
 backlog    todo    in-progress    review    done    archived
 ┌──────┐ ┌──────┐ ┌───────────┐ ┌────────┐ ┌──────┐ ┌────────┐
 │ #10  │ │ #12  │ │ #15       │ │ #18    │ │ #20  │ │        │
 │ #11  │ │ #13  │ │ #16       │ │        │ │ #21  │ │        │
 │      │ │ #14  │ │           │ │        │ │      │ │        │
 └──────┘ └──────┘ └───────────┘ └────────┘ └──────┘ └────────┘
                    ▲ require_claim           terminal  terminal
```

Each column corresponds to a status from `config.toml`. Some statuses
enforce rules (claims, branches) before a task can enter them.

## Card vs List mode

Press `V` to toggle between **card mode** (visual cards with title,
priority, tags, and age) and **list mode** (compact one-line-per-task
table).

## Reader panel

Press `R` to open a side panel that shows the full markdown body of
the selected task. Use `<` / `>` to adjust panel width and `z` / `Z`
to fold/unfold headings.

## Getting around

- `h`/`l` or arrow keys move between columns
- `j`/`k` or arrow keys move between tasks within a column
- `Enter` opens the full detail view for the selected task
- `?` opens the quick-reference keybinding overlay
- `H` opens this guide
- `q` or `Esc` quits (clears any active filter first)

## Quick actions

| Key | Action |
|-----|--------|
| `c` | Create new task |
| `e` | Edit selected task |
| `d` | Delete task |
| `m` | Move task to another status |
| `+`/`-` | Raise/lower priority |
| `o` | Open task in `$EDITOR` |
| `y` | Copy task content to clipboard |
