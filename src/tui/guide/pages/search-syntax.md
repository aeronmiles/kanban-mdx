# Search & Filter Syntax

Press `/` or `f` to open the search bar. The board filters live as
you type. Press `Enter` to accept, `Esc` to clear.

## Free text

Any text that is not a recognized filter token performs a substring
match against the task title, body, and tags.

## Time filters

| Syntax | Meaning |
|--------|---------|
| `@48h` `@3d` `@2w` `@1mo` `@today` | Within duration (follows age mode) |
| `@>2w` | Older than duration |
| `created:3d` | Explicit field: created within 3 days |
| `updated:>1w` | Explicit field: updated more than 1 week ago |

Duration units: `m` (minutes), `h` (hours), `d` (days), `w` (weeks),
`mo` (months). Examples: `30m`, `48h`, `3d`, `2w`, `1mo`.

## Priority filters

| Syntax | Meaning |
|--------|---------|
| `p:high` | Exact match |
| `p:medium+` | At or above medium |
| `p:high-` | At or below high |

Priority abbreviations: `c` = critical, `h` = high, `m` = medium,
`l` = low.

## ID filters

```
#5          single task
id:5        same as #5
id:1,3,7    multiple IDs
id:5-10     range
```

## Flag filters

| Syntax | Meaning |
|--------|---------|
| `@blocked` | Show only blocked tasks |

Press `B` from the board to toggle `@blocked` in the search query.

## Semantic search

Prefix your query with `~` to use embedding-based semantic search:

```
~error handling      semantic search
p:high ~performance  combine with filters
```

Semantic search requires embeddings to be configured and indexed.

## Combining filters

All filters AND together. Examples:

```
@24h p:h+ ~perf fix
@blocked p:h+
```

The first finds tasks updated in the last 24 hours, with high or
critical priority, semantically related to "perf fix". The second
shows blocked tasks with high or critical priority.

## Search history

- `Up`/`Down` arrows browse previous searches
- `Tab` autocompletes from search history
- `Ctrl+N`/`Ctrl+P` jump to next/previous match on the board
