//! TUI application state — struct definition, construction, core methods,
//! and key/mouse dispatch.
//!
//! Methods are split across domain-specific files (board_nav, context,
//! detail_nav, semantic, persistence, actions) using `impl App` blocks.

use std::collections::HashMap;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};

use super::jump::JumpList;
use super::theme::{ThemeKind, ThemedStyleSheet};
use crate::model::config::Config;
use crate::model::task::Task;

// Re-export all types so existing `use crate::tui::app::{...}` paths work.
pub use super::jump::{JumpEntry, JumpList as JumpListType};
pub use super::types::*;

// ── App ──────────────────────────────────────────────────────────────

pub struct App {
    pub columns: Vec<Column>,
    pub active_col: usize,
    pub active_row: usize,
    pub view: AppView,
    pub view_mode: ViewMode,
    pub sort_mode: SortMode,
    pub time_mode: TimeMode,
    pub should_quit: bool,
    pub status_message: String,
    /// When the status message was set (for auto-clear).
    pub status_message_at: Option<Instant>,
    pub cfg: Config,
    pub reader_open: bool,
    pub reader_scroll: usize,
    pub create_state: CreateState,
    pub terminal_width: u16,
    pub terminal_height: u16,
    pub reader_max_width: u16,
    pub reader_width_pct: u16,
    /// Whether the go-to-task overlay is active (#34).
    pub goto_active: bool,
    /// Input buffer for the go-to-task dialog (#34).
    pub goto_input: String,
    /// Active markdown theme for the reader/detail views.
    pub theme_kind: ThemeKind,
    /// Brightness adjustment (-1.0 to 1.0, default 0.0).
    pub brightness: f32,
    /// Saturation adjustment (-1.0 to 1.0, default 0.0).
    pub saturation: f32,
    /// Scroll offset for help overlay (#51).
    pub help_scroll: usize,
    /// Filter string for help overlay (#51).
    pub help_filter: String,
    /// Whether help filter input is active (#51).
    pub help_filter_active: bool,
    /// Scroll offset for search DSL help overlay.
    pub search_help_scroll: usize,
    /// View to return to when closing search help overlay.
    pub search_help_return: AppView,
    /// Board-level toggle: when true, only show tasks with a worktree set (#57).
    pub worktree_filter_active: bool,
    /// Whether to hide columns with no visible tasks.
    pub hide_empty_columns: bool,
    /// When true, mouse capture is disabled so the terminal handles native
    /// text selection. Toggled via Ctrl+S.
    pub select_mode: bool,

    // ── Composed sub-state structs ──────────────────────────────────
    pub search: SearchState,
    pub detail: DetailState,
    pub picker: PickerState,
    pub debug: DebugState,
    pub guide: GuideState,

    /// Loaded markdown file for the file reader view.
    pub file_view: Option<FileView>,
    /// File picker state for browsing directories.
    pub file_picker: FilePickerState,

    /// Input buffer for the block-reason overlay.
    pub block_reason_input: String,
    /// View to return to after block-reason overlay is dismissed.
    pub block_return_view: AppView,

    /// Back/forward navigation history (detail-view context transitions only).
    pub jump_list: JumpList,
}

impl App {
    /// Construct a new App from a loaded Config and a list of tasks.
    /// Columns are built from config statuses; tasks are distributed to
    /// matching columns. Tasks whose status does not match any board column
    /// are silently dropped.
    pub fn new(cfg: Config, tasks: Vec<Task>) -> Self {
        let columns = Self::build_columns(&cfg, tasks);

        let view_mode = if cfg.tui.list_mode {
            ViewMode::List
        } else {
            ViewMode::Cards
        };

        let sort_mode = match cfg.tui.sort_mode {
            1 => SortMode::Newest,
            2 => SortMode::Oldest,
            3 => SortMode::CreatedNew,
            4 => SortMode::CreatedOld,
            _ => SortMode::ByPriority,
        };

        let time_mode = match cfg.tui.time_mode {
            1 => TimeMode::Updated,
            _ => TimeMode::Created,
        };

        let default_priority_idx = cfg
            .priorities
            .iter()
            .position(|p| p == &cfg.defaults.priority)
            .unwrap_or(0);

        let theme_kind = ThemeKind::from_config_str(&cfg.tui.theme);

        let reader_max_width = cfg.reader_max_width().max(30) as u16;
        let reader_width_pct = cfg.reader_width_pct().clamp(10, 90) as u16;
        let brightness = cfg.tui.brightness;
        let saturation = cfg.tui.saturation;
        let hide_empty_columns = cfg.tui.hide_empty_columns;

        // Build persistent history files in the kanban directory.
        let search_history = InputHistory::with_path(cfg.dir().join("search_history"));
        let find_history = InputHistory::with_path(cfg.dir().join("find_history"));
        let jump_list = JumpList::with_path(cfg.dir().join("jump_history"), 100);

        let mut app = Self {
            columns,
            active_col: 0,
            active_row: 0,
            view: AppView::Board,
            view_mode,
            sort_mode,
            time_mode,
            should_quit: false,
            status_message: String::new(),
            status_message_at: None,
            cfg,
            reader_open: false,
            reader_scroll: 0,
            create_state: CreateState {
                priority_index: default_priority_idx,
                ..Default::default()
            },
            terminal_width: 80,
            terminal_height: 24,
            reader_max_width,
            reader_width_pct,
            goto_active: false,
            goto_input: String::new(),
            theme_kind,
            brightness,
            saturation,
            help_scroll: 0,
            help_filter: String::new(),
            help_filter_active: false,
            search_help_scroll: 0,
            search_help_return: AppView::Board,
            worktree_filter_active: false,
            hide_empty_columns,
            select_mode: false,
            search: SearchState {
                query: String::new(),
                active: false,
                history: search_history,
                tab_prefix: None,
                tab_idx: 0,
                sem_last_key: None,
                sem_pending: false,
                sem_loading: false,
                sem_error: None,
                sem_search_rx: None,
                sem_scores: HashMap::new(),
                sem_find_rx: None,
            },
            detail: DetailState {
                scroll: 0,
                cache: None,
                heading_cache: None,
                find_query: String::new(),
                find_active: false,
                find_matches: Vec::new(),
                find_current: 0,
                find_history,
                find_tab_prefix: None,
                find_tab_idx: 0,
                fold_level: 0,
            },
            picker: PickerState {
                branch_list: Vec::new(),
                branch_cursor: 0,
                branch_filter: String::new(),
                branch_worktree_only: false,
                context_mode: false,
                context_task_id: 0,
                context_label: String::new(),
                context_items: Vec::new(),
                context_cursor: 0,
                context_filter: String::new(),
                context_worktree_only: false,
                context_picker_mode: ContextPickerMode::SwitchContext,
                confirm_branch_name: String::new(),
                pending_undo_before: None,
                move_cursor: 0,
                move_filter: String::new(),
                move_filter_active: false,
                delete_cursor: 1,
            },
            debug: DebugState {
                scroll: 0,
                dbg_build_ms: 0,
                dbg_render_ms: 0,
                dbg_lines: 0,
                dbg_vrows: 0,
                fps: 0.0,
                fps_last_frame: Instant::now(),
                perf_mode: true,
                needs_redraw: true,
            },
            file_view: None,
            file_picker: FilePickerState {
                cwd: std::path::PathBuf::new(),
                entries: Vec::new(),
                cursor: 0,
                filter: String::new(),
                path_input_active: false,
                path_input: String::new(),
                tab_completions: Vec::new(),
                tab_idx: 0,
                tab_prefix: None,
                return_view: AppView::Board,
            },
            guide: GuideState {
                mode: GuideMode::Index,
                topic_cursor: 0,
                topic_filter: String::new(),
                topic_filter_active: false,
                scroll: 0,
                cache: None,
                fold_level: 0,
                find_query: String::new(),
                find_active: false,
                find_matches: Vec::new(),
                find_current: 0,
            },
            block_reason_input: String::new(),
            block_return_view: AppView::Board,
            jump_list,
        };

        // Apply default collapsed columns from config.
        for col in &mut app.columns {
            if app.cfg.tui.collapsed_columns.contains(&col.name) {
                col.collapsed = true;
            }
        }

        // Find first non-collapsed column to be active.
        if let Some(first_expanded) = app.columns.iter().position(|c| !c.collapsed) {
            app.active_col = first_expanded;
        }

        app.sort_all_columns();
        app
    }

    /// Construct a standalone file reader (no board, no config directory).
    pub fn new_file_reader(path: String, title: String, body: String) -> Self {
        let cfg = Config::new_default("reader");
        let search_history = InputHistory::new_ephemeral();
        let find_history = InputHistory::new_ephemeral();
        let jump_list = JumpList::new(100);

        Self {
            columns: Vec::new(),
            active_col: 0,
            active_row: 0,
            view: AppView::Detail,
            view_mode: ViewMode::Cards,
            sort_mode: SortMode::ByPriority,
            time_mode: TimeMode::Created,
            should_quit: false,
            status_message: String::new(),
            status_message_at: None,
            cfg,
            reader_open: false,
            reader_scroll: 0,
            create_state: CreateState::default(),
            terminal_width: 80,
            terminal_height: 24,
            reader_max_width: 100,
            reader_width_pct: 50,
            goto_active: false,
            goto_input: String::new(),
            theme_kind: ThemeKind::Dark,
            brightness: 0.0,
            saturation: -0.2,
            help_scroll: 0,
            help_filter: String::new(),
            help_filter_active: false,
            search_help_scroll: 0,
            search_help_return: AppView::Board,
            worktree_filter_active: false,
            hide_empty_columns: false,
            select_mode: false,
            search: SearchState {
                query: String::new(),
                active: false,
                history: search_history,
                tab_prefix: None,
                tab_idx: 0,
                sem_last_key: None,
                sem_pending: false,
                sem_loading: false,
                sem_error: None,
                sem_search_rx: None,
                sem_scores: HashMap::new(),
                sem_find_rx: None,
            },
            detail: DetailState {
                scroll: 0,
                cache: None,
                heading_cache: None,
                find_query: String::new(),
                find_active: false,
                find_matches: Vec::new(),
                find_current: 0,
                find_history,
                find_tab_prefix: None,
                find_tab_idx: 0,
                fold_level: 0,
            },
            picker: PickerState {
                branch_list: Vec::new(),
                branch_cursor: 0,
                branch_filter: String::new(),
                branch_worktree_only: false,
                context_mode: false,
                context_task_id: 0,
                context_label: String::new(),
                context_items: Vec::new(),
                context_cursor: 0,
                context_filter: String::new(),
                context_worktree_only: false,
                context_picker_mode: ContextPickerMode::SwitchContext,
                confirm_branch_name: String::new(),
                pending_undo_before: None,
                move_cursor: 0,
                move_filter: String::new(),
                move_filter_active: false,
                delete_cursor: 1,
            },
            debug: DebugState {
                scroll: 0,
                dbg_build_ms: 0,
                dbg_render_ms: 0,
                dbg_lines: 0,
                dbg_vrows: 0,
                fps: 0.0,
                fps_last_frame: Instant::now(),
                perf_mode: true,
                needs_redraw: true,
            },
            file_view: Some(FileView {
                path,
                title,
                body,
                standalone: true,
            }),
            file_picker: FilePickerState {
                cwd: std::path::PathBuf::new(),
                entries: Vec::new(),
                cursor: 0,
                filter: String::new(),
                path_input_active: false,
                path_input: String::new(),
                tab_completions: Vec::new(),
                tab_idx: 0,
                tab_prefix: None,
                return_view: AppView::Board,
            },
            guide: GuideState {
                mode: GuideMode::Index,
                topic_cursor: 0,
                topic_filter: String::new(),
                topic_filter_active: false,
                scroll: 0,
                cache: None,
                fold_level: 0,
                find_query: String::new(),
                find_active: false,
                find_matches: Vec::new(),
                find_current: 0,
            },
            block_reason_input: String::new(),
            block_return_view: AppView::Board,
            jump_list,
        }
    }

    /// Returns `true` when the app is in standalone file-reader mode.
    pub fn is_file_reader(&self) -> bool {
        self.file_view.is_some()
    }

    // ── Status / FPS ─────────────────────────────────────────────────

    /// Set the ephemeral status message (auto-clears after a timeout).
    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = msg.into();
        self.status_message_at = Some(Instant::now());
    }

    /// Clear a status message if it has been visible long enough.
    /// Returns `true` if a message was cleared (needs redraw).
    pub fn expire_status(&mut self, timeout: std::time::Duration) -> bool {
        if let Some(at) = self.status_message_at {
            if at.elapsed() >= timeout {
                self.status_message.clear();
                self.status_message_at = None;
                self.debug.needs_redraw = true;
                return true;
            }
        }
        false
    }

    /// Update the FPS counter (call once per rendered frame).
    pub fn update_fps(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.debug.fps_last_frame).as_secs_f64();
        self.debug.fps_last_frame = now;
        if dt > 0.0 {
            let instant_fps = 1.0 / dt;
            // Exponential moving average (α = 0.1 for smoothing).
            self.debug.fps = self.debug.fps * 0.9 + instant_fps * 0.1;
        }
    }

    // ── Column building ──────────────────────────────────────────────

    /// Build columns from config board_statuses and distribute tasks.
    pub(crate) fn build_columns(cfg: &Config, tasks: Vec<Task>) -> Vec<Column> {
        let statuses = cfg.board_statuses();

        let mut columns: Vec<Column> = statuses
            .iter()
            .map(|s| Column {
                name: s.clone(),
                tasks: Vec::new(),
                collapsed: false,
                scroll_offset: 0,
                active_row: 0,
            })
            .collect();

        for task in tasks {
            if let Some(col) = columns.iter_mut().find(|c| c.name == task.status) {
                col.tasks.push(task);
            }
        }

        columns
    }

    /// Returns the active markdown style sheet based on the current theme.
    pub fn markdown_theme(&self) -> ThemedStyleSheet {
        ThemedStyleSheet(self.theme_kind)
    }

    pub fn active_task(&self) -> Option<&Task> {
        self.columns
            .get(self.active_col)
            .and_then(|col| col.tasks.get(self.active_row))
    }

    // ── Filtering ────────────────────────────────────────────────────

    pub fn filtered_tasks<'a>(
        col: &'a Column,
        query: &str,
        worktree_only: bool,
        sem_scores: &HashMap<i32, f32>,
        context_ids: &[i32],
        time_mode: &str,
    ) -> Vec<&'a Task> {
        let iter = col.tasks.iter();
        let base: Box<dyn Iterator<Item = &'a Task>> = if worktree_only {
            Box::new(iter.filter(|t| !t.worktree.is_empty()))
        } else {
            Box::new(iter)
        };

        // Apply context filter first.
        let context_filtered: Vec<&'a Task> = if !context_ids.is_empty() {
            base.filter(|t| context_ids.contains(&t.id)).collect()
        } else {
            base.collect()
        };

        if query.is_empty() {
            return context_filtered;
        }

        // Extract DSL portion (before ~) and check for semantic portion.
        let dsl_part = Self::dsl_query_text(query);
        let has_sem = Self::is_semantic_query(query);

        // Apply DSL filter first.
        let mut after_dsl: Vec<&'a Task> = if dsl_part.is_empty() {
            context_filtered
        } else {
            let filter = super::search::SearchFilter::parse(dsl_part);
            context_filtered.into_iter().filter(|t| filter.matches(t, time_mode)).collect()
        };

        // Sort by semantic score (descending) when semantic search is active.
        // Tasks without a score sort to the bottom.
        if has_sem && !sem_scores.is_empty() {
            after_dsl.sort_by(|a, b| {
                let sa = sem_scores.get(&a.id).copied().unwrap_or(-1.0);
                let sb = sem_scores.get(&b.id).copied().unwrap_or(-1.0);
                sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        after_dsl
    }

    /// Compute task IDs that should be visible under the current context filter.
    /// Returns an empty Vec when context mode is inactive (meaning show all).
    pub fn compute_context_ids(&self) -> Vec<i32> {
        if !self.picker.context_mode {
            return Vec::new();
        }

        let all_tasks: Vec<Task> = self
            .columns
            .iter()
            .flat_map(|c| c.tasks.iter().cloned())
            .collect();

        if self.picker.context_task_id > 0 {
            // Manual selection.
            let agent = std::env::var("KANBAN_AGENT").unwrap_or_default();
            crate::board::branch_context::expand_context(self.picker.context_task_id, &all_tasks, &agent)
        } else {
            // Auto-detect from current branch.
            if let Some(branch) = crate::util::git::current_branch() {
                if let Some(task) =
                    crate::board::branch_context::resolve_context_task(&branch, &all_tasks)
                {
                    let agent = std::env::var("KANBAN_AGENT").unwrap_or_default();
                    crate::board::branch_context::expand_context(task.id, &all_tasks, &agent)
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        }
    }

    // ── Reload ───────────────────────────────────────────────────────

    /// Reload tasks from disk, preserving cursor position.
    pub fn reload_tasks(&mut self) {
        self.debug.needs_redraw = true;
        let tasks_path = self.cfg.tasks_path();
        let (tasks, _) = match crate::model::task::read_all_lenient(&tasks_path) {
            Ok(result) => result,
            Err(_) => return,
        };

        // Remember the active task ID to restore cursor.
        let active_id = self.active_task().map(|t| t.id);

        self.columns = Self::build_columns(&self.cfg, tasks);

        // Re-apply collapsed state from config (persisted across sessions).
        for col in &mut self.columns {
            col.collapsed = self.cfg.tui.collapsed_columns.contains(&col.name);
        }

        self.sort_all_columns();

        // Restore cursor position.
        if let Some(id) = active_id {
            self.select_task_by_id(id);
        } else {
            self.clamp_active_row();
        }

        self.set_status("Board refreshed");
    }

    pub(crate) fn select_task_by_id(&mut self, id: i32) {
        for (col_idx, col) in self.columns.iter().enumerate() {
            if let Some(row_idx) = col.tasks.iter().position(|t| t.id == id) {
                self.active_col = col_idx;
                self.active_row = row_idx;
                return;
            }
        }
        self.clamp_active_row();
    }

    // ── Key / mouse dispatch ─────────────────────────────────────────

    pub fn handle_key(&mut self, key: KeyEvent) {
        self.debug.needs_redraw = true;

        // Global: Ctrl+C always quits.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        // Global toggle: F12 switches perf optimisations on/off for A/B testing.
        if key.code == KeyCode::F(12) {
            self.debug.perf_mode = !self.debug.perf_mode;
            let label = if self.debug.perf_mode { "ON" } else { "OFF" };
            self.set_status(format!("Perf mode: {}", label));
            return;
        }

        // Shift+Alt brackets: jump list navigation (global, any view).
        if key.modifiers.contains(KeyModifiers::ALT) && key.modifiers.contains(KeyModifiers::SHIFT) {
            match key.code {
                KeyCode::Char('[') | KeyCode::Char('{') => {
                    let snap = self.current_snapshot();
                    if let Some(entry) = self.jump_list.back(snap).cloned() {
                        self.restore_jump(entry);
                    }
                    return;
                }
                KeyCode::Char(']') | KeyCode::Char('}') => {
                    let snap = self.current_snapshot();
                    if let Some(entry) = self.jump_list.forward(snap).cloned() {
                        self.restore_jump(entry);
                    }
                    return;
                }
                _ => {}
            }
        }
        // macOS curly quote fallback for Shift+Alt variants.
        match key.code {
            KeyCode::Char('\u{201d}') => {
                let snap = self.current_snapshot();
                if let Some(entry) = self.jump_list.back(snap).cloned() {
                    self.restore_jump(entry);
                }
                return;
            }
            KeyCode::Char('\u{2019}') => {
                let snap = self.current_snapshot();
                if let Some(entry) = self.jump_list.forward(snap).cloned() {
                    self.restore_jump(entry);
                }
                return;
            }
            _ => {}
        }

        // Goto overlay intercepts all keys when active.
        if self.goto_active {
            self.handle_goto_key(key);
            return;
        }

        match self.view {
            AppView::Board => self.handle_board_key(key),
            AppView::MoveTask => self.handle_move_task_key(key),
            AppView::ConfirmDelete => self.handle_confirm_delete_key(key),
            AppView::Help => self.handle_help_key(key),
            AppView::SearchHelp => self.handle_search_help_key(key),
            AppView::Detail => self.handle_detail_key(key),
            AppView::Search => self.handle_search_key(key),
            AppView::CreateTask => self.handle_create_key(key),
            AppView::Debug => self.handle_debug_key(key),
            AppView::BranchPicker => self.handle_branch_picker_key(key),
            AppView::ContextPicker => self.handle_context_picker_key(key),
            AppView::ConfirmBranch => self.handle_confirm_branch_key(key),
            AppView::Guide => self.handle_guide_key(key),
            AppView::FilePicker => self.handle_file_picker_key(key),
            AppView::BlockReason => self.handle_block_reason_key(key),
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        use crossterm::event::MouseEventKind;

        if self.select_mode {
            return;
        }
        self.debug.needs_redraw = true;
        match self.view {
            AppView::Board | AppView::Search => self.handle_board_mouse(mouse),
            AppView::Detail => self.handle_detail_mouse(mouse),
            // Scrollable overlays — translate scroll wheel to offset changes.
            AppView::Help => match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.help_scroll = self.help_scroll.saturating_sub(3);
                }
                MouseEventKind::ScrollDown => {
                    self.help_scroll += 3;
                }
                _ => {}
            },
            AppView::SearchHelp => match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.search_help_scroll = self.search_help_scroll.saturating_sub(3);
                }
                MouseEventKind::ScrollDown => {
                    self.search_help_scroll += 3;
                }
                _ => {}
            },
            AppView::Debug => match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.debug.scroll = self.debug.scroll.saturating_sub(3);
                }
                MouseEventKind::ScrollDown => {
                    self.debug.scroll += 3;
                }
                _ => {}
            },
            AppView::Guide => match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.guide.scroll = self.guide.scroll.saturating_sub(3);
                }
                MouseEventKind::ScrollDown => {
                    self.guide.scroll += 3;
                }
                _ => {}
            },
            _ => {}
        }
    }

    // ── Layout ───────────────────────────────────────────────────────

    pub(crate) fn is_mouse_over_reader(&self, x: u16) -> bool {
        if !self.reader_open {
            return false;
        }
        let reader_start = self.board_width();
        x >= reader_start
    }

    /// Board width accounting for reader panel.
    pub fn board_width(&self) -> u16 {
        if !self.reader_open {
            self.terminal_width
        } else {
            let rw = self.reader_panel_width();
            self.terminal_width.saturating_sub(rw + 1) // +1 for separator
        }
    }

    /// Reader panel width as a percentage of terminal width.
    pub fn reader_panel_width(&self) -> u16 {
        let w = (self.terminal_width as u32 * self.reader_width_pct as u32 / 100) as u16;
        w.max(30).min(self.terminal_width.saturating_sub(20))
    }

    /// Effective content width inside the reader panel (after borders + padding).
    pub(crate) fn reader_content_width(&self) -> u16 {
        self.reader_panel_width().saturating_sub(3)
    }

    /// Effective content width for the detail view.
    pub(crate) fn detail_content_width(&self) -> u16 {
        if self.reader_max_width > 0 {
            (self.reader_max_width as u16).min(self.terminal_width)
        } else {
            self.terminal_width
        }
    }

    pub(crate) fn reader_page_size(&self) -> usize {
        (self.terminal_height as usize).saturating_sub(4).max(1)
    }

    pub(crate) fn detail_page_size(&self) -> usize {
        (self.terminal_height as usize).saturating_sub(4).max(1)
    }

    // ── Filter helpers ───────────────────────────────────────────────

    /// Returns indices of columns matching the move filter.
    pub fn filtered_columns(&self) -> Vec<usize> {
        if self.picker.move_filter.is_empty() {
            (0..self.columns.len()).collect()
        } else {
            let q = self.picker.move_filter.to_lowercase();
            self.columns
                .iter()
                .enumerate()
                .filter(|(_, col)| col.name.to_lowercase().contains(&q))
                .map(|(i, _)| i)
                .collect()
        }
    }

    pub(crate) fn open_guide(&mut self) {
        self.guide.mode = GuideMode::Index;
        self.guide.topic_cursor = 0;
        self.guide.topic_filter.clear();
        self.guide.topic_filter_active = false;
        self.guide.scroll = 0;
        self.guide.cache = None;
        self.guide.fold_level = 0;
        self.guide.find_query.clear();
        self.guide.find_active = false;
        self.guide.find_matches.clear();
        self.guide.find_current = 0;
        self.view = AppView::Guide;
    }

    pub(crate) fn guide_page_size(&self) -> usize {
        (self.terminal_height as usize).saturating_sub(4).max(1)
    }

    pub fn filtered_branches(&self) -> Vec<String> {
        if self.picker.branch_filter.is_empty() {
            self.picker.branch_list.clone()
        } else {
            let q = self.picker.branch_filter.to_lowercase();
            self.picker.branch_list
                .iter()
                .filter(|b| b.to_lowercase().contains(&q))
                .cloned()
                .collect()
        }
    }

}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
