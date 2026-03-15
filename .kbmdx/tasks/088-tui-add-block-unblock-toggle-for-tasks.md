---
id: 88
title: 'TUI: add block/unblock toggle for tasks'
status: done
priority: medium
created: '2026-03-15T09:12:51.751289Z'
updated: '2026-03-15T09:20:12.512968Z'
started: '2026-03-15T09:20:12.512893Z'
completed: '2026-03-15T09:20:12.512893Z'
---

Add a key binding in the TUI to toggle the blocked flag on the active task, with an optional block reason prompt. Currently blocking is CLI-only (kbmdx edit <ID> --block/--unblock). The TUI can see and filter blocked tasks but cannot set the flag.
[2026-03-15 09:15] ## Implementation Plan

### Approach
Use the existing `e` (edit) overlay pattern as a reference. Add a simple `B` key binding (board + detail views) that toggles `task.blocked` on the active task. When blocking, prompt for an optional reason via a minimal text input overlay. When unblocking, clear both `blocked` and `block_reason` immediately.

### Key: `B` (Shift+B)
Currently `B` toggles the `@blocked` search filter. Reassign the filter toggle to a different key or make `B` context-aware:
- **No active task**: toggle `@blocked` filter (current behaviour)  
- **Active task selected**: toggle blocked flag on that task

Alternative: use a different key for the toggle (e.g. `!`) and let `B` always mean "block/unblock active task". Decide during implementation.

### Steps

1. **Add `BlockReason` overlay state** (`src/tui/types.rs`)
   - New `AppView::BlockReason` variant
   - Add `block_reason_input: String` field to `App` (or a small sub-state)

2. **Add key handler** (`src/tui/keys/board.rs` + `src/tui/keys/detail.rs`)
   - `B` on active task:
     - If task is **not blocked** → open `BlockReason` overlay (text input for reason, Enter to confirm, Esc to cancel, empty reason is valid)
     - If task **is blocked** → immediately unblock (set `blocked = false`, clear `block_reason`), persist, show status "Unblocked #N"
   - Guarded: only when `active_task().is_some()` and not in file-reader mode

3. **Add overlay key handler** (`src/tui/keys/overlays.rs`)
   - New `handle_block_reason_key()`:
     - `Enter` → set `task.blocked = true`, `task.block_reason = input`, `task.updated = now`, persist, return to previous view, show status "Blocked #N: reason"
     - `Esc` → cancel, return to previous view
     - `Backspace` / `Ctrl+W` / `Ctrl+U` → edit input
     - `Char(c)` → append to input

4. **Add overlay rendering** (`src/tui/render/overlays.rs`)
   - Small centered dialog: "Block reason (optional):" with text input and Enter/Esc hints
   - Reuse existing `centered_rect` + dialog block pattern from `ConfirmDelete`

5. **Wire into dispatch** (`src/tui/app.rs`)
   - Add `AppView::BlockReason` to `handle_key()` match
   - Add `block_reason_input` field initialised to `String::new()`

6. **Update help overlay** (`src/tui/render/overlays.rs`)
   - Add `("B", "Block/unblock task", ...)` to the help entries table

7. **Update guide** (`src/tui/guide/pages/dependencies.md`)
   - Document `B` key for blocking/unblocking in the TUI section

### Files touched
- `src/tui/types.rs` — new `AppView` variant
- `src/tui/app.rs` — new field, dispatch
- `src/tui/keys/board.rs` — `B` handler change
- `src/tui/keys/detail.rs` — `B` handler (mirror board)
- `src/tui/keys/overlays.rs` — new `handle_block_reason_key()`
- `src/tui/render/overlays.rs` — new overlay render + help entry
- `src/tui/guide/pages/dependencies.md` — document key
- `src/tui/persistence.rs` — no changes (existing `persist_task` handles it)

### Design notes
- Unblock is instant (no confirmation needed — reversible via `B` again or undo)
- Block prompts for reason because the reason is useful context; empty reason is valid (just press Enter)
- Undo support comes for free via the existing undo system if `persist_task` records undo entries
- Card border already shows red for blocked tasks — no rendering changes needed
