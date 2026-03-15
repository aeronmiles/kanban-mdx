---
id: 86
title: Open local markdown files in the full detail reader view
status: done
priority: high
created: '2026-03-14T16:46:33.038060Z'
updated: '2026-03-14T17:45:48.356262Z'
tags:
- feature
- tui
- cli
---

## Context

The TUI detail view has a rich markdown rendering pipeline (syntax highlighting, heading folding, find-in-detail, theme switching) that currently only works for kanban tasks. This feature exposes that same reader for arbitrary local `.md` files via:

1. **CLI command** `kbmdx read <path>` — standalone reader, no board required
2. **TUI file picker** (`O` from board) — browse directories and open `.md` files
3. **TUI path input** (`/` inside file picker) — type/paste a path with tab-completion

---

## Part A: Core File View Infrastructure

### A1. `src/tui/types.rs` — New types

```rust
pub struct FileView {
    pub path: String,      // absolute path
    pub title: String,     // filename for header display
    pub body: String,      // raw markdown content
    pub standalone: bool,  // true = CLI read (q quits), false = TUI picker (q returns to board)
}

pub struct FilePickerState {
    pub cwd: std::path::PathBuf,        // current directory
    pub entries: Vec<FilePickerEntry>,   // dir entries (dirs first, then .md files)
    pub cursor: usize,                  // selected entry
    pub filter: String,                 // type-to-filter
    pub path_input_active: bool,        // path input sub-mode
    pub path_input: String,             // path text buffer
    pub tab_completions: Vec<String>,   // tab-completion candidates
    pub tab_idx: usize,
    pub tab_prefix: Option<String>,
}

pub struct FilePickerEntry {
    pub name: String,                   // "dirname/" or "file.md"
    pub path: std::path::PathBuf,
    pub is_dir: bool,
}
```

Add `InputHistory::new_ephemeral()` (in-memory, no file persistence).
Add `FilePicker` to the `AppView` enum.

### A2. `src/tui/app.rs` — New fields + constructors

- Add `pub file_view: Option<FileView>` and `pub file_picker: FilePickerState` to `App`
- Initialize both in `App::new()` (file_view: None, file_picker with empty state)
- Add `pub fn new_file_reader(path, title, body) -> Self` — `Config::new_default("reader")`, empty columns, `view: Detail`, `file_view: Some(FileView { standalone: true, .. })`, ephemeral histories, `JumpList::new(100)`
- Add `pub fn is_file_reader(&self) -> bool`
- Add `AppView::FilePicker` arm to `handle_key` dispatch

### A3. `src/tui/render/detail.rs` — File rendering

- `build_file_lines(app, body, width) -> DetailContent` — like `build_detail_lines` but no metadata grid, just markdown body. Cache key uses `task_id=0`.
- `render_file_detail(app, frame)` — header: filename + path; footer: applicable keys only (no move/goto); reuses `render_scrolled_content`.
- Modify `render_detail()`: early-return to `render_file_detail` when `app.file_view.is_some()`.

### A4. `src/tui/render/mod.rs` — Dispatch + re-export

- Re-export `build_file_lines`
- Add `AppView::FilePicker` render arm: board + `render_file_picker` overlay

### A5. `src/tui/detail_nav.rs` — Adapt for file mode

- Add `heading_offsets_for_file(body, width)` — uses `build_file_lines`, `task_id=0`
- Update heading nav methods to check `self.file_view` first
- Update `recompute_find_matches` for file mode
- Update `anchor_scroll_across_fold` for file mode

### A6. `src/tui/keys/detail.rs` — Gate task-specific keys

- `q`/`Esc`/`Backspace`: if `standalone` → quit; if `!standalone` → clear file_view, return to Board
- `m`, `:`, `Ctrl+G`: skip when `is_file_reader()`
- `y`/`Y`/`o`: call file-specific variants

### A7. `src/tui/actions.rs` — File clipboard/editor

Add `copy_file_content()`, `copy_file_path()`, `open_file_in_editor()`.

### A8. `src/tui/persistence.rs` — Guard no-config writes

Early return in persistence methods when `self.is_file_reader()`.

---

## Part B: CLI `kbmdx read` Command

### B1. `src/tui/mod.rs` — `run_tui_reader(path, title, body)`

Terminal setup → `App::new_file_reader()` → event loop. Watches the file for live reload.

### B2. `src/cli/read.rs` — New command (create file)

`ReadArgs { path: String }`. Resolves to absolute path, reads content, calls `run_tui_reader()`. No board/config required.

### B3. `src/cli/mod.rs` — `pub mod read;`

### B4. `src/cli/root.rs` — Register `Read` variant + dispatch

---

## Part C: TUI File Picker + Path Input

### C1. `src/tui/file_picker.rs` — Business logic (create file)

- `open_file_picker()` — sets cwd, scans dir, switches to `AppView::FilePicker`
- `scan_file_picker_dir()` — reads cwd, populates entries (dirs first, then .md files)
- `filtered_file_entries()` — filter by `file_picker.filter`
- `open_file_entry(entry)` — reads file, sets `file_view = Some(FileView { standalone: false })`, switches to `Detail`
- `compute_path_completions()` — tab-completion for path input

### C2. `src/tui/keys/file_picker.rs` — Key handler (create file)

**Directory browser mode** (`!path_input_active`):

| Key | Action |
|-----|--------|
| `Esc`/`q` | Return to Board |
| `j`/`Down` | Cursor down |
| `k`/`Up` | Cursor up |
| `Enter` | Dir → enter; file → open in reader |
| `Backspace` | Filter → pop char; empty → go up dir |
| `h`/`Left` | Go up one directory |
| `/` | Switch to path input mode |
| `g`/`Home` | Top |
| `G`/`End` | Bottom |
| Printable | Append to filter |

**Path input mode** (`path_input_active`):

| Key | Action |
|-----|--------|
| `Esc` | Cancel → browser mode |
| `Enter` | Open path |
| `Tab` | Tab-complete |
| `Backspace` | Pop char; empty → cancel |
| `Ctrl+W`/`Ctrl+U` | Delete word / clear line |
| Printable | Append |

### C3. `src/tui/render/pickers.rs` — `render_file_picker()`

Follows existing picker pattern (`centered_fixed` overlay, rounded border):
- Title: ` Open File `
- Current directory path, filter input, entry list (dirs in branch_style, files in title_inactive, cursor in list_active)
- In path input mode: path text with cursor
- Footer hints: `enter:open  h:up  /:path  esc:cancel`
- Dialog width: 60, max 18 visible entries

### C4. Wire up modules

- `src/tui/mod.rs` — `mod file_picker;`
- `src/tui/keys/mod.rs` — `mod file_picker;`
- `src/tui/keys/board.rs` — `KeyCode::Char('O') => self.open_file_picker()`

---

## Implementation Order

1. A1–A2: Types + App fields (skeleton compiling)
2. A3–A4: File rendering + render dispatch
3. A5–A8: Navigation, key gating, actions, persistence guards
4. B1–B4: CLI read command + run_tui_reader
5. C1–C4: File picker + path input + board keybinding

## Key Design Decisions

- **`task_id=0` cache key**: Safe — real task IDs start at 1
- **`standalone: bool` on FileView**: Controls q/Esc (quit vs return to board)
- **No new `AppView::FileView`**: Reuses `Detail`, branching on `file_view.is_some()`
- **New `AppView::FilePicker`**: Overlay on board (like branch/context pickers)
- **Persistence no-ops**: `is_file_reader()` guard prevents config writes without kanban dir

## Verification

1. `cargo build` — compiles
2. `cargo run -- read README.md` — standalone reader
3. `cargo run -- read nonexistent.md` — clean error
4. `cargo run -- tui` → `O` → browse → select .md → detail → `q` → board
5. `cargo run -- tui` → `O` → `/` → type path → Tab → Enter → detail
6. In file view: scroll, find, fold, heading nav, theme, yank, open editor
7. Live reload (CLI read): edit file → TUI updates
8. Existing board + task detail unchanged
