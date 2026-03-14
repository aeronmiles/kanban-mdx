---
id: 4
title: Worktree-transparent board access
status: done
priority: low
created: 2026-03-10T10:18:32.198169Z
updated: 2026-03-11T06:52:57.227561Z
started: 2026-03-10T10:25:27.191829Z
completed: 2026-03-10T10:50:21.995675Z
tags:
- layer-3
class: standard
branch: main
---

## Summary

Make `kanban-mdx` commands work from any git worktree by automatically resolving the board in the main worktree. Currently, `FindDir()` fails in worktrees because `kanban/` is `.gitignore`-d and never present. This forces users/agents back to board home for every board mutation.

## Research

See `docs/research/2026-03-10-worktree-context-switching.md` for the full design exploration (five options evaluated).

## Implementation plan

### Phase 1: Worktree-transparent FindDir (core)

Enhance `config.FindDir()` to detect git worktrees and resolve the main worktree's board:

1. After the normal upward walk fails, check if cwd is inside a git worktree:
   - Walk up looking for `.git` (file, not directory)
   - Parse `gitdir: <path>` from the `.git` file
   - Navigate `../../` from that path to reach main `.git` dir, then parent for repo root
2. Retry `FindDir()` from the main worktree root
3. Return the canonical board path (in main worktree)

**Key function to add:** `resolveMainWorktree(dir string) (string, error)` in `internal/config/`.

**Files to modify:**
- `internal/config/config.go` — `FindDir()` fallback logic
- `internal/config/config_test.go` — test with synthetic worktree `.git` file

**Recommendations:**
- Default-on, no config flag. The behavior is strictly additive (only triggers when normal discovery fails) and has zero cost in the common case.
- Pure Go parsing of the `.git` file, no subprocess. The `gitdir:` format is stable across git versions.
- Guard against infinite recursion: if the resolved main root also fails, return the original error.
- Skip worktree resolution if `--dir` is explicitly set (explicit overrides implicit).

### Phase 2: Auto-detect context (nice-to-have)

When running from a worktree, auto-detect which task the user is working on by matching the current branch against task `branch` fields.

- `pick --claim <agent>`: auto-set `branch` and `worktree` from current git context
- `list`/`show`: annotate output with "you are in the worktree for task #N"
- Add a `kbmdx which` command that prints the current task based on branch detection

### Phase 3: `switch`/`park` workflow commands (follow-up)

Automate the worktree lifecycle:
- `kbmdx switch <ID>`: pick + claim + `git worktree add` + print shell `cd` command
- `kbmdx park [<ID>]`: merge to main + move done + `git worktree remove`
- Shell integration: `kanban-switch()` zsh/bash wrapper that evals the cd

This is a separate task — depends on Phase 1 being complete.

## Testing strategy

- Unit test `resolveMainWorktree()` with a synthetic `.git` file (no real git repo needed)
- Integration test: create a real worktree with `git worktree add`, run `FindDir()` from inside it
- Verify lock file path is identical whether resolved from main or worktree
- Verify `--dir` bypasses worktree resolution
- Backward compat: existing behavior unchanged when not in a worktree

## Scope boundaries

- **In scope:** Phase 1 only (FindDir enhancement)
- **Out of scope:** Phase 2 and 3 are follow-up tasks. Board-per-worktree (Option B) is explicitly rejected. No daemon/service model.
- **Not needed:** Config migration (no schema change). No new CLI flags.
