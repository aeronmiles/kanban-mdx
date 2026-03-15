---
id: 89
title: 'Protocol + Adapters: agent integration architecture'
status: references
priority: medium
created: '2026-03-15T09:33:40.289991Z'
updated: '2026-03-15T09:33:40.289991Z'
tags:
- design
- agents
- architecture
---

## Protocol + Adapters: Agent Integration Architecture

### The Core Insight

kbmdx is already ~70% of a protocol. The `--json` output is a stable schema. The `activity.jsonl` is an event stream. Error codes (`TASK_CLAIMED`, `NOTHING_TO_PICK`) give adapters clear control flow. Claims are a lease-based coordination primitive with built-in expiry.

What's missing isn't a new system — it's filling the gaps in the existing surface so that an external adapter can do the orchestration kbmdx shouldn't.

### Architecture

```
                          ┌──────────────────────────────────┐
                          │           kbmdx (core)           │
                          │                                  │
                          │  Tasks ←→ Claims ←→ Activity Log │
                          │       ↑                          │
                          │   config.toml                    │
                          │   dispatch.toml (new, optional)  │
                          └──────────┬───────────────────────┘
                                     │ CLI (--json)
                          ┌──────────▼───────────────────────┐
                          │       kbmdx-dispatch (adapter)   │
                          │                                  │
                          │  Watch board → Match → Spawn     │
                          │  Monitor claims → Detect failure │
                          │  Pool management → Backpressure  │
                          └───┬──────────┬──────────┬────────┘
                              │          │          │
                         ┌────▼───┐ ┌────▼───┐ ┌───▼─────┐
                         │ Claude │ │ Cursor │ │  Codex  │
                         │  Code  │ │        │ │         │
                         └────────┘ └────────┘ └─────────┘
```

**Separation of concerns:**
- **kbmdx** owns: task state, claims, activity log, config, validation
- **kbmdx-dispatch** owns: agent lifecycle, pool management, routing, failure recovery
- **Runtime adapters** own: translating kbmdx context into agent-specific invocations

---

## Protocol Layer 1: Discovery & Hydration

Additions to kbmdx core that make the existing CLI a better protocol endpoint.

### 1a. Task Hydration Bundle — `kbmdx show ID --hydrate`

The single biggest gap. Today, an adapter must make 3-4 calls to get everything an agent needs:

```bash
kbmdx show 42 --json           # task itself
kbmdx deps 42 --json           # dependency context
kbmdx log --task 42 --json     # recent activity
kbmdx show 10 --json           # parent task (if any)
```

`--hydrate` bundles this into one call:

```json
{
  "task": { /* full task object */ },
  "parent": { "id": 10, "title": "Epic: Auth Rewrite", "status": "in-progress" },
  "dependencies": {
    "upstream": [{ "id": 40, "title": "Design API", "status": "done" }],
    "downstream": [{ "id": 45, "title": "Write docs", "status": "backlog" }]
  },
  "activity": [
    {"timestamp": "...", "action": "move", "detail": "todo -> in-progress"},
    {"timestamp": "...", "action": "edit", "detail": "body updated"}
  ],
  "siblings": [
    { "id": 41, "title": "Implement auth middleware", "status": "in-progress", "claimed_by": "frost-maple" }
  ]
}
```

**Why this matters:** An agent starting a task needs to understand not just *what* the task says, but *what came before* (activity), *what surrounds it* (siblings, parent), and *what constrains it* (dependencies). Without `--hydrate`, every adapter re-implements this assembly.

### 1b. Capability Tags on Tasks

Tasks gain an optional `requires` field:

```yaml
requires:
  - rust
  - tui
```

Not enforced by kbmdx — it's metadata. But `kbmdx pick` can filter on it:

```bash
kbmdx pick --claim <agent> --requires rust,tui --status todo --move in-progress
```

The adapter matches agent capabilities to task requirements. kbmdx just provides the filter.

**Minimal addition:** One new optional field on Task, one new filter option on `pick` and `list`. No agent registry in kbmdx itself.

### 1c. Stale Claim Detection — `kbmdx health`

```bash
kbmdx health --json
```

```json
{
  "stale_claims": [
    { "id": 42, "claimed_by": "frost-maple", "claimed_at": "...", "expired_ago": "23m" }
  ],
  "orphan_worktrees": [
    { "id": 38, "worktree": "../kbmdx-task-38", "exists": false }
  ],
  "blocked_count": 3,
  "overdue_count": 1
}
```

Today, stale claims are only checked during `pick`. `health` exposes them proactively so adapters can release and re-queue without waiting for the next pick attempt.

---

## Protocol Layer 2: Lifecycle Events

### 2a. Session Correlation

No new command needed. The adapter does:

```bash
AGENT=$(kbmdx agent-name)
```

kbmdx gains session correlation in the activity log. When an agent is active, its claim/release/move events can be grouped by agent name:

```bash
kbmdx log --claimed-by frost-maple --json
```

New filter on `log` command. Zero new data model changes — just a query filter.

### 2b. Structured Progress (Lightweight, Convention-Based)

Rather than inventing a new `checkin` command, use the existing `--set-section` mechanism:

```bash
kbmdx edit 42 -a "Completed module A, starting tests." -t --claim frost-maple \
  --set-section agent-status --section-body "phase: testing\nfiles: src/tui/app.rs, src/tui/keys/mod.rs"
```

Writes a structured section in the task body:

```markdown
## Agent Status
phase: testing
files: src/tui/app.rs, src/tui/keys/mod.rs
```

The adapter reads this via `kbmdx show 42 --section agent-status`. No new fields on the Task struct. The section name `agent-status` is a protocol convention, not a schema change.

**Why this is better than new fields:** Keeps the Task model at 30 fields. Progress data is ephemeral — only meaningful during execution. In the body, it naturally becomes part of the task's narrative history rather than cluttering frontmatter.

### 2c. Completion Outcomes

Use body sections for structured outcomes:

```bash
kbmdx edit 42 --set-section outcome \
  --section-body "result: success\ncommits: 3\nfiles_changed: 7\ntests: 42 passed, 0 failed"
```

For handoffs:

```bash
kbmdx handoff 42 --claim frost-maple \
  --set-section outcome \
  --section-body "result: deferred\nreason: needs product decision\nquestion: Should we use OAuth2 or SAML?" \
  --release
```

The adapter parses outcomes to decide next steps (retry, escalate, move to next task).

---

## Protocol Layer 3: Observability

### 3a. Board Watch — `kbmdx watch`

The **highest-value protocol addition**. Today, adapters must poll:

```bash
while true; do kbmdx list --json --status todo --unclaimed; sleep 10; done
```

`kbmdx watch` uses the same `notify` file-watcher the TUI already has:

```bash
kbmdx watch --json --events move,create,claim,release
```

Output: one JSON event per line (JSONL streaming), emitted when task files change:

```json
{"timestamp":"...","event":"move","task_id":42,"from":"todo","to":"in-progress","agent":"frost-maple"}
{"timestamp":"...","event":"create","task_id":89,"title":"New task","status":"backlog"}
{"timestamp":"...","event":"claim_expired","task_id":42,"agent":"frost-maple","expired_at":"..."}
```

**Implementation:** The TUI already sets up a `notify::RecommendedWatcher` on the tasks directory. Extract this into a shared utility. `kbmdx watch` formats the diff between old and new state as JSON events.

**This is the single feature that transforms kbmdx from "poll-based API" to "event-driven protocol."**

### 3b. Agent Metrics

`kbmdx metrics --json` gains agent-specific data derived from activity log correlation:

```json
{
  "agent_metrics": {
    "frost-maple": {
      "tasks_completed": 3,
      "avg_cycle_time": "2h 15m",
      "current_task": 42,
      "session_duration": "4h 30m"
    }
  }
}
```

No new data — just new queries on existing data.

---

## Adapter Architecture: `kbmdx-dispatch`

Separate binary in the workspace. Optional — kbmdx works fine without it.

### Configuration: `.kbmdx/dispatch.toml`

```toml
[pool]
max_concurrent = 3
pick_interval = "30s"
claim_renewal_interval = "20m"

[[agents]]
name = "coder"
runtime = "claude-code"
capabilities = ["rust", "typescript", "testing"]
instances = 2
pick_from = ["todo", "backlog"]
worktree_pattern = "../{board}-task-{id}"

[agents.runtime_config]
model = "opus"
skills = ["kanban-based-development"]
max_turns = 200

[[agents]]
name = "reviewer"
runtime = "claude-code"
capabilities = ["code-review", "docs"]
instances = 1
pick_from = ["review"]

[agents.runtime_config]
model = "sonnet"
skills = ["kanban-based-development"]
max_turns = 50
```

**kbmdx never reads this file.** It's solely for `kbmdx-dispatch`. Core stays clean.

### Dispatch Loop

```
┌─────────────────────────────────────────────┐
│              kbmdx-dispatch                 │
│                                             │
│  1. Read dispatch.toml                      │
│  2. Start kbmdx watch (or poll fallback)    │
│  3. Loop:                                   │
│     a. Check capacity (running < max)       │
│     b. Pick task matching available agent    │
│     c. Hydrate task context                 │
│     d. Spawn agent runtime process          │
│     e. Monitor:                             │
│        - Process alive? (OS-level)          │
│        - Claim still valid? (kbmdx health)  │
│        - Progress? (read agent-status)      │
│     f. On completion:                       │
│        - Parse outcome section              │
│        - If success → verify → done         │
│        - If deferred → log, pick next       │
│        - If crash → release claim, re-queue │
│     g. Renew claims for active agents       │
│  4. On signal (Ctrl+C):                     │
│     - Release all claims                    │
│     - Wait for running agents to finish     │
│     - Exit                                  │
└─────────────────────────────────────────────┘
```

### Runtime Adapters

Each runtime is a trait implementation:

```rust
trait RuntimeAdapter {
    fn spawn(&self, ctx: &TaskContext, config: &RuntimeConfig) -> Result<Child>;
    fn inject_context(&self, ctx: &TaskContext) -> String;
}

struct ClaudeCodeAdapter;
impl RuntimeAdapter for ClaudeCodeAdapter {
    fn spawn(&self, ctx: &TaskContext, config: &RuntimeConfig) -> Result<Child> {
        Command::new("claude")
            .arg("--print")
            .arg("--model").arg(&config.model)
            .arg("--prompt").arg(self.inject_context(ctx))
            .current_dir(&ctx.worktree_path)
            .spawn()
    }

    fn inject_context(&self, ctx: &TaskContext) -> String {
        format!(
            "You are agent `{}`. Your task:\n\n{}\n\n\
             ## Board context:\n{}\n\n\
             Follow the kanban-based-development skill workflow.",
            ctx.agent_name, ctx.hydrated_task, ctx.board_context
        )
    }
}
```

New runtimes are just new `impl RuntimeAdapter` blocks.

---

## Failure Modes & Recovery

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Agent process crashes | Process exit code != 0 | Release claim, log error, re-queue task |
| Agent hangs (no progress) | Claim timeout approaches | Renew claim; if no activity in 2x timeout → kill + re-queue |
| Agent produces broken code | `kbmdx verify` fails | Move to `review` with block reason, pick next |
| kbmdx-dispatch crashes | OS-level (systemd, launchd) | On restart: scan for stale claims, release owned |
| Race condition (two dispatchers) | `TASK_CLAIMED` error from pick | Normal — pick is atomic, second gets different task |
| Board locked | `try_lock` returns None | Back off, retry in 1s. Just contention |

The file lock + atomic pick + claim timeout trio means most failures self-heal. A crashed agent's claim expires, and the next pick cycle grabs the task.

---

## Implementation Cost

### kbmdx Core Additions (Protocol Surface)

| Addition | Effort | Value |
|----------|--------|-------|
| `kbmdx watch --json` | Medium (~200 loc) | **Critical** — polling to events, reuses TUI file watcher |
| `kbmdx show ID --hydrate` | Low (~150 loc) | High — one call replaces four, pure query composition |
| `kbmdx health --json` | Low (~100 loc) | High — stale claims + orphan worktrees |
| `requires` field + pick filter | Low (~50 loc) | Medium — one optional field, one filter |
| `kbmdx log --claimed-by` | Trivial (~20 loc) | Medium — filter on existing data |
| Agent-status section convention | Zero (docs only) | Medium — uses existing `--set-section` |

**Total new code in kbmdx core: ~400-600 lines.** No new dependencies. No daemon. No runtime coupling.

### kbmdx-dispatch (Adapter Layer)

| Component | Estimate |
|-----------|----------|
| `dispatch.toml` parser | ~200 loc |
| Dispatch loop + pool manager | ~400 loc |
| Claude Code adapter | ~100 loc |
| Claim renewal background task | ~100 loc |
| Health monitor | ~150 loc |
| Graceful shutdown | ~100 loc |
| CLI (`kbmdx-dispatch start/status/stop`) | ~200 loc |
| **Total** | **~1,250 loc** |

Separate binary, separate crate, separate release cycle.

---

## Incremental Build Path

### Phase 1: Protocol Surface (kbmdx core)

Add `watch`, `--hydrate`, `health`, and `--claimed-by` log filter. Ship a 50-line shell script as a proof-of-concept adapter that polls and spawns Claude Code. Validate the protocol design before investing in the full dispatcher.

### Phase 2: Full Dispatcher (kbmdx-dispatch)

Build the Rust adapter binary with pool management, Claude Code runtime adapter, claim renewal, and failure recovery. Ship as a workspace crate.

### Phase 3: Multi-Runtime & TUI Integration

Add Cursor/Codex adapters. Add agent fleet status to the TUI board view (who's working on what, progress phases, health indicators). Add `requires` capability matching.

---

## Design Principles

1. **kbmdx never imports agent runtime code.** The core binary has zero knowledge of Claude Code, Cursor, or any agent. It only knows about tasks, claims, and events.

2. **The protocol is the CLI.** No IPC, no sockets, no gRPC. `--json` output + exit codes + JSONL streaming. Any language can build an adapter.

3. **Convention over schema.** Agent progress and outcomes use `--set-section` body conventions, not new frontmatter fields. The task model stays lean.

4. **Failures self-heal.** Claim timeout + atomic pick means crashed agents don't leave permanent damage. The dispatcher just needs to detect and re-queue.

5. **Separate crate, separate release.** `kbmdx-dispatch` can iterate faster than kbmdx core. Breaking adapter changes don't break the board.
