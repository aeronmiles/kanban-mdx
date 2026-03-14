---
id: 76
title: 'Refactor: Decompose TUI god object and address codebase technical debt'
status: done
priority: high
created: 2026-03-13T07:50:37.478369Z
updated: 2026-03-13T12:08:24.332306Z
started: 2026-03-13T12:08:24.332306Z
completed: 2026-03-13T12:08:24.332306Z
tags:
    - layer-4
class: standard
---

# Refactor Plan: kanban-mdx Codebase

Reference: Task #75 (kanban-mdx Rust Codebase Evaluation Report)

This plan is ordered by dependency — later phases build on earlier ones. Each phase is independently shippable. All phases preserve the existing public API and TUI behavior (no user-facing changes).

---

## Phase 1: Extract TUI sub-state structs from App (CRITICAL)

**Problem:** \`App\` struct in \`tui/app.rs\` has 147 fields and 142 methods across 4,657 lines. Every new feature adds more fields. Testing is impossible — you must construct the entire App to test any behavior.

**Approach:** Extract cohesive field groups into sub-structs that App composes. App becomes a thin coordinator. No behavioral changes — just move fields and update access patterns (\`self.search.query\` instead of \`self.search_query\`).

### Sub-structs to extract:

**1a. \`SearchState\`** → new file \`tui/search_state.rs\`
Fields to move:
- \`search_query\`, \`search_active\`
- \`search_history\`, \`search_tab_prefix\`, \`search_tab_idx\`
- All \`sem_*\` fields (8 fields): \`sem_last_key\`, \`sem_pending\`, \`sem_loading\`, \`sem_error\`, \`sem_search_rx\`, \`sem_search_ids\`, \`sem_scores\`, \`sem_find_rx\`
Methods to move: \`clear_sem_state()\`, \`is_semantic_query()\`, \`tick_semantic_debounce()\`, \`fire_sem_board_search()\`, \`fire_sem_detail_find()\`

**1b. \`DetailState\`** → new file \`tui/detail_state.rs\`
Fields to move:
- \`detail_scroll\`, \`detail_cache\`, \`heading_cache\`
- \`find_query\`, \`find_active\`, \`find_matches\`, \`find_current\`
- \`find_history\`, \`find_tab_prefix\`, \`find_tab_idx\`
- \`_fold_level\`
Methods to move: \`recompute_find_matches()\`, \`fold_level()\`, viewport-related methods for detail view

**1c. \`PickerState\`** → new file \`tui/picker_state.rs\`
Fields to move:
- Branch picker: \`branch_list\`, \`branch_cursor\`, \`branch_filter\`, \`branch_worktree_only\`
- Context picker: \`context_mode\`, \`context_task_id\`, \`context_label\`, \`context_items\`, \`context_cursor\`, \`context_filter\`, \`context_worktree_only\`, \`context_picker_mode\`, \`confirm_branch_name\`, \`pending_undo_before\`
- Move dialog: \`move_cursor\`, \`move_filter\`, \`move_filter_active\`
- Delete dialog: \`delete_cursor\`
Methods to move: \`filtered_branches()\`, \`filtered_context_items()\`

**1d. \`DebugState\`** → inline in \`tui/app.rs\` (small enough)
Fields to move:
- \`debug_scroll\`, \`dbg_build_ms\`, \`dbg_render_ms\`, \`dbg_lines\`, \`dbg_vrows\`
- \`fps\`, \`fps_last_frame\`, \`perf_mode\`, \`needs_redraw\`
Methods to move: \`update_fps()\`

**Estimated reduction:** App drops from 147 to ~35 fields. Each sub-struct is independently testable.

### Execution:
1. Create each sub-struct with \`Default\` impl
2. Replace flat fields in App with composed structs: \`pub search: SearchState\`, etc.
3. Update all \`self.field\` → \`self.search.field\` etc. in app.rs
4. Update all \`app.field\` → \`app.search.field\` in ui.rs
5. Cargo check after each sub-struct extraction (incremental, always compiling)

---

## Phase 2: Split key handlers into per-view files

**Problem:** \`handle_board_key()\` is 510 lines, \`handle_detail_key()\` is 358 lines. All 20 handlers live in app.rs.

**Approach:** Move each \`handle_*_key()\` method into a file matching the view it handles.

### File split:
- \`tui/keys/board.rs\` — \`handle_board_key\` (510 lines), \`handle_board_mouse\` (31), \`handle_board_click\` 
- \`tui/keys/detail.rs\` — \`handle_detail_key\` (358 lines), \`handle_detail_mouse\` (14)
- \`tui/keys/search.rs\` — \`handle_search_key\` (130 lines)
- \`tui/keys/dialogs.rs\` — \`handle_move_task_key\` (91), \`handle_confirm_delete_key\` (27), \`handle_goto_key\` (58)
- \`tui/keys/pickers.rs\` — \`handle_branch_picker_key\` (50), \`handle_context_picker_key\` (50), \`handle_confirm_branch_key\` (13)
- \`tui/keys/create.rs\` — \`handle_create_key\` (50) + sub-handlers (title/body/priority/tags)
- \`tui/keys/overlays.rs\` — \`handle_help_key\` (60), \`handle_search_help_key\` (24), \`handle_debug_key\` (31)
- \`tui/keys/mod.rs\` — \`handle_key\` dispatch (the existing 33-line dispatcher stays)

**Pattern:** Each file contains \`impl App { fn handle_*_key(...) { ... } }\` — Rust allows impl blocks across files in the same crate.

**Estimated reduction:** app.rs drops from ~4,657 to ~2,400 lines. Each handler file is <200 lines and testable in isolation.

---

## Phase 3: Split ui.rs render functions into per-view files

**Problem:** 2,610 lines, 28 functions — already well-structured but hard to navigate.

### File split:
- \`tui/render/board.rs\` — \`render_board\`, \`render_columns\`, \`render_column\`, \`render_cards\`, \`render_card\`, \`render_list\`
- \`tui/render/detail.rs\` — \`render_detail\`, \`render_scrolled_content\`, \`highlight_find_in_line\`
- \`tui/render/dialogs.rs\` — \`render_move_dialog\`, \`render_delete_confirm\`, \`render_create_dialog\`, \`render_goto_dialog\`
- \`tui/render/pickers.rs\` — \`render_branch_picker\`, \`render_context_picker\`, \`render_confirm_branch\`
- \`tui/render/overlays.rs\` — \`render_help\`, \`render_search_help\`, \`render_debug\`
- \`tui/render/chrome.rs\` — \`render_status_bar\`, \`render_search_bar\`, \`render_suggestions\`, \`render_reader_panel\`
- \`tui/render/layout.rs\` — \`centered_rect\`, \`centered_fixed\`, \`truncate\`, \`pad_right\`, \`card_height\`
- \`tui/render/mod.rs\` — re-exports + \`render()\` dispatcher

**These are free functions, not methods** — split is mechanical (cut + paste + use imports).

---

## Phase 4: Fix anti-patterns and type safety

**4a. Replace \`isize\` sentinel with \`Option<usize>\`**
- File: \`tui/app.rs\` — \`InputHistory::cursor: isize\` uses -1 as "not browsing"
- Change to \`cursor: Option<usize>\`, update \`up()\`/\`down()\` accordingly

**4b. Enum-type sort and group-by fields**
- Currently: \`sort_by: &str\` validated at runtime in \`board/sort.rs\`
- Create: \`enum SortField { Id, Status, Priority, Created, Updated, Due }\`
- Create: \`enum GroupField { Assignee, Tag, Class, Priority, Status }\`
- Implement \`FromStr\` for CLI parsing, use the enum throughout

**4c. Standardize error types**
- \`cli/edit.rs\` \`edit_one()\` returns \`Result<(), String>\` — change to \`Result<(), CliError>\`
- Audit all CLI commands for consistent \`CliError\` usage

**4d. Replace RefCell render caches with computed-on-demand pattern**
- \`detail_cache\` and \`heading_cache\` use \`RefCell<Option<...>>\` for interior mutability
- Instead: compute in \`&mut self\` methods and store directly (the App is always mutably borrowed when updating state)

---

## Phase 5: Dependency and config hygiene

**5a. Replace deprecated \`serde_yaml\`**
- \`serde_yaml = "0.9.34-deprecated"\` → migrate to \`serde_yml\` (drop-in for basic use)
- Test: round-trip all config and task file fixtures

**5b. Evaluate lighter HTTP client**
- \`reqwest\` (blocking) pulls in \`tokio\` just for a runtime
- Consider \`ureq\` — pure blocking, no async runtime, much lighter
- If switched, \`tokio\` can be dropped entirely from Cargo.toml

**5c. Add HTTP request timeouts to sembed**
- \`openai.rs\` and \`ollama.rs\` — add 30s default timeout on \`ClientBuilder\`
- Make configurable via \`OpenAIConfig\` / \`OllamaConfig\`

**5d. Replace \`fs2\` with \`fd-lock\`**
- \`fs2\` last released 2016, no maintenance
- \`fd-lock\` is actively maintained, same API surface

**5e. Extract magic numbers to named constants**
- \`MAX_SLUG_LENGTH = 50\`, \`LOG_MAX_LINES = 10_000\`, \`UNDO_MAX_ENTRIES = 100\`
- \`HIGHLIGHT_CACHE_MAX = 256\`, \`FS_DEBOUNCE_MS = 100\`, \`STATUS_EXPIRY_MS = 2000\`
- Each constant lives at the top of its module

---

## Phase 6: Single-pass find_by_id optimization

**Problem:** \`model/task.rs\` \`find_by_id()\` scans the directory twice — once by filename prefix, once by reading all YAML frontmatter.

**Approach:**
- Single iteration: check filename prefix first (O(1) string op), if ambiguous read frontmatter
- Early exit on exact filename match (most common case)
- Estimated speedup: 2x for boards with >100 tasks

---

## Phase 7: TUI test infrastructure

**Problem:** 7,267 lines of TUI code (app.rs + ui.rs) with zero tests.

**7a. Behavioral tests for key handlers**
- Create \`tui/tests/\` module
- Helper: \`fn test_app(tasks: Vec<Task>) -> App\` with default config
- Helper: \`fn send_key(app: &mut App, key: KeyCode)\`
- Test patterns:
  - Navigation: j/k/h/l move cursor, Enter opens detail, Esc returns
  - Search: / opens search, typing filters, Enter applies
  - Dialogs: m opens move, d opens delete, confirmations work
  - State transitions: every AppView variant reachable and dismissible

**7b. Snapshot tests for render functions**
- Use ratatui \`TestBackend\` with fixed dimensions
- Golden file comparison for board, detail, help, search views
- Run with \`--update\` flag to regenerate

---

## Ordering & Dependencies

\`\`\`
Phase 1 (sub-structs) ← must come first, everything builds on this
  ↓
Phase 2 (key handlers split) ← requires Phase 1 field paths
  ↓
Phase 3 (render split) ← independent of Phase 2, depends on Phase 1 field paths
  ↓
Phase 4 (type safety) ← can start after Phase 1
  ↓
Phase 5 (deps/config) ← independent, can run in parallel with Phase 2-4
  ↓
Phase 6 (find_by_id) ← independent, any time
  ↓
Phase 7 (tests) ← should come after Phase 1-3 so tests target the final structure
\`\`\`

Phases 5 and 6 are fully independent and can be done any time. Phase 7 ideally waits until the file structure stabilizes after Phases 1-3.

Phases 1-3, 4b, 5c, 6 complete. app.rs: 4657 to 2879 lines. 420 tests pass.
