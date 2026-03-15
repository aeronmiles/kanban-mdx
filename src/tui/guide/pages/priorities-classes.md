# Priorities & Classes of Service

## Priority levels

Tasks have a priority that determines their position in the board
and their pick order when agents claim work.

| Priority | Sort key | TUI shortcut |
|----------|:--------:|:------------:|
| critical | 0 (first) | `+` to raise |
| high | 1 | |
| medium | 2 | |
| low | 3 (last) | `-` to lower |

## Classes of service

Classes group tasks by urgency and scheduling policy. The `pick`
command uses class as the primary sort, then priority within a class.

### Class priority order

```
 0: expedite    ← picked first (WIP limit: 1, bypasses column WIP)
 1: fixed-date  ← sorted by due date within class
 2: standard    ← default class for all tasks
 3: intangible  ← picked last
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
   1. #12  expedite/medium     ← expedite class wins
   2. #14  fixed-date/medium   ← earlier due date
   3. #15  fixed-date/high     ← later due date
   4. #8   standard/critical   ← higher priority
   5. #10  standard/high       ← lower priority
```

### Class WIP limits

| Class | Default WIP | Bypass column WIP? |
|-------|:-----------:|:------------------:|
| expedite | 1 | yes |
| fixed-date | 0 (none) | |
| standard | 0 (none) | |
| intangible | 0 (none) | |
