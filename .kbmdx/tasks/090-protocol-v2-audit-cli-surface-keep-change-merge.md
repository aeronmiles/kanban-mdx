---
id: 90
title: 'Protocol v2 audit: CLI surface keep/change/merge/add/remove'
status: references
priority: high
created: '2026-03-15T09:45:00.190955Z'
updated: '2026-03-15T09:45:00.190955Z'
tags:
- design
- agents
- protocol
depends_on:
- 89
---

## Protocol v2 Audit: Full CLI Surface Review

Every command, flag, output format, environment variable, error code, and task model field evaluated against the protocol described in task #89. Each gets a verdict (**Keep** / **Change** / **Merge** / **Add** / **Remove**) with rationale.

---

## 1. Commands

### 1.1 `init` — Keep

**Current:** Creates `.kbmdx/` directory with `config.toml` and empty `tasks/` dir. Accepts board name, custom statuses, WIP limits.

**Protocol relevance:** Low. Board initialization is a one-time human operation. Adapters never call this.

**Verdict: Keep as-is.** No protocol changes needed. The only consideration is whether `init` should also scaffold `dispatch.toml` — but that's a dispatch-layer concern, not core.

---

### 1.2 `create` — Change

**Current:** Creates a task file with frontmatter. Supports `--claim` (with `$KANBAN_AGENT`), all task fields, `--body`, `--depends-on`, `--parent`.

**Protocol relevance:** High. Adapters create tasks programmatically. The dispatcher may create subtasks or break down work.

**What works:**
- `--claim` + `$KANBAN_AGENT` — adapter sets env var once, all commands inherit
- `--depends-on` / `--parent` — structural relationships at creation time
- `--body` — initial task description
- JSON output with created task — adapter gets the ID immediately

**What's missing:**
- `--requires` — capability tags for routing (`requires: [rust, tui]`). Adapters need this to match tasks to agent capabilities.
- `--set-section` / `--section-body` — only `edit` has this, but it's useful at creation time too (e.g., `## Acceptance Criteria`, `## Agent Instructions`)

**Verdict: Change.**
- Add `--requires <CAPS>` (comma-separated capability tags)
- Add `--set-section <NAME>` + `--section-body <TEXT>` (reuse edit's logic)

---

### 1.3 `show` — Change (major)

**Current:** Displays a single task. Supports `--json`, `--no-body`, `--section`, `--prompt`, `--fields`, `--children`.

**Protocol relevance:** Critical. This is how adapters read task details. Currently requires multiple calls to get full context.

**What works:**
- `--json` — structured output for parsing
- `--section <NAME>` — reads named `##` sections (key for `agent-status` convention)
- `--children` — hierarchy awareness
- `--no-body` — lightweight queries

**What's wrong:**
- An adapter starting a task needs: the task + parent context + dependency states + recent activity + sibling tasks. That's 4-5 separate commands today.

**What's missing:**
- `--hydrate` — bundles task + parent + deps (upstream/downstream) + activity log + siblings into one response

**`--prompt` debate:**
- `--prompt` outputs `key: value` pairs — a fourth format that only `show` supports
- It's useful for LLM token efficiency but isn't JSON-parseable
- The compact format already serves the "short output" need
- However, `--prompt --fields` is genuinely useful for selective field extraction that no other format supports

**Verdict: Change.**
- Add `--hydrate` flag — returns enriched JSON envelope (see task #89 for schema)
- Keep `--prompt` — it serves a real niche (LLM context injection) that compact doesn't fully cover
- `--hydrate` and `--prompt` are mutually exclusive (hydrate is always JSON)

---

### 1.4 `edit` — Keep

**Current:** The kitchen-sink mutation command. 30+ flags covering every field, sections, claims, blocking, dependencies, timestamps.

**Protocol relevance:** Critical. This is the adapter's primary write command.

**What works — everything:**
- `--set-section` / `--section-body` — the protocol's structured progress mechanism
- `--append-body` / `--timestamp` — narrative progress notes
- `--claim` / `--release` — lease management
- `--block` / `--unblock` — blocking workflow
- `--depend` / `--undepend` — runtime dependency management
- Bulk edit with comma-separated IDs
- `$KANBAN_AGENT` env var for implicit claim
- `--force` for enforcement bypass

**Debate: Is edit too overloaded?**
The flag count is high (30+), but every flag maps to a single field mutation. There's no ambiguity — `--priority high` sets priority to high. The alternative (separate commands per field) would explode the command count with no real benefit. The section mechanism is particularly elegant — it gives adapters structured data in the body without new frontmatter fields.

**Verdict: Keep as-is.** This command is already protocol-optimal. The only addition is `--requires` to set capability tags (pairs with `create --requires`).

---

### 1.5 `delete` — Keep

**Current:** Deletes task files. Requires `--yes` for non-interactive. Supports bulk IDs.

**Protocol relevance:** Low. Adapters rarely delete — they archive or move to done.

**Verdict: Keep as-is.** `--yes` is correctly required. Batch result reporting is clean.

---

### 1.6 `move` — Keep

**Current:** Changes task status. Supports `--next`/`--prev` (relative), explicit status, `--claim` during move, `--force`.

**Protocol relevance:** High. Status transitions are the core workflow.

**What works:**
- Explicit status target — unambiguous for adapters
- `--claim` during move — atomic claim+transition
- Auto-set timestamps (started on first move out of initial, completed on terminal)
- Enforcement checks (WIP, claim requirement, branch)

**Debate: `--next`/`--prev` for adapters?**
These are convenient for humans but risky for adapters — they depend on position in the status list and fail at boundaries. Adapters should always use explicit status names. However, removing them would break human workflows. They don't hurt the protocol — adapters just don't use them.

**Verdict: Keep as-is.** Explicit status is the protocol path. `--next`/`--prev` stay for human convenience.

---

### 1.7 `list` — Change

**Current:** The query engine. 20+ filter flags, sort, group-by, limit.

**Protocol relevance:** Critical. Adapters use this to discover work, monitor the board, and build dashboards.

**What works:**
- Filter composition (all AND logic) — predictable for adapters
- `--unclaimed` — find available work
- `--blocked` / `--not-blocked` / `--unblocked` — dependency-aware filtering
- `--claimed <AGENT>` — find "my" tasks
- `--context` / `-C` — worktree-scoped views
- `--group-by` — dashboard views
- `--status` / `--priority` / `--tag` / `--class` — standard filters
- `--search` — substring matching
- `--sort` / `--reverse` / `--limit` — result shaping
- `--no-body` — lightweight queries

**What's missing:**
- `--requires <CAPS>` — filter tasks by capability requirements (matches agent's capabilities)
- `--stale-claims` — tasks with expired claims (currently only detected during `pick`). Alternative: put this in `health` command instead.
- `--claimed-by <AGENT>` vs `--claimed <AGENT>` — these are the same flag. Current name `--claimed` is fine.

**Debate: `--stale-claims` here or in `health`?**
`health` is a better home. `list` filters on task properties; claim staleness is a time-dependent runtime check, not a task property. Mixing temporal checks into `list` muddies the filter semantics.

**Verdict: Change.**
- Add `--requires <CAPS>` filter (comma-separated, AND logic: task must require all specified caps, or subset)
- Stale claims go in `health`, not here

---

### 1.8 `find` — Keep

**Current:** Semantic search across task titles and bodies. Returns scored results with chunk/header/line info.

**Protocol relevance:** Medium. Useful for agents understanding related work or finding prior art. Not part of the dispatch loop.

**What works:**
- Natural language query
- Scored results with metadata (chunk, header, line, score)
- `--limit` for result capping

**Verdict: Keep as-is.** Semantic search is a value-add, not a protocol primitive.

---

### 1.9 `pick` — Change

**Current:** Atomic highest-priority unclaimed task selection. Supports `--claim`, `--status`, `--move`, `--tags`, `--no-body`.

**Protocol relevance:** Critical. This is THE dispatch primitive. The atomic claim prevents race conditions between concurrent agents.

**What works:**
- Atomic selection + claim + move in one call
- Priority-based ordering (highest first, then oldest)
- `--status` scoping (pick from specific columns)
- `--tags` filtering (routing by tag)
- Auto-populates branch/worktree from git context
- `NOTHING_TO_PICK` error code — clean signal for "no work available"

**What's missing:**
- `--requires <CAPS>` — pick only tasks whose `requires` field matches the agent's capabilities. This is the capability routing mechanism.

**Debate: Should pick support `--exclude-tags`?**
Not needed. Tags are opt-in routing. If a task has tags an agent can't handle, the `requires` field is the proper mechanism. Tags are for human categorization; `requires` is for machine routing. Don't overload tags.

**Verdict: Change.**
- Add `--requires <CAPS>` filter (agent declares what it can do; pick matches against task requirements)

---

### 1.10 `archive` — Keep

**Current:** Sets status to "archived". Bulk IDs.

**Protocol relevance:** Low. Post-completion cleanup. Adapters might archive done tasks periodically.

**Verdict: Keep as-is.**

---

### 1.11 `handoff` — Change

**Current:** Moves to `review`, appends note, optionally blocks and releases claim. Supports `--claim`, `--note`, `--timestamp`, `--block`, `--release`.

**Protocol relevance:** High. This is the agent-to-agent and agent-to-user transition command.

**What works:**
- Combined move + note + block + release in one atomic call
- `--timestamp` for timestamped handoff notes
- `--block` with reason for structured blocking

**What's missing:**
- `--set-section` / `--section-body` — `edit` has this but `handoff` doesn't. An agent completing a handoff should be able to write a structured `## Outcome` section in the same call, rather than doing `edit --set-section` followed by `handoff`.

**Debate: Should handoff support `--move <STATUS>` instead of hardcoding review?**
Currently, handoff always targets `review`. This is intentional — review IS the handoff status. But what if a board uses a different status name for the waiting state (e.g., `blocked`, `waiting`, `needs-review`)? The counter-argument is that the skill prescribes `review` as the convention, and custom status names should follow the convention. Hardcoding `review` is a feature, not a limitation — it makes the protocol predictable.

However, there's a subtlety: `handoff` should verify `review` exists in the board's status list and fail with a clear error if not, rather than silently creating an invalid status.

**Verdict: Change.**
- Add `--set-section <NAME>` + `--section-body <TEXT>` (reuse edit's logic)
- Keep hardcoded `review` target — it's a protocol convention
- Ensure clear error if `review` status doesn't exist

---

### 1.12 `deps` — Merge (into `show --hydrate`)

**Current:** Shows upstream/downstream dependencies for a task. Supports `--transitive`.

**Protocol relevance:** Medium. Adapters need dependency context, but as part of task hydration, not as a standalone query.

**What works:**
- Direction control (upstream/downstream)
- Transitive traversal
- Clean JSON output with task summaries

**Debate: Standalone command vs. folded into `show --hydrate`?**
An adapter almost never calls `deps` in isolation. It calls it as part of understanding a task before starting work — which is exactly what `--hydrate` does. The standalone command is still useful for human debugging, but for the protocol, `show --hydrate` subsumes it.

**Verdict: Merge.** Keep `deps` as a standalone command for human use. But the protocol path is `show --hydrate`, which includes dependency data. No changes to `deps` itself — it's already correct. The merge is conceptual: adapters use `--hydrate` instead of `deps`.

---

### 1.13 `board` — Change

**Current:** Board summary with status columns, task counts, WIP limits. Supports `--watch`, `--group-by`, `--parent`.

**Protocol relevance:** Medium. Adapters use it for orientation (board shape, status names, WIP limits), not for ongoing operations.

**What works:**
- Status column summary with WIP utilization
- `--group-by` for multi-dimensional views
- `--parent` for hierarchy scoping
- `--watch` for live refresh

**Debate: `--watch` and `kbmdx watch`**
`board --watch` re-renders the board on file changes — it's TUI-like behavior in the CLI. The proposed `kbmdx watch` (task #89) is different: it emits JSONL events. These are complementary, not redundant. `board --watch` is for humans; `kbmdx watch` is for adapters.

**Verdict: Change.** No changes to `board` itself. But note that `board --compact` is the canonical way for an adapter to discover the board's status names, priorities, WIP limits, and overall shape. Document this as a protocol convention.

---

### 1.14 `metrics` — Change

**Current:** Flow metrics: throughput, lead/cycle time, flow efficiency, aging items. Supports `--since`, `--parent`.

**Protocol relevance:** Medium. Useful for dispatcher health dashboards and auto-scaling decisions.

**What's missing:**
- Per-agent metrics derived from activity log correlation (tasks completed, avg cycle time, current task, session duration)
- This is the data that lets a dispatcher answer "which agent type is most effective?"

**Verdict: Change.**
- Add `--agent` flag to scope metrics to a specific agent's claims
- Add per-agent breakdown in the default output when multiple agents have activity

---

### 1.15 `log` — Change

**Current:** Activity log viewer. Filters: `--since`, `--limit`, `--action`, `--task`.

**Protocol relevance:** High. The activity log IS the event history. Adapters query it for session correlation, progress tracking, and audit.

**What works:**
- Action type filter (`create`, `move`, `edit`, `claim`, `release`, etc.)
- Task ID filter — scope to single task's history
- Time-based filter with `--since`
- Structured JSON output

**What's missing:**
- `--claimed-by <AGENT>` — filter log entries related to a specific agent's claims. This is the session correlation mechanism.
- `--watch` / streaming mode — tail the log as events happen (alternative to `kbmdx watch`)

**Debate: `--watch` on log vs. separate `watch` command?**
`log --watch` would tail `activity.jsonl` and emit new entries as JSONL — simple, low-effort. `kbmdx watch` (task #89) diffs task file state and emits semantic events (richer but harder to implement). These serve different needs: `log --watch` is raw events, `kbmdx watch` is state-change events. Both are useful.

However, implementing both is redundant for v1. `log --watch` is simpler and might be sufficient. The adapter can derive state changes from the action+detail fields in log entries (e.g., `action: "move"`, `detail: "todo -> in-progress"`).

**Verdict: Change.**
- Add `--claimed-by <AGENT>` filter
- Consider `--follow` / `--watch` as a simpler alternative to a full `watch` command (debate in New Commands section below)

---

### 1.16 `undo` / `redo` — Keep

**Current:** Reverses last mutation using file snapshots. `--dry-run` for preview.

**Protocol relevance:** Low-Medium. Agents rarely undo — they move forward. But useful as a safety net when an agent makes a mistake and the dispatcher catches it.

**Verdict: Keep as-is.**

---

### 1.17 `context` — Change

**Current:** Generates markdown board summary for embedding in `CLAUDE.md` or `AGENTS.md`. Supports `--write-to`, `--sections`, `--days`.

**Protocol relevance:** High. This is how agents get board-level context at session start.

**What works:**
- Selective sections (in-progress, blocked, overdue, recently-completed)
- `--write-to` for file generation (auto-updates delimited blocks)
- Lookback period for recently-completed

**What's missing:**
- Agent-specific context — "what did agent X last work on?" for session resumption
- Capability-filtered context — "what tasks match capabilities [rust, tui]?"
- This command generates context for a human reading `AGENTS.md`. The protocol also needs context that gets injected directly into an agent's prompt — tighter, more focused.

**Debate: Expand `context` or create a new command?**
`context` is already the "generate context for agents" command. Extending it with `--agent <NAME>` and `--requires <CAPS>` flags is more natural than creating a new command. The output stays markdown — that's the right format for injecting into agent prompts.

**Verdict: Change.**
- Add `--agent <NAME>` — includes agent's recent activity and current claims
- Add `--requires <CAPS>` — filters sections to tasks matching capabilities
- These are additive — existing usage unchanged

---

### 1.18 `config` — Keep

**Current:** Get/set config values via dot-separated paths.

**Protocol relevance:** Low. Adapters read config for board shape (status names, claim timeout) but rarely write it.

**Verdict: Keep as-is.** The dispatcher reads `claim_timeout` to set renewal intervals. That's the only protocol-relevant config query.

---

### 1.19 `worktrees` — Merge (into `health`)

**Current:** Lists git worktrees. `--check` detects stale metadata and orphan worktrees.

**Protocol relevance:** Medium. The `--check` output is exactly the kind of health data the dispatcher needs.

**Debate: Standalone vs. merged into `health`?**
`worktrees` without `--check` is just `git worktree list` with task cross-referencing. It's useful but niche. `worktrees --check` is health monitoring — it belongs with stale claim detection and blocked task counts. Merging `--check` output into `health` gives adapters one command for all health checks.

**Verdict: Merge.** Keep `worktrees` as a standalone command for human use (listing worktrees is useful). But fold `--check` results into the new `health` command's JSON output. `health` becomes the one-stop health endpoint.

---

### 1.20 `import` — Keep

**Current:** Bulk-creates tasks from JSON/YAML spec with parent/dependency resolution.

**Protocol relevance:** Medium. A dispatcher might use this to break an epic into subtasks programmatically.

**Verdict: Keep as-is.** The JSON input schema is well-defined. Ref-based dependency resolution handles the forward-reference problem cleanly.

---

### 1.21 `agent-name` — Keep

**Current:** Generates random two-word identifier (e.g., `frost-maple`).

**Protocol relevance:** High. Every agent session starts with this.

**Verdict: Keep as-is.** Simple, does one thing.

---

### 1.22 `skill` — Keep

**Current:** Install/check/update/show embedded agent skills. Supports `--agent`, `--skill`, `--global`.

**Protocol relevance:** Medium. The dispatcher uses `skill install` to set up agent environments before spawning.

**Verdict: Keep as-is.**

---

### 1.23 `embed` — Keep

**Current:** Manages semantic search embeddings. `sync`, `status`, `clear`.

**Protocol relevance:** Low. Infrastructure maintenance, not part of the dispatch loop.

**Verdict: Keep as-is.**

---

### 1.24 `read` — Keep

**Current:** Opens any markdown file in the TUI reader. No board required.

**Protocol relevance:** None. Human-facing feature.

**Verdict: Keep as-is.**

---

### 1.25 `completion` — Keep

**Current:** Generates shell completions for bash/zsh/fish/powershell.

**Protocol relevance:** None. Developer convenience.

**Verdict: Keep as-is.**

---

### 1.26 `filepath` — Keep

**Current:** Prints absolute path to a task's markdown file.

**Protocol relevance:** Low. An adapter could use this to read/write task files directly, bypassing the CLI. Useful but niche.

**Verdict: Keep as-is.**

---

### 1.27 `branch-check` — Merge (into `health`)

**Current:** Validates branch setup against task requirements.

**Protocol relevance:** Low. Branch enforcement is checked during `move`. This command is redundant pre-check.

**Verdict: Merge into `health`.** Include branch validation in the health report rather than as a standalone command. Keep the standalone command for backward compatibility.

---

### 1.28 `migrate-config` — Keep

**Current:** YAML-to-TOML config migration.

**Protocol relevance:** None. Legacy migration.

**Verdict: Keep as-is.** Will eventually be removed when YAML support is fully dropped.

---

### 1.29 `tui` — Keep

**Current:** Launches the terminal UI.

**Protocol relevance:** None directly. But the TUI's file watcher infrastructure (`notify::RecommendedWatcher`) is shared with `watch`.

**Verdict: Keep as-is.**

---

### 1.30 `gitignore` — Keep

**Current:** Manages `.gitignore` entries for kbmdx files.

**Protocol relevance:** None.

**Verdict: Keep as-is.**

---

## 2. New Commands

### 2.1 `watch` — Add

**Purpose:** Stream board events as JSONL to stdout. The single highest-value protocol addition.

**Debate: `watch` vs. `log --follow`?**

| | `watch` (state-diff) | `log --follow` (event-tail) |
|---|---|---|
| Implementation | Diff old vs. new task state on file change | Tail `activity.jsonl` for new lines |
| Output | Semantic events (`{"event": "move", "from": "todo", "to": "in-progress"}`) | Raw log entries (`{"action": "move", "detail": "todo -> in-progress"}`) |
| Claim expiry detection | Yes — can detect expired claims by scanning state | No — expiry isn't a logged event |
| New task fields | Yes — can detect any field change | Only logged mutations |
| Effort | Medium (~200 loc, reuses TUI file watcher) | Low (~50 loc, tail + parse) |
| Reliability | Atomic — reads full state, computes diff | Sequential — depends on log ordering |

**Resolution:** Both are useful for different consumers. `log --follow` is easier and covers 80% of adapter needs. `watch` is richer but more work. Implement `log --follow` first as the MVP protocol event stream. Add `watch` later if state-diff events prove necessary.

**Verdict: Add `log --follow` first (low effort), then `watch` as a follow-up.**

**`log --follow` spec:**
```bash
kbmdx log --follow [--action ACTION] [--claimed-by AGENT] [--json]
```
Tails `activity.jsonl`, emits new entries as they appear. Applies filters. Blocks until Ctrl+C.

---

### 2.2 `health` — Add

**Purpose:** One-stop health check for adapters. Consolidates checks scattered across `worktrees --check`, `list --blocked`, and implicit claim timeout logic.

**Spec:**
```bash
kbmdx health [--json]
```

**Output:**
```json
{
  "stale_claims": [
    {"id": 42, "claimed_by": "frost-maple", "claimed_at": "...", "expired_ago": "23m"}
  ],
  "orphan_worktrees": [
    {"path": "../kbmdx-task-38", "branch": "task/38-old-feature"}
  ],
  "stale_task_worktrees": [
    {"id": 38, "worktree": "../kbmdx-task-38", "exists": false}
  ],
  "blocked_count": 3,
  "overdue_count": 1,
  "wip_violations": [
    {"status": "in-progress", "count": 6, "limit": 5}
  ]
}
```

**Sources:**
- Stale claims: scan all tasks, check `claimed_at` + `claim_timeout`
- Orphan worktrees: from `worktrees --check` logic
- Stale task worktrees: tasks pointing to non-existent paths
- Blocked/overdue: from `list` filter logic
- WIP violations: from config WIP limits vs. current counts

**Verdict: Add.** Low effort (~100-150 loc), high value for adapters. Replaces multiple polling calls.

---

## 3. Global Flags

### 3.1 `--dir` / `-d` — Keep

Override kanban directory. Env: `KANBAN_DIR`.

**Protocol relevance:** Essential when the dispatcher manages multiple boards or runs from a different directory.

---

### 3.2 `--json` — Keep

JSON output format.

**Protocol relevance:** The primary adapter format. Every command should support it.

---

### 3.3 `--compact` — Keep

One-line output format.

**Protocol relevance:** Useful for LLM token efficiency and human scanning. Not the adapter's primary format (JSON is), but valuable for agent prompts.

---

### 3.4 `--table` — Keep

Human-readable table format (default).

**Protocol relevance:** None for adapters. Default for human use.

---

### 3.5 `--no-color` — Keep

Disable ANSI colors. Env: `NO_COLOR`.

**Protocol relevance:** Adapters should set `NO_COLOR=1` to avoid ANSI escape codes in non-JSON output.

---

## 4. Environment Variables

### 4.1 `KANBAN_DIR` — Keep

Board directory override.

---

### 4.2 `KANBAN_AGENT` — Keep (critical)

Default agent name for `--claim`. This is the protocol's agent identity mechanism. The dispatcher sets this once per spawned agent process and all kbmdx calls within that process inherit it.

**Debate: Is env var the right identity mechanism?**
Yes. It's process-scoped (each agent subprocess gets its own), doesn't require config files, and works with all commands that accept `--claim`. The alternative (a session file) adds statefulness that's unnecessary.

---

### 4.3 `NO_COLOR` — Keep

Standard convention. Adapters set this to avoid parsing ANSI codes.

---

### 4.4 `KANBAN_OUTPUT` — Keep

Default output format. Adapters set `KANBAN_OUTPUT=json` to avoid per-command `--json` flags.

---

## 5. Output Formats

### 5.1 JSON Format — Change

**Current:** `serde_json::to_string_pretty()` with skip_serializing_if for empty/null/false fields. Single objects for individual items, arrays for lists.

**Protocol relevance:** The primary adapter format.

**What works:**
- Pretty-printed (readable for debugging)
- Skip empty fields (reduced noise)
- Consistent envelope patterns (object vs. array)

**What's broken: Error output in JSON mode.**
Currently, errors go to stderr as plain text even with `--json`. Adapters parsing JSON get nothing on stdout and a human-readable string on stderr.

**Required fix:**
When `--json` is active, errors must be serialized as JSON to stderr:
```json
{"error": "task not found", "code": "TASK_NOT_FOUND", "details": {"id": 42}}
```

This is the single most important protocol fix. Without structured error output, adapters can't reliably distinguish error types.

**Verdict: Change.** Fix JSON error output. The rest of the JSON contract is solid.

---

### 5.2 Compact Format — Keep

**Current:** One-line-per-record format optimized for token efficiency.

**Protocol relevance:** Secondary. Useful for injecting into agent prompts (lower token cost than JSON), but not machine-parseable in general.

**Verdict: Keep as-is.** The format is well-defined and stable.

---

### 5.3 Table Format — Keep

**Current:** ANSI-colored, aligned columns via `comfy-table`.

**Protocol relevance:** None for adapters. Human default.

**Verdict: Keep as-is.**

---

### 5.4 Prompt Format (`--prompt`) — Keep

**Current:** `key: value` pairs, only on `show` command. `--fields` for selective output.

**Protocol relevance:** Niche but useful. When an adapter injects task context into an agent's prompt, `--prompt --fields id,title,status,body` is more token-efficient than JSON and more structured than compact.

**Debate: Promote to global format or keep on `show` only?**
Keep on `show` only. It doesn't make sense for `list` (which returns multiple tasks) or `board` (which is inherently structured). It's a single-task context injection format.

**Verdict: Keep as-is.** Don't generalize.

---

## 6. Error Contract

### 6.1 Error Codes (22 codes) — Keep

All 22 error codes are machine-readable and stable:

**Protocol-critical codes:**
- `TASK_CLAIMED` — another agent holds the claim (adapter should skip, not retry)
- `NOTHING_TO_PICK` — no available work (adapter should wait or stop)
- `CLAIM_REQUIRED` — status requires claiming (adapter forgot to claim)
- `WIP_LIMIT_EXCEEDED` / `CLASS_WIP_EXCEEDED` — capacity constraint (adapter should wait)
- `TASK_NOT_FOUND` — task was deleted or ID is wrong
- `BOUNDARY_ERROR` — at start/end of status list (adapter used `--next`/`--prev` incorrectly)

**Adapter behavior per code:**
| Code | Adapter action |
|------|---------------|
| `NOTHING_TO_PICK` | Wait, then retry (or stop if board is empty) |
| `TASK_CLAIMED` | Skip task, try different one |
| `WIP_LIMIT_EXCEEDED` | Wait for capacity |
| `CLAIM_REQUIRED` | Re-issue with `--claim` |
| `TASK_NOT_FOUND` | Log warning, skip |
| `NO_CHANGES` | Ignore (idempotent success) |
| `INTERNAL_ERROR` | Log error, alert operator |

**Verdict: Keep all codes.** They're well-designed for adapter consumption.

---

### 6.2 Exit Codes — Keep

- `0` — success
- `1` — user error (recoverable, adapter should handle)
- `2` — internal error (non-recoverable, alert operator)

**Verdict: Keep.** Simple, standard.

---

### 6.3 SilentError (batch operations) — Keep

Batch commands (delete, bulk edit) write results to stdout and errors to stderr independently. `SilentError` signals the exit code without duplicate output.

**Protocol relevance:** Adapters parsing batch results should check both stdout (successes) and stderr (failures), plus exit code for overall status.

**Verdict: Keep.** Clean partial-success pattern.

---

## 7. Task Model Fields

### 7.1 Core Identity — Keep all

| Field | Type | Verdict | Notes |
|-------|------|---------|-------|
| `id` | i32 | **Keep** | Immutable, auto-incremented |
| `title` | String | **Keep** | Required |
| `status` | String | **Keep** | Board-defined values |
| `priority` | String | **Keep** | Board-defined values |

---

### 7.2 Timestamps — Keep all

| Field | Type | Verdict | Notes |
|-------|------|---------|-------|
| `created` | DateTime | **Keep** | Immutable, set at creation |
| `updated` | DateTime | **Keep** | Auto-set on every mutation |
| `started` | Option\<DateTime\> | **Keep** | Auto-set on first move from initial status |
| `completed` | Option\<DateTime\> | **Keep** | Auto-set on move to terminal status |

Timestamps drive metrics (lead time, cycle time, flow efficiency). Critical for protocol observability.

---

### 7.3 Assignment & Claims — Keep all

| Field | Type | Verdict | Notes |
|-------|------|---------|-------|
| `assignee` | String | **Keep** | Human assignment (not the same as claim) |
| `claimed_by` | String | **Keep** | Agent lease holder |
| `claimed_at` | Option\<DateTime\> | **Keep** | Lease start time |

**Debate: `assignee` vs. `claimed_by` — are both needed?**
Yes. `assignee` is "who should work on this" (persistent, human-set). `claimed_by` is "who IS working on this right now" (ephemeral, agent-set, expires). An agent might be assigned to Alice but claimed by `frost-maple` (Alice's agent session). They serve different purposes.

---

### 7.4 Categorization — Keep all

| Field | Type | Verdict | Notes |
|-------|------|---------|-------|
| `tags` | Vec\<String\> | **Keep** | Freeform categorization |
| `class` | String | **Keep** | Class of service (expedite, standard, etc.) |
| `estimate` | String | **Keep** | Free-form time estimate |
| `due` | Option\<NaiveDate\> | **Keep** | Deadline |

---

### 7.5 Relationships — Keep all

| Field | Type | Verdict | Notes |
|-------|------|---------|-------|
| `parent` | Option\<i32\> | **Keep** | Epic/subtask hierarchy |
| `depends_on` | Vec\<i32\> | **Keep** | Dependency DAG |
| `blocked` | bool | **Keep** | Manual block flag |
| `block_reason` | String | **Keep** | Block context for handoffs |

---

### 7.6 Workspace — Keep all

| Field | Type | Verdict | Notes |
|-------|------|---------|-------|
| `branch` | String | **Keep** | Git branch name |
| `worktree` | String | **Keep** | Git worktree path |

---

### 7.7 Content — Keep all

| Field | Type | Verdict | Notes |
|-------|------|---------|-------|
| `body` | String | **Keep** | Markdown body (JSON-only, not in YAML frontmatter) |
| `file` | String | **Keep** | Absolute file path (JSON-only) |

The `body` field with `## Section` conventions is the protocol's extensibility mechanism. Agent status, outcomes, instructions, handoff notes — all live in named sections without adding fields.

---

### 7.8 New Field — Add

| Field | Type | Verdict | Notes |
|-------|------|---------|-------|
| `requires` | Vec\<String\> | **Add** | Capability tags for routing |

**Serialization:** `#[serde(default, skip_serializing_if = "is_empty_vec")]` — omitted if empty (consistent with `tags`, `depends_on`).

**Impact:** One new optional field. Backward compatible — existing tasks without `requires` are pickable by any agent. The dispatcher uses this to match agent capabilities to task requirements.

---

## 8. Summary Matrix

### Commands

| Command | Verdict | Protocol Change |
|---------|---------|-----------------|
| `init` | **Keep** | — |
| `create` | **Change** | Add `--requires`, `--set-section`/`--section-body` |
| `show` | **Change** | Add `--hydrate` |
| `edit` | **Keep** | Add `--requires` (minor) |
| `delete` | **Keep** | — |
| `move` | **Keep** | — |
| `list` | **Change** | Add `--requires` filter |
| `find` | **Keep** | — |
| `pick` | **Change** | Add `--requires` filter |
| `archive` | **Keep** | — |
| `handoff` | **Change** | Add `--set-section`/`--section-body` |
| `deps` | **Merge** | Protocol path is `show --hydrate`; command stays for humans |
| `board` | **Keep** | Document as protocol orientation command |
| `metrics` | **Change** | Add `--agent` filter, per-agent breakdown |
| `log` | **Change** | Add `--claimed-by`, `--follow` |
| `undo`/`redo` | **Keep** | — |
| `context` | **Change** | Add `--agent`, `--requires` |
| `config` | **Keep** | — |
| `worktrees` | **Merge** | `--check` folded into `health` |
| `import` | **Keep** | — |
| `agent-name` | **Keep** | — |
| `skill` | **Keep** | — |
| `embed` | **Keep** | — |
| `read` | **Keep** | — |
| `completion` | **Keep** | — |
| `filepath` | **Keep** | — |
| `branch-check` | **Merge** | Folded into `health` |
| `migrate-config` | **Keep** | — |
| `tui` | **Keep** | — |
| `gitignore` | **Keep** | — |
| `health` | **Add** | Stale claims, orphan worktrees, WIP violations, blocked/overdue |
| `log --follow` | **Add** | JSONL event streaming (simpler alternative to `watch`) |

### Output

| Surface | Verdict | Change |
|---------|---------|--------|
| JSON format | **Change** | Fix error output (structured JSON to stderr in `--json` mode) |
| Compact format | **Keep** | — |
| Table format | **Keep** | — |
| Prompt format | **Keep** | — |
| Error codes | **Keep** | All 22 codes stable |
| Exit codes | **Keep** | 0/1/2 unchanged |
| Batch results | **Keep** | — |

### Task Model

| Surface | Verdict | Change |
|---------|---------|--------|
| 30 existing fields | **Keep all** | No removals, no renames |
| `requires` field | **Add** | `Vec<String>`, skip if empty |

### Environment Variables

| Variable | Verdict | Change |
|----------|---------|--------|
| `KANBAN_DIR` | **Keep** | — |
| `KANBAN_AGENT` | **Keep** | — |
| `NO_COLOR` | **Keep** | — |
| `KANBAN_OUTPUT` | **Keep** | — |

---

## 9. Total Protocol v2 Delta

**New task field:** 1 (`requires`)
**New commands:** 2 (`health`, `log --follow`)
**Changed commands:** 7 (`create`, `show`, `list`, `pick`, `handoff`, `metrics`, `context`)
**Merged concepts:** 3 (`deps` → `show --hydrate`, `worktrees --check` → `health`, `branch-check` → `health`)
**Removed commands:** 0
**Breaking changes:** 0 (all additions are backward compatible)
**Critical fix:** 1 (JSON error output)

The protocol v2 surface is the existing surface + targeted additions. Nothing removed. Nothing renamed. Full backward compatibility.

---

## 10. MCP Server: Structured Protocol Transport

### The Case for MCP

The CLI protocol works — agents shell out to `kbmdx` and parse `--json` output. But it has friction:

1. **Flag parsing is lossy.** A missing `--` or a misquoted string breaks the call. AI agents generate CLI commands imperfectly.
2. **Format negotiation.** Every call needs `--json` or `KANBAN_OUTPUT=json`. Forget it once and you're parsing table output.
3. **Error channel is split.** Results on stdout, errors on stderr, exit code as a third signal. Three channels to coordinate.
4. **No session state.** Each `kbmdx` invocation is stateless. The agent identity (`$KANBAN_AGENT`) must be threaded through every call via env var or flag.
5. **No streaming.** `log --follow` requires holding a subprocess open and parsing its stdout stream. Fragile.

MCP (Model Context Protocol) solves all five. Tools have typed JSON Schema parameters — no flag parsing. Responses are structured JSON — no format negotiation. Errors are part of the response — one channel. The server maintains session state — agent identity set once. Notifications enable streaming — no subprocess management.

### Architecture

```
┌──────────────────────────────┐
│        AI Agent              │
│  (Claude Code / Cursor)      │
│                              │
│  ┌────────────────────────┐  │
│  │     MCP Client         │  │     ┌──────────────────────┐
│  │  (built into agent)    │◄─┼────►│   kbmdx-mcp-server   │
│  └────────────────────────┘  │     │                      │
│                              │     │  Session state:      │
│  ┌────────────────────────┐  │     │  - agent_name        │
│  │     Shell / Bash       │  │     │  - board_dir         │
│  │  kbmdx pick --claim .. │  │     │  - claim_timeout     │
│  └────────────────────────┘  │     │                      │
│                              │     │  Calls kbmdx-core    │
└──────────────────────────────┘     │  library directly    │
                                     └──────────┬───────────┘
                                                │
                                     ┌──────────▼───────────┐
                                     │     kbmdx-core       │
                                     │     (lib crate)      │
                                     │                      │
                                     │  Tasks, Claims,      │
                                     │  Config, Board,      │
                                     │  Log, Metrics        │
                                     └──────────────────────┘
```

**Key insight:** The MCP server and CLI binary both link against the same `kbmdx-core` library crate. The MCP server doesn't shell out to `kbmdx` — it calls the same Rust functions the CLI calls. Zero overhead, no subprocess spawning, no output parsing.

This requires extracting the current binary's logic into a library crate (already partially done — `src/board/`, `src/model/`, `src/io/` are library-shaped). The CLI becomes a thin clap wrapper around the library. The MCP server becomes a thin JSON-RPC wrapper around the same library.

### Workspace Layout

```
kanban-mdx/
├── kbmdx-core/              # Library crate (extracted from current src/)
│   ├── src/
│   │   ├── model/           # Task, Config structs
│   │   ├── board/           # Board operations (list, pick, deps, metrics, log)
│   │   ├── io/              # File I/O (task_file, config_file)
│   │   └── util/            # Git, date, agent name, file lock
│   └── Cargo.toml
├── kbmdx/                   # CLI binary (thin clap wrapper)
│   ├── src/
│   │   ├── main.rs
│   │   ├── cli/             # Command definitions + dispatch
│   │   └── output/          # Table, compact, JSON formatters
│   └── Cargo.toml
├── kbmdx-mcp/               # MCP server binary
│   ├── src/
│   │   ├── main.rs          # stdio transport setup
│   │   ├── tools.rs         # Tool definitions
│   │   ├── session.rs       # Session state (agent name, board dir)
│   │   └── events.rs        # Notification emitters
│   └── Cargo.toml
├── kbmdx-dispatch/           # Dispatcher binary (from task #89)
│   └── ...
├── tui-md/                   # Existing sub-crate
├── sembed-rs/                # Existing sub-crate
└── Cargo.toml                # Workspace root
```

### MCP Tool Surface

Map the protocol v2 CLI surface to MCP tools. Each tool has typed JSON Schema parameters and returns structured JSON. No format flags, no flag parsing, no stderr.

#### Read Tools (no side effects)

**`kbmdx_board`** — Board overview
```json
{
  "name": "kbmdx_board",
  "parameters": {
    "group_by": { "type": "string", "enum": ["assignee", "tag", "class", "priority", "status"] },
    "parent_id": { "type": "integer" }
  }
}
```
Returns: `Overview` (board name, status columns with counts/WIP, priority/class breakdowns)

**`kbmdx_show`** — Task detail
```json
{
  "name": "kbmdx_show",
  "parameters": {
    "id": { "type": "integer", "required": true },
    "hydrate": { "type": "boolean", "default": false },
    "section": { "type": "string" },
    "include_children": { "type": "boolean", "default": false }
  }
}
```
Returns: `Task` (or `HydratedTask` with deps/parent/activity/siblings when hydrate=true)

**`kbmdx_list`** — Query tasks
```json
{
  "name": "kbmdx_list",
  "parameters": {
    "status": { "type": "array", "items": { "type": "string" } },
    "priority": { "type": "array", "items": { "type": "string" } },
    "tags": { "type": "array", "items": { "type": "string" } },
    "assignee": { "type": "string" },
    "claimed_by": { "type": "string" },
    "unclaimed": { "type": "boolean" },
    "blocked": { "type": "boolean" },
    "not_blocked": { "type": "boolean" },
    "unblocked": { "type": "boolean" },
    "requires": { "type": "array", "items": { "type": "string" } },
    "class": { "type": "string" },
    "parent_id": { "type": "integer" },
    "search": { "type": "string" },
    "context": { "type": "boolean" },
    "sort": { "type": "string", "enum": ["id", "status", "priority", "created", "updated", "due"] },
    "reverse": { "type": "boolean" },
    "limit": { "type": "integer" },
    "group_by": { "type": "string" },
    "no_body": { "type": "boolean", "default": true }
  }
}
```
Returns: `Task[]` or `GroupedSummary`

**`kbmdx_deps`** — Dependency graph
```json
{
  "name": "kbmdx_deps",
  "parameters": {
    "id": { "type": "integer", "required": true },
    "direction": { "type": "string", "enum": ["upstream", "downstream", "both"], "default": "upstream" },
    "transitive": { "type": "boolean", "default": false }
  }
}
```
Returns: `DepsOutput`

**`kbmdx_health`** — Board health
```json
{
  "name": "kbmdx_health",
  "parameters": {}
}
```
Returns: `HealthReport` (stale claims, orphan worktrees, blocked/overdue counts, WIP violations)

**`kbmdx_metrics`** — Flow metrics
```json
{
  "name": "kbmdx_metrics",
  "parameters": {
    "since": { "type": "string", "format": "date" },
    "parent_id": { "type": "integer" },
    "agent": { "type": "string" }
  }
}
```
Returns: `Metrics`

**`kbmdx_log`** — Activity log
```json
{
  "name": "kbmdx_log",
  "parameters": {
    "since": { "type": "string", "format": "date" },
    "limit": { "type": "integer" },
    "action": { "type": "string" },
    "task_id": { "type": "integer" },
    "claimed_by": { "type": "string" }
  }
}
```
Returns: `LogEntry[]`

**`kbmdx_context`** — Board context for agent prompts
```json
{
  "name": "kbmdx_context",
  "parameters": {
    "sections": { "type": "array", "items": { "type": "string", "enum": ["in-progress", "blocked", "overdue", "recently-completed"] } },
    "days": { "type": "integer", "default": 7 },
    "agent": { "type": "string" },
    "requires": { "type": "array", "items": { "type": "string" } }
  }
}
```
Returns: Markdown string (context document)

**`kbmdx_find`** — Semantic search
```json
{
  "name": "kbmdx_find",
  "parameters": {
    "query": { "type": "string", "required": true },
    "limit": { "type": "integer", "default": 10 }
  }
}
```
Returns: `FindResult[]`

#### Write Tools (mutating, side effects)

**`kbmdx_create`** — Create task
```json
{
  "name": "kbmdx_create",
  "parameters": {
    "title": { "type": "string", "required": true },
    "status": { "type": "string" },
    "priority": { "type": "string" },
    "assignee": { "type": "string" },
    "tags": { "type": "array", "items": { "type": "string" } },
    "due": { "type": "string", "format": "date" },
    "estimate": { "type": "string" },
    "parent_id": { "type": "integer" },
    "depends_on": { "type": "array", "items": { "type": "integer" } },
    "body": { "type": "string" },
    "class": { "type": "string" },
    "requires": { "type": "array", "items": { "type": "string" } },
    "claim": { "type": "boolean", "default": false }
  }
}
```
Returns: Created `Task` (claim uses session agent name automatically)

**`kbmdx_edit`** — Edit task(s)
```json
{
  "name": "kbmdx_edit",
  "parameters": {
    "ids": { "type": "array", "items": { "type": "integer" }, "required": true },
    "title": { "type": "string" },
    "priority": { "type": "string" },
    "status": { "type": "string" },
    "assignee": { "type": "string" },
    "tags": { "type": "array", "items": { "type": "string" } },
    "add_tags": { "type": "array", "items": { "type": "string" } },
    "remove_tags": { "type": "array", "items": { "type": "string" } },
    "due": { "type": "string", "format": "date" },
    "clear_due": { "type": "boolean" },
    "estimate": { "type": "string" },
    "class": { "type": "string" },
    "body": { "type": "string" },
    "append_body": { "type": "string" },
    "timestamp": { "type": "boolean" },
    "set_section": { "type": "string" },
    "section_body": { "type": "string" },
    "parent_id": { "type": "integer" },
    "clear_parent": { "type": "boolean" },
    "depend": { "type": "integer" },
    "undepend": { "type": "integer" },
    "block": { "type": "string" },
    "unblock": { "type": "boolean" },
    "branch": { "type": "string" },
    "worktree": { "type": "string" },
    "requires": { "type": "array", "items": { "type": "string" } },
    "claim": { "type": "boolean" },
    "release": { "type": "boolean" },
    "force": { "type": "boolean" }
  }
}
```
Returns: `Task` or `Task[]` (claim/release uses session agent name)

**`kbmdx_move`** — Move task status
```json
{
  "name": "kbmdx_move",
  "parameters": {
    "id": { "type": "integer", "required": true },
    "status": { "type": "string" },
    "next": { "type": "boolean" },
    "prev": { "type": "boolean" },
    "claim": { "type": "boolean" },
    "force": { "type": "boolean" }
  }
}
```
Returns: Updated `Task`

**`kbmdx_pick`** — Atomic pick
```json
{
  "name": "kbmdx_pick",
  "parameters": {
    "status": { "type": "string" },
    "move_to": { "type": "string" },
    "tags": { "type": "array", "items": { "type": "string" } },
    "requires": { "type": "array", "items": { "type": "string" } }
  }
}
```
Returns: Picked `Task` (always claims using session agent name)

**`kbmdx_handoff`** — Handoff task
```json
{
  "name": "kbmdx_handoff",
  "parameters": {
    "id": { "type": "integer", "required": true },
    "note": { "type": "string" },
    "timestamp": { "type": "boolean", "default": true },
    "block": { "type": "string" },
    "release": { "type": "boolean", "default": true },
    "set_section": { "type": "string" },
    "section_body": { "type": "string" }
  }
}
```
Returns: Updated `Task`

**`kbmdx_delete`** — Delete task(s)
```json
{
  "name": "kbmdx_delete",
  "parameters": {
    "ids": { "type": "array", "items": { "type": "integer" }, "required": true }
  }
}
```
Returns: `DeleteResult[]`

**`kbmdx_archive`** — Archive task(s)
```json
{
  "name": "kbmdx_archive",
  "parameters": {
    "ids": { "type": "array", "items": { "type": "integer" }, "required": true }
  }
}
```
Returns: Archived `Task[]`

#### Session Tools

**`kbmdx_session`** — Session management
```json
{
  "name": "kbmdx_session",
  "parameters": {
    "action": { "type": "string", "enum": ["start", "status", "rename"], "required": true },
    "name": { "type": "string" }
  }
}
```
- `start` — generates agent name (or uses provided `name`), stores in session state. All subsequent tool calls use this identity for claims.
- `status` — returns current session state (agent name, board dir, active claims)
- `rename` — changes session agent name

Returns: `{ "agent_name": "frost-maple", "board": "kanban-mdx", "active_claims": [42, 55] }`

### Session State: The Key Difference from CLI

The MCP server is long-lived. It maintains state across tool calls:

```rust
struct Session {
    agent_name: String,          // set on session start, used for all claims
    board_dir: PathBuf,          // resolved once from cwd or config
    config: Config,              // loaded once, refreshed on file change
    claim_timeout: Duration,     // cached from config
}
```

**Implications:**
- `claim: true` on any write tool means "claim as this session's agent" — no need to pass the agent name every time
- `release: true` means "release this session's claim" — validates you own it
- `pick` always claims as the session agent — the `--claim` flag becomes implicit
- The server can proactively renew claims for in-progress tasks (background timer)
- The server can watch `activity.jsonl` and emit notifications on relevant changes

### MCP Notifications (Event Streaming)

MCP supports server-to-client notifications. The kbmdx-mcp server can watch the board directory and emit events:

```json
{"method": "notifications/kbmdx/task_moved", "params": {"task_id": 42, "from": "todo", "to": "in-progress", "agent": "frost-maple"}}
{"method": "notifications/kbmdx/task_created", "params": {"task_id": 89, "title": "New task"}}
{"method": "notifications/kbmdx/claim_expired", "params": {"task_id": 42, "agent": "frost-maple"}}
{"method": "notifications/kbmdx/health_alert", "params": {"stale_claims": 2, "wip_violations": 1}}
```

This replaces `log --follow` entirely for MCP-connected agents. No subprocess management, no stdout parsing — the agent's MCP client receives typed events.

### CLI vs. MCP: When to Use Which

| Scenario | CLI | MCP |
|----------|-----|-----|
| Human interactive use | Yes | No |
| Agent with shell access (Claude Code skill) | Yes | Optional |
| Agent without shell access | No | Yes |
| Dispatcher (kbmdx-dispatch) | Yes (calls CLI) | Possible (calls library directly) |
| CI/CD pipelines | Yes | No |
| Multi-agent with shared board | Yes (via claims) | Yes (via sessions) |
| Event streaming | `log --follow` | Notifications |
| Bulk scripting | Yes (pipes, xargs) | No (tool-at-a-time) |

**The CLI is not replaced.** The MCP server is an additional transport. Both call the same library. Both produce the same data. The MCP server adds session state and event streaming that the stateless CLI cannot provide.

### Tool Count

| Category | Tools | Notes |
|----------|-------|-------|
| Read | 9 | board, show, list, deps, health, metrics, log, context, find |
| Write | 6 | create, edit, move, pick, handoff, delete, archive |
| Session | 1 | session (start/status/rename) |
| **Total** | **16** | Compared to 24 CLI commands — reduced by merging and dropping human-only commands |

**Dropped from MCP surface** (human-only or infrastructure):
- `init` — one-time setup, always done by human
- `tui` / `read` — interactive UI
- `completion` — shell completions
- `filepath` — low-level, agents use `show` instead
- `undo` / `redo` — interactive safety net, not agent workflow
- `config` / `migrate-config` — admin commands
- `skill` / `embed` — infrastructure management
- `import` — bulk creation via CLI is more natural
- `worktrees` / `branch-check` / `gitignore` — folded into `health` or irrelevant

### Implementation Cost

**Prerequisites:**
- Extract `kbmdx-core` library crate from current `src/` — ~1-2 days (move `model/`, `board/`, `io/`, `util/` into a lib crate; CLI becomes a thin wrapper)
- This refactor benefits all three binaries (CLI, MCP, dispatch)

**MCP server itself:**
| Component | Estimate |
|-----------|----------|
| MCP transport setup (stdio) | ~100 loc |
| Tool definitions (16 tools, JSON Schema) | ~400 loc |
| Session state management | ~150 loc |
| File watcher + notification emitter | ~200 loc |
| Claim auto-renewal background task | ~80 loc |
| Error mapping (CliError → MCP error) | ~50 loc |
| **Total** | **~1,000 loc** |

### Integration with Skills

The existing `kanban-based-development` and `kanban-mdx` skills teach agents to use CLI commands. With MCP, agents don't need skills for command syntax — the tool schemas ARE the documentation. But the workflow guidance (claim before change, one task per agent, defer-to-user boundary) still lives in skills.

A revised skill for MCP-connected agents would be shorter: drop the command reference tables and focus purely on workflow rules and the decision tree. The tool schemas handle the "how to call" part; the skill handles the "when and why" part.

### Relationship to kbmdx-dispatch

The dispatcher (task #89) can use either transport:

- **CLI path:** `kbmdx-dispatch` shells out to `kbmdx` commands — simple, works today
- **Library path:** `kbmdx-dispatch` links against `kbmdx-core` directly — no subprocess overhead
- **MCP path:** `kbmdx-dispatch` connects to `kbmdx-mcp` as an MCP client — gets session state and notifications for free

The library path is the most natural for the dispatcher since it's a Rust binary in the same workspace. MCP is for AI agents that have built-in MCP client support. The dispatcher is not an AI agent — it's a scheduler.

### Build Path

1. **Phase 0: Extract `kbmdx-core` library crate.** This is prerequisite for both MCP and clean CLI architecture. Move `model/`, `board/`, `io/`, `util/` into a lib crate. CLI binary depends on it.

2. **Phase 1: MCP server with read tools.** `board`, `show`, `list`, `log`, `health`, `context`, `find`, `metrics`, `deps`. Session start. No mutations — safe to deploy and iterate.

3. **Phase 2: Add write tools.** `create`, `edit`, `move`, `pick`, `handoff`, `delete`, `archive`. Implicit claim via session identity.

4. **Phase 3: Notifications.** File watcher, event streaming, claim auto-renewal. This is what makes MCP superior to CLI for long-running agent sessions.
