//! TUI type definitions, constants, and standalone helper functions.
//!
//! All types used by the TUI subsystem live here. They are re-exported
//! from `app.rs` so existing `use crate::tui::app::{...}` paths work
//! unchanged.

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Instant;

use chrono::Utc;
use ratatui::style::Color;

use super::theme::{self, ThemeKind};
use crate::model::config::Config;
use crate::model::task::Task;

// ── Constants ────────────────────────────────────────────────────────

/// Maximum entries retained in input history (search, find, etc.).
const HISTORY_MAX_ENTRIES: usize = 100;

/// Quantization multiplier for floating-point theme values (brightness,
/// saturation) when used as cache keys.  Converts `f32` to `i32` with
/// ~0.001 precision so cache hits tolerate tiny floating-point drift.
pub const THEME_QUANTIZE: f32 = 1000.0;

// ── Utility functions ────────────────────────────────────────────────

/// Delete the last word from a string (Alt+Backspace / Ctrl+W behavior).
/// Trims trailing whitespace first, then removes back to the previous
/// word boundary.
pub fn delete_word_back(s: &mut String) {
    // Trim trailing spaces.
    while s.ends_with(' ') {
        s.pop();
    }
    // Remove back to the next space (or beginning).
    while !s.is_empty() && !s.ends_with(' ') {
        s.pop();
    }
}

// ── Semantic search result types ─────────────────────────────────────

/// Result from an async board-level semantic search.
pub struct SemSearchResult {
    /// The query that produced these results (to discard stale responses).
    pub query: String,
    /// Similarity scores keyed by task ID (0.0–1.0).
    pub scores: HashMap<i32, f32>,
    /// Error message if the search failed.
    pub error: Option<String>,
}

/// Result from an async detail-level semantic find.
pub struct SemFindResult {
    /// The query that produced these results.
    pub query: String,
    /// Line indices in detail view that match semantically (via chunk metadata).
    pub line_indices: Vec<usize>,
    /// Error message if the search failed.
    pub error: Option<String>,
}

// ── View / mode enums ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppView {
    Board,
    Detail,
    MoveTask,
    Help,
    SearchHelp,
    ConfirmDelete,
    Search,
    CreateTask,
    Debug,
    BranchPicker,
    ContextPicker,
    ConfirmBranch,
}

// ── Context picker types ─────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextKind {
    Auto,    // "Auto-detect (current branch)"
    Clear,   // "Clear context" / "Clear branch"
    Task,    // Task with a branch
    Branch,  // Orphaned git branch (no task)
    New,     // "Create: <name>" for new branch
}

#[derive(Clone, Debug)]
pub struct ContextItem {
    pub kind: ContextKind,
    pub task_id: Option<i32>,
    pub branch: String,
    pub label: String,
    pub missing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextPickerMode {
    SwitchContext,  // C/W key -- filter the board
    AssignBranch,   // b key -- set task.branch
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Cards,
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    ByPriority,
    Newest,
    Oldest,
    CreatedNew,
    CreatedOld,
}

impl SortMode {
    pub fn next(&self) -> SortMode {
        match self {
            SortMode::ByPriority => SortMode::Newest,
            SortMode::Newest => SortMode::Oldest,
            SortMode::Oldest => SortMode::CreatedNew,
            SortMode::CreatedNew => SortMode::CreatedOld,
            SortMode::CreatedOld => SortMode::ByPriority,
        }
    }

    pub fn prev(&self) -> SortMode {
        match self {
            SortMode::ByPriority => SortMode::CreatedOld,
            SortMode::Newest => SortMode::ByPriority,
            SortMode::Oldest => SortMode::Newest,
            SortMode::CreatedNew => SortMode::Oldest,
            SortMode::CreatedOld => SortMode::CreatedNew,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            SortMode::ByPriority => "priority",
            SortMode::Newest => "newest",
            SortMode::Oldest => "oldest",
            SortMode::CreatedNew => "created\u{2193}",
            SortMode::CreatedOld => "created\u{2191}",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeMode {
    Created,
    Updated,
}

impl TimeMode {
    pub fn next(&self) -> TimeMode {
        match self {
            TimeMode::Created => TimeMode::Updated,
            TimeMode::Updated => TimeMode::Created,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            TimeMode::Created => "created",
            TimeMode::Updated => "updated",
        }
    }
}

// ── Create wizard step ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateStep {
    Title,
    Body,
    Priority,
    Tags,
}

impl CreateStep {
    pub fn index(&self) -> usize {
        match self {
            CreateStep::Title => 0,
            CreateStep::Body => 1,
            CreateStep::Priority => 2,
            CreateStep::Tags => 3,
        }
    }

    pub fn count() -> usize {
        4
    }

    pub fn name(&self) -> &str {
        match self {
            CreateStep::Title => "Title",
            CreateStep::Body => "Body",
            CreateStep::Priority => "Priority",
            CreateStep::Tags => "Tags",
        }
    }

    pub fn next(&self) -> CreateStep {
        match self {
            CreateStep::Title => CreateStep::Body,
            CreateStep::Body => CreateStep::Priority,
            CreateStep::Priority => CreateStep::Tags,
            CreateStep::Tags => CreateStep::Tags,
        }
    }

    pub fn prev(&self) -> CreateStep {
        match self {
            CreateStep::Title => CreateStep::Title,
            CreateStep::Body => CreateStep::Title,
            CreateStep::Priority => CreateStep::Body,
            CreateStep::Tags => CreateStep::Priority,
        }
    }
}

// ── Input history ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct InputHistory {
    entries: Vec<String>,
    cursor: Option<usize>, // None = not browsing
    draft: String,         // saved current input when user starts browsing
    max_entries: usize,
    /// Optional file path for persistence between sessions.
    path: Option<std::path::PathBuf>,
}

impl InputHistory {
    /// Create a persistent history backed by a file. Loads existing entries
    /// from disk (silently ignores missing/unreadable files).
    pub fn with_path(path: std::path::PathBuf) -> Self {
        let mut h = Self {
            entries: Vec::new(),
            cursor: None,
            draft: String::new(),
            max_entries: HISTORY_MAX_ENTRIES,
            path: Some(path),
        };
        h.load();
        h
    }

    /// Add entry to history (deduplicate, move to end if exists).
    /// Persists to disk if a path is configured.
    pub fn push(&mut self, entry: &str) {
        if entry.trim().is_empty() {
            return;
        }
        self.entries.retain(|e| e != entry);
        self.entries.push(entry.to_string());
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
        self.cursor = None;
        self.save();
    }

    /// Navigate up (older). Save draft on first press.
    pub fn up(&mut self, current: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        match self.cursor {
            None => {
                self.draft = current.to_string();
                self.cursor = Some(self.entries.len() - 1);
            }
            Some(c) if c > 0 => {
                self.cursor = Some(c - 1);
            }
            _ => {}
        }
        self.entries.get(self.cursor?).map(|s| s.as_str())
    }

    /// Navigate down (newer). Restore draft at end.
    pub fn down(&mut self, _current: &str) -> Option<&str> {
        let c = self.cursor?;
        let next = c + 1;
        if next >= self.entries.len() {
            self.cursor = None;
            return Some(self.draft.as_str());
        }
        self.cursor = Some(next);
        self.entries.get(next).map(|s| s.as_str())
    }

    /// Reset browsing state.
    pub fn reset(&mut self) {
        self.cursor = None;
        self.draft.clear();
    }

    /// All entries (for populating suggestion lists).
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Get completions matching prefix.
    pub fn completions(&self, prefix: &str) -> Vec<&str> {
        if prefix.is_empty() {
            return vec![];
        }
        let p = prefix.to_lowercase();
        self.entries
            .iter()
            .filter(|e| e.to_lowercase().starts_with(&p))
            .map(|s| s.as_str())
            .collect()
    }

    /// Load entries from disk (newline-delimited plain text).
    fn load(&mut self) {
        let path = match &self.path {
            Some(p) => p,
            None => return,
        };
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                self.entries.push(trimmed.to_string());
            }
        }
        // Enforce max entries after loading.
        if self.entries.len() > self.max_entries {
            let excess = self.entries.len() - self.max_entries;
            self.entries.drain(..excess);
        }
    }

    /// Save entries to disk (newline-delimited plain text).
    fn save(&self) {
        let path = match &self.path {
            Some(p) => p,
            None => return,
        };
        let content: String = self.entries.join("\n") + "\n";
        let _ = std::fs::write(path, content);
    }
}

// ── Column ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub tasks: Vec<Task>,
    pub collapsed: bool,
    pub scroll_offset: usize,
    /// Per-column cursor position so switching columns preserves each column's
    /// selected task independently.
    pub active_row: usize,
}

impl Column {
    pub fn visible_task_count(&self) -> usize {
        self.tasks.len()
    }
}

// ── Task helper functions ────────────────────────────────────────────

/// Compute age in hours from the task's `updated` timestamp.
pub fn task_age_hours(task: &Task) -> i64 {
    (Utc::now() - task.updated).num_hours()
}

/// Compute age in hours from the task's `created` timestamp.
pub fn task_created_age_hours(task: &Task) -> i64 {
    (Utc::now() - task.created).num_hours()
}

/// Returns a freshness indicator dot and its color based on task age.
pub fn task_freshness_dot(task: &Task) -> (&'static str, Color) {
    let hours = task_age_hours(task);
    let color = if hours < 1 {
        Color::Indexed(34) // green — fresh, just updated
    } else if hours < 24 {
        Color::Indexed(172) // amber — cooling, updated today
    } else if hours < 72 {
        Color::Indexed(196) // red — going stale
    } else {
        Color::Indexed(241) // dim gray — cold, inactive
    };
    ("\u{25cf}", color)
}

/// Returns a human-readable age string like "2h", "3d", "1w".
pub fn task_age_display(task: &Task, time_mode: TimeMode) -> String {
    let hours = match time_mode {
        TimeMode::Updated => task_age_hours(task),
        TimeMode::Created => task_created_age_hours(task),
    };
    format_hours(hours)
}

fn format_hours(hours: i64) -> String {
    if hours < 1 {
        "<1h".to_string()
    } else if hours < 24 {
        format!("{}h", hours)
    } else if hours < 168 {
        format!("{}d", hours / 24)
    } else {
        format!("{}w", hours / 168)
    }
}

/// Returns a ratatui Style colored by the configured age thresholds.
/// Walks thresholds in reverse (longest first); first match wins.
pub fn age_style(cfg: &Config, task: &Task, time_mode: TimeMode) -> ratatui::style::Style {
    use ratatui::style::Style;
    use std::time::Duration as StdDuration;

    let hours = match time_mode {
        TimeMode::Updated => task_age_hours(task),
        TimeMode::Created => task_created_age_hours(task),
    };
    let task_dur = StdDuration::from_secs((hours.max(0) as u64) * 3600);
    let thresholds = cfg.age_thresholds_parsed();

    for (dur, color_str) in thresholds.iter().rev() {
        if task_dur >= *dur {
            if let Ok(n) = color_str.parse::<u8>() {
                return Style::default().fg(theme::adjusted(Color::Indexed(n)));
            }
        }
    }
    Style::default().fg(theme::adjusted(Color::Indexed(241))) // dim fallback
}

/// Returns a short priority label, padded to 4 chars.
pub fn priority_label(priority: &str) -> String {
    let label = match priority.to_lowercase().as_str() {
        "critical" => "crit",
        "high" => "high",
        "medium" => "med",
        "low" => "low",
        other => return format!("{:<4}", &other[..other.len().min(4)]),
    };
    format!("{:<4}", label)
}

/// Returns a sort key for priority strings.
/// critical=0, high=1, medium=2, low=3, unknown=99
pub fn priority_sort_key(priority: &str) -> u8 {
    match priority.to_lowercase().as_str() {
        "critical" => 0,
        "high" => 1,
        "medium" => 2,
        "low" => 3,
        _ => 99,
    }
}

/// Raise priority one level.
pub fn priority_raise(priority: &str) -> String {
    match priority.to_lowercase().as_str() {
        "low" => "medium".to_string(),
        "medium" => "high".to_string(),
        "high" => "critical".to_string(),
        "critical" => "critical".to_string(),
        other => other.to_string(),
    }
}

/// Lower priority one level.
pub fn priority_lower(priority: &str) -> String {
    match priority.to_lowercase().as_str() {
        "critical" => "high".to_string(),
        "high" => "medium".to_string(),
        "medium" => "low".to_string(),
        "low" => "low".to_string(),
        other => other.to_string(),
    }
}

// ── Create wizard state ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CreateState {
    pub step: CreateStep,
    pub title: String,
    pub body: String,
    pub priority_index: usize,
    pub tags: String,
    pub status: String,
    pub is_edit: bool,
    pub edit_id: i32,
}

impl Default for CreateState {
    fn default() -> Self {
        Self {
            step: CreateStep::Title,
            title: String::new(),
            body: String::new(),
            priority_index: 0,
            tags: String::new(),
            status: String::new(),
            is_edit: false,
            edit_id: 0,
        }
    }
}

// ── Sub-state structs ────────────────────────────────────────────────

/// Search and semantic search state.
pub struct SearchState {
    pub query: String,
    pub active: bool,
    pub history: InputHistory,
    pub tab_prefix: Option<String>,
    pub tab_idx: usize,
    pub sem_last_key: Option<Instant>,
    pub sem_pending: bool,
    pub sem_loading: bool,
    pub sem_error: Option<String>,
    pub sem_search_rx: Option<mpsc::Receiver<SemSearchResult>>,
    pub sem_scores: HashMap<i32, f32>,
    pub sem_find_rx: Option<mpsc::Receiver<SemFindResult>>,
}

/// Detail view and find-in-detail state.
///
/// Caches are `Option<...>` (no RefCell) — all mutation goes through
/// `&mut App` (key handlers and render).
pub struct DetailState {
    pub scroll: usize,
    pub(crate) cache: Option<DetailLinesCache>,
    pub(crate) heading_cache: Option<HeadingOffsetsCache>,
    pub find_query: String,
    pub find_active: bool,
    pub find_matches: Vec<usize>,
    pub find_current: usize,
    pub find_history: InputHistory,
    pub find_tab_prefix: Option<String>,
    pub find_tab_idx: usize,
    pub fold_level: usize,
}

/// Picker state for branch/context/move/delete dialogs.
pub struct PickerState {
    pub branch_list: Vec<String>,
    pub branch_cursor: usize,
    pub branch_filter: String,
    pub branch_worktree_only: bool,
    pub context_mode: bool,
    pub context_task_id: i32,
    pub context_label: String,
    pub context_items: Vec<ContextItem>,
    pub context_cursor: usize,
    pub context_filter: String,
    pub context_worktree_only: bool,
    pub context_picker_mode: ContextPickerMode,
    pub confirm_branch_name: String,
    pub pending_undo_before: Option<(i32, crate::board::undo::FileSnapshot)>,
    pub move_cursor: usize,
    pub move_filter: String,
    pub move_filter_active: bool,
    pub delete_cursor: usize,
}

/// Debug, performance, and rendering state.
///
/// Metrics fields are plain types (no `Cell`) — render takes `&mut App`.
pub struct DebugState {
    pub scroll: usize,
    pub dbg_build_ms: u128,
    pub dbg_render_ms: u128,
    pub dbg_lines: usize,
    pub dbg_vrows: usize,
    pub fps: f64,
    pub fps_last_frame: Instant,
    pub perf_mode: bool,
    pub needs_redraw: bool,
}

// ── Render caches ────────────────────────────────────────────────────

/// Bundle of pre-computed detail lines and visual-row offsets.
/// All per-frame scroll arithmetic uses the precomputed `vrow_offsets`,
/// avoiding O(n) Paragraph::line_count() scans on every render.
#[derive(Clone)]
pub struct DetailContent {
    pub lines: std::rc::Rc<Vec<ratatui::text::Line<'static>>>,
    /// `vrow_offsets[i]` = cumulative visual rows for lines `0..i`.
    /// Length is `lines.len() + 1`, with `vrow_offsets[0] = 0`.
    pub(crate) vrow_offsets: std::rc::Rc<Vec<usize>>,
}

impl DetailContent {
    /// Total visual rows in the entire document.
    pub fn total_vrows(&self) -> usize {
        self.vrow_offsets.last().copied().unwrap_or(0)
    }

    /// Visual-row offset of line `idx` (O(1) lookup).
    pub fn line_to_vrow(&self, idx: usize) -> usize {
        let clamped = idx.min(self.lines.len());
        self.vrow_offsets[clamped]
    }

    /// Line index containing the given visual row (O(log n) binary search).
    pub fn vrow_to_line(&self, vrow: usize) -> usize {
        // partition_point returns first index where offsets[i] > vrow,
        // subtract 1 to get the line that *contains* this vrow.
        let idx = self.vrow_offsets.partition_point(|&o| o <= vrow);
        idx.saturating_sub(1).min(self.lines.len().saturating_sub(1))
    }

    /// Return a cloned slice of lines around `scroll` and the local scroll
    /// offset within that slice.
    pub fn viewport_slice(
        &self,
        scroll: usize,
        viewport_h: u16,
    ) -> (Vec<ratatui::text::Line<'static>>, usize) {
        let (start, end) = self.viewport_range(scroll, viewport_h);
        let visible = self.lines[start..end].to_vec();
        let v_start = self.vrow_offsets[start];
        let local_scroll = scroll.saturating_sub(v_start);
        (visible, local_scroll)
    }

    /// Line range covering the viewport around `scroll`.
    pub fn viewport_range(&self, scroll: usize, viewport_h: u16) -> (usize, usize) {
        let scroll_line = self.vrow_to_line(scroll);
        let margin = (viewport_h as usize) * 2;
        let start = scroll_line.saturating_sub(margin);
        let end = (scroll_line + viewport_h as usize + margin).min(self.lines.len());
        (start, end)
    }
}

/// Cached output of `build_detail_lines` (metadata + rendered markdown body).
pub struct DetailLinesCache {
    pub task_id: u32,
    pub updated_epoch: i64,
    pub body: String,
    pub width: u16,
    pub theme: ThemeKind,
    pub brightness_q: i32,
    pub saturation_q: i32,
    pub fold_level: usize,
    pub lines: std::rc::Rc<Vec<ratatui::text::Line<'static>>>,
    /// Cumulative visual-row prefix sums.
    pub vrow_offsets: std::rc::Rc<Vec<usize>>,
}

/// Cached heading offsets computed from the markdown body.
pub struct HeadingOffsetsCache {
    pub task_id: u32,
    pub body: String,
    pub theme: ThemeKind,
    pub brightness_q: i32,
    pub saturation_q: i32,
    pub content_width: u16,
    pub fold_level: usize,
    pub offsets_any: Vec<usize>,
    pub offsets_l2: Vec<usize>,
    /// Plain text of each markdown body line (for find matching).
    pub body_line_texts: Vec<String>,
    /// Number of metadata lines before the body.
    pub meta_count: usize,
}
