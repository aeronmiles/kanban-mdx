# Navigation & Views

## Board navigation

| Key | Action |
|-----|--------|
| `h`/`l`/arrows | Move between columns |
| `j`/`k`/arrows | Move between tasks |
| `g`/`Home` | Top of column |
| `G`/`End` | Bottom of column |
| `J`/`K` | Half-page down/up |
| `Ctrl+J`/`Ctrl+K` | Full page down/up |
| `PgUp`/`PgDn` | Page scroll |
| `:`/`Ctrl+G` | Jump to task by ID |

## Column management

| Key | Action |
|-----|--------|
| `x` | Collapse/expand current column |
| `X` | Expand all columns |
| `1`-`9` | Solo-expand column N |
| `Shift+1`-`9` | Toggle-collapse column N |

### Triage workflow

Press `1` to solo the backlog. Review each task, press `m` to promote.
Press `2` to solo todo next. Press `X` to restore all columns.

## Display modes

| Key | Action |
|-----|--------|
| `V` | Toggle cards/list view |
| `s`/`S` | Cycle sort mode forward/reverse |
| `a` | Toggle age display (created/updated) |
| `t` | Cycle color theme |
| `T` | Reset theme adjustments |
| `,`/`.` | Brightness down/up |
| `Alt+,`/`Alt+.` | Saturation down/up |

Sort modes cycle through: priority, newest, oldest, created-new,
created-old.

## Reader panel

| Key | Action |
|-----|--------|
| `R` | Toggle reader panel |
| `<`/`>` | Narrow/widen panel |
| `z`/`Z` | Fold deeper/shallower (headings) |
| `'`/`"` | Next/previous `##` heading |

## Detail view

Press `Enter` to open a task's full detail view. Keybindings mirror
a document reader:

| Key | Action |
|-----|--------|
| `j`/`k` | Scroll up/down |
| `J`/`K` or `d`/`u` | Half-page down/up |
| `g`/`G` | Jump to top/bottom |
| `}`/`{` | Next/prev heading |
| `1`-`9` | Jump to Nth heading |
| `/` | Find in text |
| `n`/`N` | Next/prev find match |
| `z`/`Z` | Fold deeper/shallower |
| `<`/`>` | Narrow/widen content width |
| `m` | Move task |
| `y`/`Y` | Copy content/path |
| `o` | Open in `$EDITOR` |
| `Esc`/`q` | Back to board |

## View state machine

```
                ┌─────────────────┐
        ┌──────│     Board       │──────┐
        │      │  (default view) │      │
        │      └──┬──┬──┬──┬──┬──┘      │
        │         │  │  │  │  │         │
 Enter  │    /,f  │  m  │  d  │  c      │  ?,q
        │         │     │     │         │
        v         v     v     v         v
 ┌──────────┐ ┌──────┐ ┌──────────┐ ┌────────┐ ┌──────┐
 │  Detail  │ │Search│ │ConfirmDel│ │CreateWiz│ │ Help │
 │  Esc <───│ │Esc <─│ │  y/n <──│ │ 4 steps │ │Esc <─│
 └──────────┘ └──────┘ └──────────┘ └─────────┘ └──────┘
```
