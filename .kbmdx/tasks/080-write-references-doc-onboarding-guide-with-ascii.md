---
id: 80
title: 'Write references doc: onboarding guide with ASCII workflow diagrams'
status: done
priority: medium
created: 2026-03-13T14:17:43.8174Z
updated: 2026-03-13T14:36:12.837376Z
started: 2026-03-13T14:36:12.837376Z
completed: 2026-03-13T14:36:12.837376Z
tags:
    - layer-3
    - docs
class: standard
---

Create a comprehensive references document for kanban-mdx that serves as both an onboarding guide for new users/agents and a visual workflow reference. Target location: a new reference file under the kbmdx skill (e.g. `src/skill/skills/kbmdx/references/workflows.md`) or a standalone `docs/workflows.md`.

## What to cover

### 1. Task Lifecycle State Machine
ASCII diagram showing the full status flow:
```
backlog → todo → in-progress → review → done → archived
```
With annotations for:
- Which statuses require claims (`require_claim`)
- Which statuses require branch matching (`require_branch`)
- Auto-timestamps: `started` (first move from initial) and `completed` (move to terminal)
- WIP limits enforcement points
- `--force` bypass semantics

### 2. Agent Session Lifecycle
ASCII flow diagram covering the full mandatory loop:
```
agent-name → board → pick --claim → show → worktree add →
  implement → merge → edit --release → move done → cleanup → pick next
```
Including the parking/handoff branch:
```
  ... → blocked? → handoff --block --release → pick next
  ... → resume: edit --claim → unblock → move in-progress
```

### 3. Worktree Workflow
Visual diagram of the board-home vs worktree relationship:
- Board home: owns `kanban/` tasks dir, runs `kbmdx` commands, merges branches
- Worktree: isolated code changes, tests, commits
- How `task.branch` and `task.worktree` fields track active worktrees
- Stale/orphan detection via `kbmdx worktrees --check`
- Branch naming convention: `task/<ID>-<kebab-description>`

### 4. Claim Coordination Model
Diagram showing multi-agent claim flow:
- Atomic `pick --claim` prevents race conditions
- Claim timeout (default 1h) and renewal via `--claim`
- Never-steal-claims rule
- `KANBAN_AGENT` env var fallback

### 5. Context Resolution
How `--context` / `-C` works:
- Branch → task resolution (exact match, then `task/<ID>-*` convention)
- Context expansion: root task + parent + siblings + upstream deps + downstream dependents + same-claimant tasks

### 6. Board Modes (TUI)
AppView state machine:
```
Board ↔ Detail ↔ MoveTask
  ↕        ↕
Help   Search   CreateTask   ContextPicker   BranchPicker
```
With key bindings for each transition.

### 7. Output Format Decision Tree
```
Need structured data? → --json
Agent/LLM context?    → --compact
Human reading?        → (default table)
Single task for LLM?  → show --prompt --fields
```

### 8. Configuration Enforcement Rules
Per-status rules table:
| Status | require_claim | require_branch | show_duration |
And how `--force` interacts with each.

### 9. Class of Service & Priority Sorting
How `pick` sorts candidates:
- Class priority (expedite > fixed-date > standard > intangible)
- Within fixed-date: sort by due date
- Within same class: sort by priority index, then ID (oldest first)

## Design guidelines
- Use ASCII box-drawing characters for diagrams (compatible with markdown rendering and terminal display)
- Keep each diagram self-contained and annotated
- Include `kbmdx` command examples alongside each workflow step
- Reference the kbmdx SKILL.md and kanban-based-development SKILL.md as authoritative sources
- Target audience: AI agents being onboarded to a kanban-mdx project for the first time, and human developers setting up multi-agent workflows
