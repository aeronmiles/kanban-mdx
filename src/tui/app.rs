//! TUI application state and key handling.
//!
//! Uses `crate::model::task::Task` and `crate::model::config::Config` for real
//! data models. Supports board view, detail view, move dialog, delete
//! confirmation, help overlay, search, create wizard, reader panel, and
//! file watching.

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};

use super::theme::{ThemeKind, ThemedStyleSheet};
use crate::model::config::Config;
use crate::model::task::Task;

/// Debounce delay for semantic search (reset on each keystroke).
const SEMANTIC_DEBOUNCE_MS: u64 = 300;

/// Maximum entries retained in input history (search, find, etc.).
const HISTORY_MAX_ENTRIES: usize = 100;

/// Quantization multiplier for floating-point theme values (brightness,
/// saturation) when used as cache keys.  Converts `f32` to `i32` with
/// ~0.001 precision so cache hits tolerate tiny floating-point drift.
pub(crate) const THEME_QUANTIZE: f32 = 1000.0;

/// Delete the last word from a string (Alt+Backspace / Ctrl+W behavior).
/// Trims trailing whitespace first, then removes back to the previous
/// word boundary.
pub(crate) fn delete_word_back(s: &mut String) {
    // Trim trailing spaces.
    while s.ends_with(' ') {
        s.pop();
    }
    // Remove back to the next space (or beginning).
    while !s.is_empty() && !s.ends_with(' ') {
        s.pop();
    }
}

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

// ── View / mode enums ─────────────────────────────────────────────────

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

// ── Context picker types ────────────────────────────────────────────

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

// ── Create wizard step ──────────────────────────────────────────────

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

// ── Input history ─────────────────────────────────────────────────────

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

// ── Column ────────────────────────────────────────────────────────────

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

// ── Task helper functions ─────────────────────────────────────────────

/// Compute age in hours from the task's `updated` timestamp.
pub fn task_age_hours(task: &Task) -> i64 {
    (Utc::now() - task.updated).num_hours()
}

/// Compute age in hours from the task's `created` timestamp.
pub fn task_created_age_hours(task: &Task) -> i64 {
    (Utc::now() - task.created).num_hours()
}

/// Returns a freshness indicator dot and its color based on task age.
pub fn task_freshness_dot(task: &Task) -> (&'static str, ratatui::style::Color) {
    use ratatui::style::Color;
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
    use ratatui::style::{Color, Style};
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
                return Style::default().fg(super::theme::adjusted(Color::Indexed(n)));
            }
        }
    }
    Style::default().fg(super::theme::adjusted(Color::Indexed(241))) // dim fallback
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

// ── Sub-state structs ─────────────────────────────────────────────────

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
pub struct DetailState {
    pub scroll: usize,
    pub(crate) cache: std::cell::RefCell<Option<DetailLinesCache>>,
    pub(crate) heading_cache: std::cell::RefCell<Option<HeadingOffsetsCache>>,
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
pub struct DebugState {
    pub scroll: usize,
    pub dbg_build_ms: std::cell::Cell<u128>,
    pub dbg_render_ms: std::cell::Cell<u128>,
    pub dbg_lines: std::cell::Cell<usize>,
    pub dbg_vrows: std::cell::Cell<usize>,
    pub fps: f64,
    pub fps_last_frame: Instant,
    pub perf_mode: bool,
    pub needs_redraw: bool,
}

// ── App ───────────────────────────────────────────────────────────────

// ---------------------------------------------------------------------------
// Render caches — avoid re-parsing markdown on every frame / key event
// ---------------------------------------------------------------------------

/// Bundle of pre-computed detail lines and visual-row offsets.
/// All per-frame scroll arithmetic uses the precomputed `vrow_offsets`,
/// avoiding O(n) Paragraph::line_count() scans on every render.
#[derive(Clone)]
pub(crate) struct DetailContent {
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
pub(crate) struct DetailLinesCache {
    pub task_id: u32,
    pub updated_epoch: i64,
    pub body: String,
    pub width: u16,
    pub theme: ThemeKind,
    pub brightness_q: i32,
    pub saturation_q: i32,
    pub fold_level: usize,
    pub lines: std::rc::Rc<Vec<ratatui::text::Line<'static>>>,
    /// Cumulative visual-row prefix sums: `vrow_offsets[i]` = total visual rows
    /// for lines `0..i`.  Length is `lines.len() + 1`, with `vrow_offsets[0] = 0`.
    /// Used for O(1) `line_to_vrow` and O(log n) `vrow_to_line` lookups.
    pub vrow_offsets: std::rc::Rc<Vec<usize>>,
}

/// Cached heading offsets computed from the markdown body.
pub(crate) struct HeadingOffsetsCache {
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

// ── Jump List ─────────────────────────────────────────────────────────

/// A snapshot of navigation state at a point in time.
#[derive(Debug, Clone)]
pub struct JumpEntry {
    pub view: AppView,
    pub task_id: Option<i32>,
    pub col: usize,
    pub row: usize,
    pub scroll: usize,
    pub fold_level: usize,
    /// Per-column collapsed state (parallel to `App::columns`).
    pub collapsed: Vec<bool>,
}

impl JumpEntry {
    /// Whether two entries represent the same navigation destination
    /// (same view + task_id, ignoring scroll/fold/layout state).
    fn same_destination(&self, other: &JumpEntry) -> bool {
        self.view == other.view && self.task_id == other.task_id
    }

    /// Serialize to a tab-delimited string.
    /// Format: `view\ttask_id\tcol\trow\tscroll\tfold\tcollapsed_csv`
    fn serialize(&self) -> String {
        let view_id = match self.view {
            AppView::Board => 0,
            AppView::Detail => 1,
            _ => 0,
        };
        let task_str = self
            .task_id
            .map(|id| id.to_string())
            .unwrap_or_default();
        let collapsed_csv: String = self
            .collapsed
            .iter()
            .map(|&b| if b { "1" } else { "0" })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            view_id, task_str, self.col, self.row, self.scroll, self.fold_level, collapsed_csv
        )
    }

    /// Deserialize from a tab-delimited string.
    fn deserialize(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('\t').collect();
        if parts.len() < 7 {
            return None;
        }
        let view = match parts[0].parse::<u8>().ok()? {
            0 => AppView::Board,
            1 => AppView::Detail,
            _ => return None,
        };
        let task_id = if parts[1].is_empty() {
            None
        } else {
            Some(parts[1].parse::<i32>().ok()?)
        };
        let col = parts[2].parse().ok()?;
        let row = parts[3].parse().ok()?;
        let scroll = parts[4].parse().ok()?;
        let fold_level = parts[5].parse().ok()?;
        let collapsed = if parts[6].is_empty() {
            Vec::new()
        } else {
            parts[6]
                .split(',')
                .map(|v| v == "1")
                .collect()
        };
        Some(Self {
            view,
            task_id,
            col,
            row,
            scroll,
            fold_level,
            collapsed,
        })
    }
}

/// Destination stack of significant navigation positions.
///
/// Only detail-view context transitions are recorded (exiting detail,
/// switching tasks via goto while in detail). Heading navigation and
/// board-level movements are local and do not push.
///
/// Entries are updated in-place when leaving via back/forward so they
/// always reflect the *final* position in that context, not the arrival.
pub struct JumpList {
    entries: Vec<JumpEntry>,
    /// Equal to `entries.len()` means "at the present / tip".
    cursor: usize,
    max_entries: usize,
    /// Optional file path for persistence between sessions.
    path: Option<std::path::PathBuf>,
}

impl JumpList {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            cursor: 0,
            max_entries,
            path: None,
        }
    }

    /// Create a persistent jump list backed by a file.
    pub fn with_path(path: std::path::PathBuf, max_entries: usize) -> Self {
        let mut jl = Self {
            entries: Vec::new(),
            cursor: 0,
            max_entries,
            path: Some(path),
        };
        jl.load();
        jl.cursor = jl.entries.len(); // start at tip
        jl
    }

    /// Push a new entry, truncating any forward history.
    ///
    /// Before inserting, collapses any trailing ping-pong pattern
    /// (e.g. `A,B,A,B,A` → `A`) so that "back" skips redundant bouncing
    /// and reaches the previous *distinct* destination directly.
    /// Also deduplicates any remaining earlier entry for the same
    /// destination as the new entry.
    pub fn push(&mut self, entry: JumpEntry) {
        self.entries.truncate(self.cursor);
        self.sanitize_pingpong();
        // Remove earlier entries for the same destination (view + task_id).
        self.entries.retain(|e| !e.same_destination(&entry));
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
        self.cursor = self.entries.len();
        self.save();
    }

    /// Collapse a trailing ping-pong pattern in the entries list.
    ///
    /// A ping-pong is 3+ consecutive entries that alternate between exactly
    /// two destinations: `[…, X, A, B, A, B, A]`.  The alternating run
    /// `A, B, A, B, A` is collapsed to its final entry `A`, so going back
    /// skips the redundant bouncing and reaches `X` directly.
    fn sanitize_pingpong(&mut self) {
        let len = self.entries.len();
        if len < 3 {
            return;
        }

        let last = len - 1;
        // The two candidate ping-pong destinations (Copy types).
        let a_view = self.entries[last].view;
        let a_tid = self.entries[last].task_id;
        let b_view = self.entries[last - 1].view;
        let b_tid = self.entries[last - 1].task_id;

        // Must be two distinct destinations to form a ping-pong.
        if a_view == b_view && a_tid == b_tid {
            return;
        }

        // Walk backwards checking if entries continue alternating a ↔ b.
        let mut start = last - 1;
        for i in (0..last - 1).rev() {
            let (ev, et) = if (last - i) % 2 == 0 {
                (a_view, a_tid)
            } else {
                (b_view, b_tid)
            };
            if self.entries[i].view == ev && self.entries[i].task_id == et {
                start = i;
            } else {
                break;
            }
        }

        if last - start + 1 >= 3 {
            // Drain everything except the final entry of the run.
            self.entries.drain(start..last);
        }
    }

    /// Update the entry at cursor in-place without forking forward history.
    /// At the tip (no entry at cursor), falls back to a regular `push`.
    ///
    /// Used when exiting a context (q/Esc) — preserves forward history so
    /// the user can continue navigating after reviewing a past position.
    pub fn update_in_place(&mut self, entry: JumpEntry) {
        if self.cursor < self.entries.len() {
            self.entries[self.cursor] = entry;
            self.save();
        } else {
            self.push(entry);
        }
    }

    /// Go back one entry.  Updates the entry being left with `current`
    /// so the destination reflects the final position in that context.
    pub fn back(&mut self, current: JumpEntry) -> Option<&JumpEntry> {
        if self.entries.is_empty() {
            return None;
        }
        if self.cursor == self.entries.len() {
            // At the tip — save current so forward() can return here.
            self.entries.push(current);
        } else {
            // Mid-history — update the entry we're leaving.
            self.entries[self.cursor] = current;
        }
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        self.save();
        self.entries.get(self.cursor)
    }

    /// Go forward one entry.  Updates the entry being left with `current`.
    pub fn forward(&mut self, current: JumpEntry) -> Option<&JumpEntry> {
        if self.cursor + 1 >= self.entries.len() {
            return None;
        }
        self.entries[self.cursor] = current;
        self.cursor += 1;
        self.save();
        self.entries.get(self.cursor)
    }

    /// Number of entries behind the cursor.
    pub fn back_count(&self) -> usize {
        self.cursor
    }

    /// Number of entries ahead of the cursor.
    pub fn forward_count(&self) -> usize {
        self.entries.len().saturating_sub(self.cursor + 1)
    }

    /// Whether the jump list has any history worth displaying.
    pub fn has_history(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Load entries from disk (tab-delimited, one entry per line).
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
            if trimmed.is_empty() {
                continue;
            }
            if let Some(entry) = JumpEntry::deserialize(trimmed) {
                self.entries.push(entry);
            }
        }
        if self.entries.len() > self.max_entries {
            let excess = self.entries.len() - self.max_entries;
            self.entries.drain(..excess);
        }
    }

    /// Save entries to disk (tab-delimited, one entry per line).
    fn save(&self) {
        let path = match &self.path {
            Some(p) => p,
            None => return,
        };
        let content: String = self
            .entries
            .iter()
            .map(|e| e.serialize())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let _ = std::fs::write(path, content);
    }
}

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
                cache: std::cell::RefCell::new(None),
                heading_cache: std::cell::RefCell::new(None),
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
                dbg_build_ms: std::cell::Cell::new(0),
                dbg_render_ms: std::cell::Cell::new(0),
                dbg_lines: std::cell::Cell::new(0),
                dbg_vrows: std::cell::Cell::new(0),
                fps: 0.0,
                fps_last_frame: Instant::now(),
                perf_mode: true,
                needs_redraw: true,
            },
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
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        if self.select_mode {
            return;
        }
        self.debug.needs_redraw = true;
        match self.view {
            AppView::Board => self.handle_board_mouse(mouse),
            AppView::Detail => self.handle_detail_mouse(mouse),
            _ => {}
        }
    }

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

    /// Reader panel width as a percentage of terminal width (configured via
    /// `tui.reader_width_pct`, adjusted with `<`/`>`). Clamped to at least 30
    /// and so the board keeps at least 20 columns.
    pub fn reader_panel_width(&self) -> u16 {
        let w = (self.terminal_width as u32 * self.reader_width_pct as u32 / 100) as u16;
        w.max(30).min(self.terminal_width.saturating_sub(20))
    }

    /// Effective content width inside the reader panel (after borders + padding).
    /// The actual reader rect is `reader_panel_width() + 1` because `board_width()`
    /// reserves 1 extra px for the panel's left border (visual separator).  The
    /// block has 2 borders + 2 padding = 4, so inner = panel_w + 1 − 4 = panel_w − 3.
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

    pub(crate) fn toggle_reader(&mut self) {
        if self.reader_open {
            self.reader_open = false;
            self.reader_scroll = 0;
        } else if self.terminal_width >= 60 {
            self.reader_open = true;
            self.reader_scroll = 0;
        } else {
            self.set_status("Terminal too narrow for reader panel");
        }
    }

    // ── MoveTask View ───────────────────────────────────────────────

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

    pub(crate) fn assign_branch_to_task(&mut self, branch: &str) {
        let mut msg = None;
        if let Some(task) = self
            .columns
            .get_mut(self.active_col)
            .and_then(|c| c.tasks.get_mut(self.active_row))
        {
            task.branch = branch.to_string();
            task.updated = Utc::now();
            msg = Some(format!("#{} branch: {}", task.id, branch));
        }
        if let Some(m) = msg {
            self.set_status(m);
        }
        self.persist_task(self.active_col, self.active_row);
    }

    pub(crate) fn clear_branch_from_task(&mut self) {
        let mut msg = None;
        if let Some(task) = self
            .columns
            .get_mut(self.active_col)
            .and_then(|c| c.tasks.get_mut(self.active_row))
        {
            task.branch.clear();
            task.updated = Utc::now();
            msg = Some(format!("#{} branch cleared", task.id));
        }
        if let Some(m) = msg {
            self.set_status(m);
        }
        self.persist_task(self.active_col, self.active_row);
    }

    // ── Context Picker ──────────────────────────────────────────────

    pub(crate) fn open_context_picker(&mut self, worktree_only: bool) {
        self.picker.context_worktree_only = worktree_only;
        self.picker.context_picker_mode = ContextPickerMode::SwitchContext;
        self.build_context_items();
        self.picker.context_cursor = 0;
        self.picker.context_filter.clear();
        self.view = AppView::ContextPicker;
    }

    pub(crate) fn open_assign_context(&mut self, worktree_only: bool) {
        self.picker.context_worktree_only = worktree_only;
        self.picker.context_picker_mode = ContextPickerMode::AssignBranch;
        self.build_context_items();
        self.picker.context_cursor = 0;
        self.picker.context_filter.clear();

        // Pre-select current branch.
        if let Some(task) = self.active_task() {
            let branch = task.branch.clone();
            if !branch.is_empty() {
                let filtered = self.filtered_context_items();
                for (i, item) in filtered.iter().enumerate() {
                    if item.branch == branch {
                        self.picker.context_cursor = i;
                        break;
                    }
                }
            }
        }
        self.view = AppView::ContextPicker;
    }

    pub(crate) fn build_context_items(&mut self) {
        let mut items = Vec::new();
        let git_branches = crate::util::git::local_branches();

        // Meta-options depend on picker mode.
        if self.picker.context_picker_mode == ContextPickerMode::SwitchContext {
            items.push(ContextItem {
                kind: ContextKind::Auto,
                task_id: None,
                branch: String::new(),
                label: "Auto-detect (current branch)".to_string(),
                missing: false,
            });
            items.push(ContextItem {
                kind: ContextKind::Clear,
                task_id: None,
                branch: String::new(),
                label: "Clear context".to_string(),
                missing: false,
            });
        } else {
            items.push(ContextItem {
                kind: ContextKind::Clear,
                task_id: None,
                branch: String::new(),
                label: "Clear branch".to_string(),
                missing: false,
            });
        }

        // Collect worktree branches if restricting to worktrees.
        let worktree_branches: std::collections::HashSet<String> =
            if self.picker.context_worktree_only {
                crate::util::git::list_worktree_branches()
                    .into_iter()
                    .collect()
            } else {
                std::collections::HashSet::new()
            };

        // Add task branches (deduped).
        let mut seen = std::collections::HashSet::new();
        let all_tasks: Vec<&Task> = self
            .columns
            .iter()
            .flat_map(|c| c.tasks.iter())
            .collect();
        for task in &all_tasks {
            if task.branch.is_empty() {
                continue;
            }
            if self.picker.context_worktree_only && !worktree_branches.contains(&task.branch) {
                continue;
            }
            if !seen.insert(task.branch.clone()) {
                continue;
            }
            let missing = !git_branches.contains(&task.branch);
            items.push(ContextItem {
                kind: ContextKind::Task,
                task_id: Some(task.id),
                branch: task.branch.clone(),
                label: format!("#{} {}", task.id, task.branch),
                missing,
            });
        }

        // Add orphaned git branches (branches with no task).
        let mut branch_list: Vec<String> = if self.picker.context_worktree_only {
            worktree_branches.iter().cloned().collect()
        } else {
            git_branches.iter().cloned().collect()
        };
        branch_list.sort();
        for branch in &branch_list {
            if !seen.insert(branch.clone()) {
                continue;
            }
            items.push(ContextItem {
                kind: ContextKind::Branch,
                task_id: None,
                branch: branch.clone(),
                label: branch.clone(),
                missing: false,
            });
        }

        self.picker.context_items = items;
    }

    pub fn filtered_context_items(&self) -> Vec<&ContextItem> {
        if self.picker.context_filter.is_empty() {
            self.picker.context_items.iter().collect()
        } else {
            let q = self.picker.context_filter.to_lowercase();
            self.picker.context_items
                .iter()
                .filter(|item| {
                    // Always show Auto and Clear.
                    matches!(item.kind, ContextKind::Auto | ContextKind::Clear)
                        || item.label.to_lowercase().contains(&q)
                        || item.branch.to_lowercase().contains(&q)
                })
                .collect()
        }
    }

    pub(crate) fn maybe_add_create_item(&mut self) {
        // Remove any existing New items.
        self.picker.context_items
            .retain(|item| item.kind != ContextKind::New);

        let filter = self.picker.context_filter.trim().to_string();
        if filter.is_empty() {
            return;
        }

        // Check if filter matches any existing branch exactly.
        let has_exact = self.picker.context_items.iter().any(|item| item.branch == filter);
        if !has_exact {
            self.picker.context_items.push(ContextItem {
                kind: ContextKind::New,
                task_id: None,
                branch: filter.clone(),
                label: format!("Create: {}", filter),
                missing: false,
            });
        }
    }

    pub(crate) fn execute_context_select(&mut self, item: &ContextItem) {
        match item.kind {
            ContextKind::Clear => {
                self.picker.context_mode = false;
                self.picker.context_task_id = 0;
                self.picker.context_label.clear();
                self.set_status("Context cleared".to_string());
                self.view = AppView::Board;
            }
            ContextKind::Auto => {
                self.picker.context_mode = true;
                self.picker.context_task_id = 0;
                self.picker.context_label.clear();
                self.set_status("Context: auto-detect".to_string());
                self.view = AppView::Board;
            }
            ContextKind::New => {
                self.picker.confirm_branch_name = item.branch.clone();
                self.view = AppView::ConfirmBranch;
            }
            ContextKind::Task | ContextKind::Branch => {
                self.picker.context_mode = true;
                self.picker.context_task_id = item.task_id.unwrap_or(0);
                self.picker.context_label = item.branch.clone();
                self.set_status(format!("Context: {}", item.branch));
                self.view = AppView::Board;
            }
        }
    }

    pub(crate) fn execute_assign_context(&mut self, item: &ContextItem) {
        match item.kind {
            ContextKind::Clear => {
                self.record_branch_undo();
                self.clear_branch_from_task();
                self.complete_branch_undo();
                self.view = AppView::Board;
            }
            ContextKind::New => {
                self.picker.confirm_branch_name = item.branch.clone();
                self.view = AppView::ConfirmBranch;
            }
            _ => {
                self.record_branch_undo();
                let branch = item.branch.clone();
                self.assign_branch_to_task(&branch);
                self.complete_branch_undo();
                self.view = AppView::Board;
            }
        }
    }

    pub(crate) fn record_branch_undo(&mut self) {
        if let Some(task) = self.active_task() {
            let path = &task.file;
            if path.is_empty() {
                return;
            }
            let before = crate::board::undo::snapshot_file(std::path::Path::new(path));
            self.picker.pending_undo_before = Some((task.id, before));
        }
    }

    pub(crate) fn complete_branch_undo(&mut self) {
        if let Some((task_id, before)) = self.picker.pending_undo_before.take() {
            if let Some(task) = self
                .columns
                .get(self.active_col)
                .and_then(|c| c.tasks.get(self.active_row))
            {
                let after =
                    crate::board::undo::snapshot_file(std::path::Path::new(&task.file));
                let detail = format!("branch -> {}", task.branch);
                let entry = crate::board::undo::UndoEntry {
                    timestamp: Utc::now(),
                    action: "branch-assign".to_string(),
                    task_id,
                    detail,
                    files_before: vec![before],
                    files_after: vec![after],
                };
                let _ = crate::board::undo::record_undo(self.cfg.dir(), &entry);
            }
        }
    }

    pub(crate) fn create_branch_and_proceed(&mut self) {
        let name = self.picker.confirm_branch_name.clone();
        let git_branches = crate::util::git::local_branches();
        let branch_exists = git_branches.contains(&name);

        if self.picker.context_worktree_only {
            // Create worktree.
            let path = format!("../kb-{}", name);
            let result = if branch_exists {
                std::process::Command::new("git")
                    .args(["worktree", "add", &path, &name])
                    .output()
            } else {
                std::process::Command::new("git")
                    .args(["worktree", "add", &path, "-b", &name])
                    .output()
            };
            match result {
                Ok(out) if out.status.success() => {
                    self.set_status(format!("Created worktree: {}", path));
                }
                Ok(out) => {
                    let err = String::from_utf8_lossy(&out.stderr);
                    self.set_status(format!("Worktree error: {}", err.trim()));
                    self.view = AppView::Board;
                    return;
                }
                Err(e) => {
                    self.set_status(format!("Git error: {}", e));
                    self.view = AppView::Board;
                    return;
                }
            }
        } else if !branch_exists {
            // Create branch only.
            let result = std::process::Command::new("git")
                .args(["branch", &name])
                .output();
            match result {
                Ok(out) if out.status.success() => {
                    self.set_status(format!("Created branch: {}", name));
                }
                Ok(out) => {
                    let err = String::from_utf8_lossy(&out.stderr);
                    self.set_status(format!("Git error: {}", err.trim()));
                    self.view = AppView::Board;
                    return;
                }
                Err(e) => {
                    self.set_status(format!("Git error: {}", e));
                    self.view = AppView::Board;
                    return;
                }
            }
        }

        // Now proceed with the original action.
        let item = ContextItem {
            kind: ContextKind::Branch,
            task_id: None,
            branch: name,
            label: String::new(),
            missing: false,
        };

        match self.picker.context_picker_mode {
            ContextPickerMode::SwitchContext => self.execute_context_select(&item),
            ContextPickerMode::AssignBranch => {
                self.record_branch_undo();
                let branch = item.branch.clone();
                self.assign_branch_to_task(&branch);
                self.complete_branch_undo();
                self.view = AppView::Board;
            }
        }
    }

    // ── Navigation helpers ──────────────────────────────────────────

    /// Save `active_row` into the current column, switch `active_col` to
    /// `new_col`, and restore that column's saved cursor.  Every column
    /// switch must go through this so per-column cursors stay in sync.
    pub(crate) fn switch_col(&mut self, new_col: usize) {
        // Save current cursor into the column we're leaving.
        if let Some(col) = self.columns.get_mut(self.active_col) {
            col.active_row = self.active_row;
        }
        self.active_col = new_col;
        // Restore saved cursor from the column we're entering.
        self.active_row = self
            .columns
            .get(self.active_col)
            .map(|c| c.active_row)
            .unwrap_or(0);
        self.clamp_active_row();
    }

    pub(crate) fn move_col_left(&mut self) {
        if self.columns.is_empty() {
            return;
        }
        let len = self.columns.len();
        let mut col = self.active_col;
        for _ in 0..len {
            col = if col == 0 { len - 1 } else { col - 1 };
            if !self.columns[col].collapsed {
                self.switch_col(col);
                return;
            }
        }
    }

    pub(crate) fn move_col_right(&mut self) {
        if self.columns.is_empty() {
            return;
        }
        let len = self.columns.len();
        let mut col = self.active_col;
        for _ in 0..len {
            col = if col + 1 >= len { 0 } else { col + 1 };
            if !self.columns[col].collapsed {
                self.switch_col(col);
                return;
            }
        }
    }

    /// When exactly one column is expanded (solo mode), cycle the solo to
    /// the previous column, preserving the layout. Otherwise normal nav.
    pub(crate) fn cycle_or_move_col_left(&mut self) {
        let expanded = self.columns.iter().filter(|c| !c.collapsed).count();
        if expanded == 1 {
            let len = self.columns.len();
            let prev = if self.active_col == 0 { len - 1 } else { self.active_col - 1 };
            for (i, col) in self.columns.iter_mut().enumerate() {
                col.collapsed = i != prev;
            }
            self.switch_col(prev);
            self.persist_collapsed();
        } else {
            self.move_col_left();
        }
    }

    /// When exactly one column is expanded (solo mode), cycle the solo to
    /// the next column, preserving the layout. Otherwise normal nav.
    pub(crate) fn cycle_or_move_col_right(&mut self) {
        let expanded = self.columns.iter().filter(|c| !c.collapsed).count();
        if expanded == 1 {
            let len = self.columns.len();
            let next = if self.active_col + 1 >= len { 0 } else { self.active_col + 1 };
            for (i, col) in self.columns.iter_mut().enumerate() {
                col.collapsed = i != next;
            }
            self.switch_col(next);
            self.persist_collapsed();
        } else {
            self.move_col_right();
        }
    }

    /// Returns indices (into `col.tasks`) of tasks visible under current
    /// filters, or `None` when no filter is active (all tasks visible).
    pub(crate) fn active_col_filtered_indices(&self) -> Option<Vec<usize>> {
        if self.search.query.is_empty() && !self.worktree_filter_active && !self.picker.context_mode {
            return None;
        }
        let col = self.columns.get(self.active_col)?;
        let context_ids = self.compute_context_ids();
        let filtered = Self::filtered_tasks(
            col,
            &self.search.query,
            self.worktree_filter_active,
            &self.search.sem_scores,
            &context_ids,
            self.time_mode.label(),
        );
        // Map filtered tasks back to col.tasks indices, preserving the
        // filtered order (which may differ from original — e.g. semantic
        // ranking).  Navigation must follow this order so that j/k moves
        // through tasks in the same sequence the user sees on screen.
        let indices: Vec<usize> = filtered
            .iter()
            .filter_map(|ft| col.tasks.iter().position(|t| t.id == ft.id))
            .collect();
        Some(indices)
    }

    pub(crate) fn move_row_down(&mut self, amount: usize) {
        if let Some(indices) = self.active_col_filtered_indices() {
            if indices.is_empty() {
                return;
            }
            let cur_pos = indices.iter().position(|&i| i == self.active_row);
            match cur_pos {
                Some(p) => {
                    self.active_row = indices[(p + amount).min(indices.len() - 1)];
                }
                None => {
                    // Not on a visible task — snap to next visible one downward.
                    let next = indices
                        .iter()
                        .position(|&i| i > self.active_row)
                        .unwrap_or(0);
                    self.active_row = indices[next];
                }
            }
            return;
        }
        let last = self.last_row_index();
        self.active_row = (self.active_row + amount).min(last);
    }

    pub(crate) fn move_row_up(&mut self, amount: usize) {
        if let Some(indices) = self.active_col_filtered_indices() {
            if indices.is_empty() {
                return;
            }
            let cur_pos = indices.iter().position(|&i| i == self.active_row);
            match cur_pos {
                Some(p) => {
                    self.active_row = indices[p.saturating_sub(amount)];
                }
                None => {
                    // Not on a visible task — snap to previous visible one.
                    let prev = indices
                        .iter()
                        .rposition(|&i| i < self.active_row)
                        .unwrap_or(indices.len() - 1);
                    self.active_row = indices[prev];
                }
            }
            return;
        }
        self.active_row = self.active_row.saturating_sub(amount);
    }

    pub(crate) fn last_row_index(&self) -> usize {
        if let Some(indices) = self.active_col_filtered_indices() {
            return indices.last().copied().unwrap_or(0);
        }
        self.columns
            .get(self.active_col)
            .map(|c| c.tasks.len().saturating_sub(1))
            .unwrap_or(0)
    }

    pub(crate) fn clamp_active_row(&mut self) {
        if let Some(indices) = self.active_col_filtered_indices() {
            if indices.is_empty() {
                self.active_row = 0;
                return;
            }
            if !indices.contains(&self.active_row) {
                // Snap to nearest visible task.
                self.active_row = *indices
                    .iter()
                    .min_by_key(|&&i| (i as isize - self.active_row as isize).unsigned_abs())
                    .unwrap();
            }
            return;
        }
        let last = self.last_row_index();
        if self.active_row > last {
            self.active_row = last;
        }
    }

    pub(crate) fn visible_height(&self) -> usize {
        if let Some(indices) = self.active_col_filtered_indices() {
            return indices.len().max(2);
        }
        self.columns
            .get(self.active_col)
            .map(|c| c.tasks.len())
            .unwrap_or(10)
            .max(2)
    }

    // ── Sort ────────────────────────────────────────────────────────

    pub(crate) fn sort_all_columns(&mut self) {
        for col in &mut self.columns {
            Self::sort_column(col, self.sort_mode);
        }
    }

    pub(crate) fn sort_column(col: &mut Column, mode: SortMode) {
        match mode {
            SortMode::ByPriority => {
                col.tasks.sort_by_key(|t| priority_sort_key(&t.priority));
            }
            SortMode::Newest => {
                col.tasks.sort_by_key(|t| task_age_hours(t));
            }
            SortMode::Oldest => {
                col.tasks
                    .sort_by(|a, b| task_age_hours(b).cmp(&task_age_hours(a)));
            }
            SortMode::CreatedNew => {
                col.tasks.sort_by_key(|t| task_created_age_hours(t));
            }
            SortMode::CreatedOld => {
                col.tasks
                    .sort_by(|a, b| task_created_age_hours(b).cmp(&task_created_age_hours(a)));
            }
        }
    }

    // ── Move / delete operations ────────────────────────────────────

    pub(crate) fn execute_move(&mut self, target_col: usize) {
        if target_col >= self.columns.len() || target_col == self.active_col {
            return;
        }
        let src = self.active_col;
        if self.active_row >= self.columns[src].tasks.len() {
            return;
        }

        let mut task = self.columns[src].tasks.remove(self.active_row);
        let id = task.id;
        let new_status = self.columns[target_col].name.clone();

        task.status.clone_from(&new_status);
        task.updated = Utc::now();

        self.columns[target_col].tasks.push(task);
        Self::sort_column(&mut self.columns[target_col], self.sort_mode);

        let new_row = self.columns[target_col]
            .tasks
            .iter()
            .position(|t| t.id == id)
            .unwrap_or(0);

        self.set_status(format!("Moved #{} to {}", id, new_status));
        self.active_col = target_col;
        self.active_row = new_row;

        self.persist_task(target_col, new_row);
    }

    pub(crate) fn execute_delete(&mut self) {
        let col_idx = self.active_col;
        let row_idx = self.active_row;
        let can_delete = self
            .columns
            .get(col_idx)
            .map_or(false, |col| row_idx < col.tasks.len());

        if !can_delete {
            return;
        }

        let mut task = self.columns[col_idx].tasks.remove(row_idx);
        let id = task.id;

        // Soft-delete: move to archived status.
        task.status = crate::model::config::ARCHIVED_STATUS.to_string();
        task.updated = Utc::now();
        self.persist_deleted_task(&task);

        self.set_status(format!("Deleted #{}", id));

        let col = &self.columns[col_idx];
        if !col.tasks.is_empty() {
            self.active_row = self.active_row.min(col.tasks.len() - 1);
        } else {
            self.active_row = 0;
        }
    }

    // ── Disk persistence ────────────────────────────────────────────

    pub(crate) fn persist_task(&mut self, col_idx: usize, row_idx: usize) {
        if let Some(task) = self.columns.get(col_idx).and_then(|c| c.tasks.get(row_idx)) {
            if task.file.is_empty() {
                return;
            }
            let path = std::path::Path::new(&task.file);
            if let Err(e) = crate::io::task_file::write(path, task) {
                self.set_status(format!("Save error: {}", e));
            }
        }
    }

    // ── Reader / detail helpers ──────────────────────────────────────

    pub(crate) fn reader_page_size(&self) -> usize {
        (self.terminal_height as usize).saturating_sub(4).max(1)
    }

    pub(crate) fn detail_page_size(&self) -> usize {
        (self.terminal_height as usize).saturating_sub(4).max(1)
    }

    pub(crate) fn toggle_collapse_idx(&mut self, idx: usize) {
        if idx < self.columns.len() {
            // Don't collapse the last visible column.
            if !self.columns[idx].collapsed {
                let expanded = self.columns.iter().filter(|c| !c.collapsed).count();
                if expanded <= 1 {
                    return;
                }
            }
            let new_state = !self.columns[idx].collapsed;
            self.columns[idx].collapsed = new_state;
            if new_state {
                if self.active_col == idx {
                    self.move_col_right();
                }
            } else {
                self.switch_col(idx);
            }
            self.persist_collapsed();
        }
    }

    /// Sync collapsed column state to config and save to disk.
    pub(crate) fn persist_collapsed(&mut self) {
        self.cfg.tui.collapsed_columns = self
            .columns
            .iter()
            .filter(|c| c.collapsed)
            .map(|c| c.name.clone())
            .collect();
        if let Err(e) = crate::io::config_file::save(&self.cfg) {
            self.set_status(format!("Config save error: {}", e));
        }
    }

    /// Sync sort_mode, time_mode, list_mode, theme, brightness, and saturation
    /// to config and save to disk.
    pub(crate) fn persist_tui_state(&mut self) {
        self.cfg.tui.sort_mode = match self.sort_mode {
            SortMode::ByPriority => 0,
            SortMode::Newest => 1,
            SortMode::Oldest => 2,
            SortMode::CreatedNew => 3,
            SortMode::CreatedOld => 4,
        };
        self.cfg.tui.time_mode = match self.time_mode {
            TimeMode::Created => 0,
            TimeMode::Updated => 1,
        };
        self.cfg.tui.list_mode = matches!(self.view_mode, ViewMode::List);
        self.cfg.tui.theme = self.theme_kind.as_config_str().to_string();
        self.cfg.tui.brightness = self.brightness;
        self.cfg.tui.saturation = self.saturation;
        if let Err(e) = crate::io::config_file::save(&self.cfg) {
            self.set_status(format!("Config save error: {}", e));
        }
    }

    /// Switch to board view and solo-focus a column by 0-based index.
    /// Used by digit shortcuts (`1`-`9`) from non-board views.
    pub(crate) fn focus_column_and_return(&mut self, idx: usize) {
        if idx < self.columns.len() {
            for (i, col) in self.columns.iter_mut().enumerate() {
                col.collapsed = i != idx;
            }
            self.switch_col(idx);
            self.view = AppView::Board;
        }
    }

    /// Return cached heading offsets, recomputing only when task/theme/width changes.
    ///
    /// Scans the actual rendered detail lines (from `build_detail_lines`) so
    /// heading positions always match what is displayed — no separate markdown
    /// parse, no fragile metadata-line counting.
    ///
    /// Offsets are returned as **visual row** positions (accounting for line
    /// wrapping at `content_width`) so they can be used directly as scroll
    /// values with `Paragraph::scroll()`.
    ///
    /// When `exact_level` is `None` (any heading), stacked headings are
    /// collapsed (matching Go's `detailHeadingOffsets`).
    /// When `exact_level` is `Some(n)`, every heading at that level is
    /// returned without collapsing (`headingOffsetsForLevel`).
    pub(crate) fn heading_offsets(
        &self,
        task: &Task,
        exact_level: Option<usize>,
        content_width: u16,
    ) -> Vec<usize> {
        let bq = (self.brightness * THEME_QUANTIZE) as i32;
        let sq = (self.saturation * THEME_QUANTIZE) as i32;

        // Check cache.
        {
            let cache = self.detail.heading_cache.borrow();
            if let Some(ref entry) = *cache {
                if entry.task_id == task.id as u32
                    && entry.body == task.body
                    && entry.theme == self.theme_kind
                    && entry.brightness_q == bq
                    && entry.saturation_q == sq
                    && entry.content_width == content_width
                    && entry.fold_level == self.fold_level()
                {
                    return match exact_level {
                        None => entry.offsets_any.clone(),
                        Some(2) => entry.offsets_l2.clone(),
                        Some(level) => Self::compute_level_offsets(
                            &entry.body_line_texts,
                            entry.meta_count,
                            level,
                        ),
                    };
                }
            }
        }

        // Cache miss — scan the actual rendered detail lines.
        let rendered = super::render::build_detail_lines(self, task, content_width);

        // Collect line texts and identify headings directly from the rendered output.
        let mut line_texts: Vec<String> = Vec::with_capacity(rendered.lines.len());
        let mut all_headings: Vec<usize> = Vec::new();
        let mut l2_headings: Vec<usize> = Vec::new();

        for (i, line) in rendered.lines.iter().enumerate() {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            let is_code = line.style.bg.is_some();

            if !is_code {
                if let Some(first_span) = line.spans.first() {
                    let content = first_span.content.as_ref();
                    if content.starts_with('#') {
                        all_headings.push(i);
                        // Levels 1-2: matches "# " and "## " but not "### ".
                        if !content.starts_with("###") {
                            l2_headings.push(i);
                        }
                    }
                }
            }

            line_texts.push(text);
        }

        // Collapse stacked any-level headings (consecutive headings with no
        // non-blank content between them — keep only the first of each group).
        let collapsed_line_indices = Self::collapse_rendered_headings(&all_headings, &line_texts);

        // Convert line indices to visual row offsets (accounting for wrapping).
        let offsets_any: Vec<usize> = collapsed_line_indices
            .iter()
            .map(|&idx| rendered.line_to_vrow(idx))
            .collect();
        let offsets_l2: Vec<usize> = l2_headings
            .iter()
            .map(|&idx| rendered.line_to_vrow(idx))
            .collect();

        // Determine where the body starts (for find-match and level computation).
        // The body starts after the "───" separator line in the metadata block.
        let meta_count = line_texts
            .iter()
            .position(|t| t.starts_with('#'))
            .unwrap_or(0);

        // Body line texts = everything from meta_count onwards (for find matching).
        let body_line_texts: Vec<String> = line_texts[meta_count..].to_vec();

        // Store in cache.
        *self.detail.heading_cache.borrow_mut() = Some(HeadingOffsetsCache {
            task_id: task.id as u32,
            body: task.body.clone(),
            theme: self.theme_kind,
            brightness_q: bq,
            saturation_q: sq,
            content_width,
            fold_level: self.fold_level(),
            offsets_any: offsets_any.clone(),
            offsets_l2: offsets_l2.clone(),
            body_line_texts,
            meta_count,
        });

        match exact_level {
            None => offsets_any,
            Some(2) => offsets_l2,
            Some(level) => {
                let line_indices = Self::compute_level_offsets(
                    &self.detail.heading_cache.borrow().as_ref().unwrap().body_line_texts,
                    meta_count,
                    level,
                );
                line_indices
                    .iter()
                    .map(|&idx| rendered.line_to_vrow(idx))
                    .collect()
            }
        }
    }

    /// Collapse stacked headings from absolute line indices: consecutive
    /// headings with only blank lines between them are grouped — keep only
    /// the first of each group.
    pub(crate) fn collapse_rendered_headings(headings: &[usize], line_texts: &[String]) -> Vec<usize> {
        if headings.len() <= 1 {
            return headings.to_vec();
        }
        let mut offsets = Vec::with_capacity(headings.len());
        offsets.push(headings[0]);
        for k in 1..headings.len() {
            let prev_idx = headings[k - 1];
            let cur_idx = headings[k];
            let has_content =
                (prev_idx + 1..cur_idx).any(|j| !line_texts[j].trim().is_empty());
            if has_content {
                offsets.push(cur_idx);
            }
        }
        offsets
    }

    /// Compute heading offsets up to (and including) the given level from body
    /// line texts.  E.g. `level=2` matches both `# ` and `## ` headings.
    /// `meta_count` is the offset of body lines within the full rendered view.
    pub(crate) fn compute_level_offsets(body_texts: &[String], meta_count: usize, level: usize) -> Vec<usize> {
        let max_prefix = "#".repeat(level);
        let deeper = format!("{}#", max_prefix);
        let mut offsets = Vec::new();
        for (i, text) in body_texts.iter().enumerate() {
            if text.starts_with('#') && !text.starts_with(&deeper) {
                offsets.push(meta_count + i);
            }
        }
        offsets
    }

    pub(crate) fn reader_next_heading(&mut self) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.reader_content_width();
            let offsets = self.heading_offsets(&task, None, w);
            for &off in &offsets {
                if off > self.reader_scroll {
                    self.reader_scroll = off;
                    return;
                }
            }
        }
    }

    pub(crate) fn reader_prev_heading(&mut self) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.reader_content_width();
            let offsets = self.heading_offsets(&task, None, w);
            let mut target = None;
            for &off in &offsets {
                if off >= self.reader_scroll {
                    break;
                }
                target = Some(off);
            }
            if let Some(t) = target {
                self.reader_scroll = t;
            }
        }
    }

    pub(crate) fn reader_next_heading_level(&mut self, level: usize) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.reader_content_width();
            let offsets = self.heading_offsets(&task, Some(level), w);
            for &off in &offsets {
                if off > self.reader_scroll {
                    self.reader_scroll = off;
                    return;
                }
            }
        }
    }

    pub(crate) fn reader_prev_heading_level(&mut self, level: usize) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.reader_content_width();
            let offsets = self.heading_offsets(&task, Some(level), w);
            let mut target = None;
            for &off in &offsets {
                if off >= self.reader_scroll {
                    break;
                }
                target = Some(off);
            }
            if let Some(t) = target {
                self.reader_scroll = t;
            }
        }
    }

    pub(crate) fn detail_next_heading(&mut self) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.detail_content_width();
            let offsets = self.heading_offsets(&task, None, w);
            for &off in &offsets {
                if off > self.detail.scroll {
                    self.detail.scroll = off;
                    return;
                }
            }
        }
    }

    pub(crate) fn detail_prev_heading(&mut self) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.detail_content_width();
            let offsets = self.heading_offsets(&task, None, w);
            let mut target = None;
            for &off in &offsets {
                if off >= self.detail.scroll {
                    break;
                }
                target = Some(off);
            }
            if let Some(t) = target {
                self.detail.scroll = t;
            }
        }
    }

    pub(crate) fn detail_next_heading_level(&mut self, level: usize) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.detail_content_width();
            let offsets = self.heading_offsets(&task, Some(level), w);
            for &off in &offsets {
                if off > self.detail.scroll {
                    self.detail.scroll = off;
                    return;
                }
            }
        }
    }

    pub(crate) fn detail_prev_heading_level(&mut self, level: usize) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.detail_content_width();
            let offsets = self.heading_offsets(&task, Some(level), w);
            let mut target = None;
            for &off in &offsets {
                if off >= self.detail.scroll {
                    break;
                }
                target = Some(off);
            }
            if let Some(t) = target {
                self.detail.scroll = t;
            }
        }
    }

    /// Jump to the Nth `##` heading (0-indexed) in the detail view.
    pub(crate) fn detail_goto_heading_index(&mut self, index: usize) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.detail_content_width();
            let offsets = self.heading_offsets(&task, Some(2), w);
            if let Some(&off) = offsets.get(index) {
                self.detail.scroll = off;
            }
        }
    }

    // ── Jump list helpers ──────────────────────────────────────────

    /// Create a snapshot of the current navigation state.
    pub(crate) fn current_snapshot(&self) -> JumpEntry {
        let (scroll, fold_level) = match self.view {
            AppView::Detail => (self.detail.scroll, self.detail.fold_level),
            _ => (0, 0),
        };
        JumpEntry {
            view: self.view,
            task_id: self.active_task().map(|t| t.id),
            col: self.active_col,
            row: self.active_row,
            scroll,
            fold_level,
            collapsed: self.columns.iter().map(|c| c.collapsed).collect(),
        }
    }

    /// Context switch: push current position, forking forward history.
    /// Used when entering detail (Enter) or switching tasks (goto).
    pub(crate) fn push_jump(&mut self) {
        let snap = self.current_snapshot();
        self.jump_list.push(snap);
    }

    /// Exit context: save final position without forking forward history.
    /// Used when leaving detail (q/Esc) — lets the user resume forward
    /// navigation after reviewing a past position.
    pub(crate) fn exit_jump(&mut self) {
        let snap = self.current_snapshot();
        self.jump_list.update_in_place(snap);
    }

    /// Restore navigation state from a jump entry.
    pub(crate) fn restore_jump(&mut self, entry: JumpEntry) {
        // Restore board layout (collapsed state).
        for (col, &was_collapsed) in self.columns.iter_mut().zip(entry.collapsed.iter()) {
            col.collapsed = was_collapsed;
        }
        // Restore cursor — try task_id first, fall back to col/row.
        if let Some(id) = entry.task_id {
            self.select_task_by_id(id);
        } else {
            self.active_col = entry.col.min(self.columns.len().saturating_sub(1));
            self.active_row = entry.row;
            self.clamp_active_row();
        }
        self.view = entry.view;
        if entry.view == AppView::Detail {
            self.detail.scroll = entry.scroll;
            self.detail.fold_level = entry.fold_level;
        }
    }

    // ── Find-in-detail (#49) ────────────────────────────────────────

    /// Recompute find matches from the current find_query against the
    /// active task's detail lines (metadata + body).
    ///
    /// Uses cached body line texts from the heading cache when available,
    /// avoiding a redundant markdown parse on every keystroke.
    pub fn recompute_find_matches(&mut self) {
        self.detail.find_matches.clear();
        self.detail.find_current = 0;

        if self.detail.find_query.is_empty() {
            return;
        }

        if let Some(task) = self.active_task().cloned() {
            let query = self.detail.find_query.to_lowercase();

            // Check title.
            if task.title.to_lowercase().contains(&query) {
                self.detail.find_matches.push(0);
            }

            // Ensure heading cache is populated (scans rendered detail lines).
            let w = self.detail_content_width();
            let _ = self.heading_offsets(&task, None, w);

            // Read cached body line texts.
            let cache = self.detail.heading_cache.borrow();
            if let Some(ref entry) = *cache {
                if entry.task_id == task.id as u32 && entry.body == task.body {
                    for (i, text) in entry.body_line_texts.iter().enumerate() {
                        if text.to_lowercase().contains(&query) {
                            self.detail.find_matches.push(entry.meta_count + i);
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn find_next(&mut self) {
        if self.detail.find_matches.is_empty() {
            return;
        }
        self.detail.find_current = (self.detail.find_current + 1) % self.detail.find_matches.len();
        self.scroll_to_find_match();
    }

    pub(crate) fn find_prev(&mut self) {
        if self.detail.find_matches.is_empty() {
            return;
        }
        if self.detail.find_current == 0 {
            self.detail.find_current = self.detail.find_matches.len() - 1;
        } else {
            self.detail.find_current -= 1;
        }
        self.scroll_to_find_match();
    }

    pub(crate) fn scroll_to_find_match(&mut self) {
        if let Some(&line_idx) = self.detail.find_matches.get(self.detail.find_current) {
            let w = self.detail_content_width();
            let cache = self.detail.cache.borrow();
            let vrow = if let Some(ref entry) = *cache {
                if entry.width == w {
                    entry.vrow_offsets.get(line_idx).copied().unwrap_or(line_idx)
                } else {
                    line_idx
                }
            } else {
                line_idx
            };
            drop(cache);
            let half_page = self.detail_page_size() / 2;
            self.detail.scroll = vrow.saturating_sub(half_page);
        }
    }

    /// Toggle select mode: disables/enables mouse capture so the terminal
    /// handles native text selection.
    pub(crate) fn toggle_select_mode(&mut self) {
        self.select_mode = !self.select_mode;
        let mut stdout = std::io::stdout();
        if self.select_mode {
            let _ = crossterm::execute!(stdout, crossterm::event::DisableMouseCapture);
            self.set_status("Visual mode ON — native text selection enabled");
        } else {
            let _ = crossterm::execute!(stdout, crossterm::event::EnableMouseCapture);
            self.set_status("Visual mode OFF");
        }
    }

    /// Best-effort clipboard copy via platform tools (pbcopy / xclip).
    fn copy_to_clipboard(text: &str) -> bool {
        #[cfg(target_os = "macos")]
        {
            use std::process::{Command, Stdio};
            if let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = std::io::Write::write_all(stdin, text.as_bytes());
                }
                let _ = child.wait();
                return true;
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            use std::process::{Command, Stdio};
            let result = Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(Stdio::piped())
                .spawn();
            if let Ok(mut child) = result {
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = std::io::Write::write_all(stdin, text.as_bytes());
                }
                let _ = child.wait();
                return true;
            }
        }
        false
    }

    pub(crate) fn copy_task_path(&mut self) {
        if let Some(task) = self.active_task() {
            if task.file.is_empty() {
                self.set_status("No file path");
                return;
            }
            let path = task.file.clone();
            Self::copy_to_clipboard(&path);
            self.set_status(format!("Copied: {}", path));
        }
    }

    /// Copy the active task's title and body to the clipboard.
    pub(crate) fn copy_task_content(&mut self) {
        if let Some(task) = self.active_task() {
            let mut text = task.title.clone();
            if !task.body.is_empty() {
                text.push_str("\n\n");
                text.push_str(&task.body);
            }
            Self::copy_to_clipboard(&text);
            self.set_status(format!("Copied content: {}", task.title));
        }
    }

    pub(crate) fn open_in_editor(&mut self) {
        if let Some(task) = self.active_task() {
            if task.file.is_empty() {
                self.set_status("No file path");
                return;
            }
            let file = task.file.clone();

            // Detect the current editor context and open the file appropriately.
            // For GUI editors (VS Code, Cursor, Zed, etc.) we can spawn the editor
            // without suspending the TUI since they open in a separate window/tab.
            if let Some(cmd) = Self::detect_gui_editor() {
                use std::process::{Command, Stdio};
                let result = Command::new(&cmd)
                    .arg(&file)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn();
                match result {
                    Ok(_) => self.set_status(format!("Opened in {cmd}")),
                    Err(e) => self.set_status(format!("Failed to open in {cmd}: {e}")),
                }
            } else {
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                self.set_status(format!("Run: {} {}", editor, file));
            }
        }
    }

    /// Detect a GUI editor from the current terminal environment.
    /// Returns the CLI command to use, or `None` if no GUI editor is detected.
    fn detect_gui_editor() -> Option<String> {
        // Check TERM_PROGRAM first — set by the terminal emulator / integrated terminal.
        if let Ok(term) = std::env::var("TERM_PROGRAM") {
            match term.to_lowercase().as_str() {
                "vscode" => return Some("code".to_string()),
                "cursor" => return Some("cursor".to_string()),
                _ => {}
            }
        }

        // Check VISUAL — convention for GUI-capable editors.
        if let Ok(visual) = std::env::var("VISUAL") {
            let cmd = visual.split_whitespace().next().unwrap_or("");
            // Only use VISUAL if it's a known GUI editor, not a terminal one.
            let base = std::path::Path::new(cmd)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(cmd);
            match base {
                "code" | "cursor" | "zed" | "subl" | "atom" | "mate" | "idea" | "webstorm"
                | "goland" | "rustrover" | "fleet" => return Some(visual),
                _ => {}
            }
        }

        None
    }

    pub fn fold_level(&self) -> usize {
        self.detail.fold_level
    }

    pub(crate) fn fold_deeper(&mut self) {
        let old = self.detail.fold_level;
        // Cycle: 0 → 3 → 2 (fold ### first, then ##).
        match self.detail.fold_level {
            0 => self.detail.fold_level = 3,
            3 => self.detail.fold_level = 2,
            _ => {} // already at max fold
        }
        if self.detail.fold_level != old {
            self.anchor_scroll_across_fold();
            self.set_status(format!("Fold level: h{}", self.detail.fold_level));
        }
    }

    pub(crate) fn fold_shallower(&mut self) {
        let old = self.detail.fold_level;
        // Cycle: 2 → 3 → 0.
        match self.detail.fold_level {
            2 => self.detail.fold_level = 3,
            3 => self.detail.fold_level = 0,
            _ => {} // already fully expanded
        }
        if self.detail.fold_level != old {
            self.anchor_scroll_across_fold();
            self.set_status(format!(
                "Fold level: {}",
                if self.detail.fold_level == 0 { "off".to_string() } else { format!("h{}", self.detail.fold_level) }
            ));
        }
    }

    /// After a fold-level change, adjust the scroll offset so that the
    /// content at the viewport stays at the same screen position.
    ///
    /// Uses heading ordinal matching: the N-th heading in the old
    /// rendering corresponds to the N-th heading in the new rendering
    /// (heading order is stable across fold levels). The scroll offset
    /// is shifted by exactly the amount the anchor heading moved.
    ///
    /// All positions are in **visual rows** (post-wrapping) to match
    /// `Paragraph::scroll()` semantics.
    pub(crate) fn anchor_scroll_across_fold(&mut self) {
        let scroll = match self.view {
            AppView::Detail => self.detail.scroll,
            _ => self.reader_scroll,
        };

        // 1. Collect heading visual-row positions from the OLD rendered lines.
        let (old_heading_vrows, cached_width) = {
            let cache = self.detail.cache.borrow();
            let entry = match cache.as_ref() {
                Some(e) => e,
                None => return,
            };
            // Find heading line indices (skip code blocks for consistency).
            let heading_line_indices: Vec<usize> = entry
                .lines
                .iter()
                .enumerate()
                .filter_map(|(i, line)| {
                    let is_code = line.style.bg.is_some();
                    if is_code {
                        return None;
                    }
                    line.spans
                        .first()
                        .filter(|s| s.content.starts_with('#'))
                        .map(|_| i)
                })
                .collect();
            // Convert to visual rows using cached offsets (O(1) per lookup).
            let vrows: Vec<usize> = heading_line_indices
                .iter()
                .map(|&idx| entry.vrow_offsets.get(idx).copied().unwrap_or(idx))
                .collect();
            (vrows, entry.width)
        };

        // Find the heading at or just before the scroll position (in visual rows).
        let anchor_idx = match old_heading_vrows.iter().rposition(|&pos| pos <= scroll) {
            Some(idx) => idx,
            None => return, // no heading before scroll — nothing to anchor
        };
        let old_vrow = old_heading_vrows[anchor_idx];

        // 2. Invalidate caches (fold_level already changed) and rebuild.
        *self.detail.cache.borrow_mut() = None;
        *self.detail.heading_cache.borrow_mut() = None;

        let task = match self.active_task() {
            Some(t) => t.clone(),
            None => return,
        };
        let content = super::render::build_detail_lines(self, &task, cached_width);

        // 3. Collect heading visual-row positions from the NEW rendered lines.
        let new_heading_line_indices: Vec<usize> = content
            .lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| {
                let is_code = line.style.bg.is_some();
                if is_code {
                    return None;
                }
                line.spans
                    .first()
                    .filter(|s| s.content.starts_with('#'))
                    .map(|_| i)
            })
            .collect();

        let new_vrow = match new_heading_line_indices.get(anchor_idx) {
            Some(&idx) => content.line_to_vrow(idx),
            None => return,
        };

        // 4. Shift scroll by exactly how far the anchor heading moved
        //    in visual-row space.
        let shift = new_vrow as isize - old_vrow as isize;
        let new_scroll = (scroll as isize + shift).max(0) as usize;
        match self.view {
            AppView::Detail => self.detail.scroll = new_scroll,
            _ => self.reader_scroll = new_scroll,
        }
    }

    pub(crate) fn adjust_reader_max_width(&mut self, delta: i16) {
        let current = self.reader_max_width as i16;
        let new_val = (current + delta).max(30).min(200);
        self.reader_max_width = new_val as u16;
        self.cfg.tui.reader_max_width = self.reader_max_width as i32;
        if let Err(e) = crate::io::config_file::save(&self.cfg) {
            self.set_status(format!("Config save error: {}", e));
        } else {
            self.set_status(format!("Detail width: {}", self.reader_max_width));
        }
    }

    pub(crate) fn adjust_reader_width_pct(&mut self, delta: i16) {
        let current = self.reader_width_pct as i16;
        let new_val = (current + delta).clamp(10, 90);
        self.reader_width_pct = new_val as u16;
        self.cfg.tui.reader_width_pct = self.reader_width_pct as i32;
        if let Err(e) = crate::io::config_file::save(&self.cfg) {
            self.set_status(format!("Config save error: {}", e));
        } else {
            self.set_status(format!("Reader width: {}%", self.reader_width_pct));
        }
    }

    pub(crate) fn persist_deleted_task(&mut self, task: &Task) {
        if task.file.is_empty() {
            return;
        }
        let path = std::path::Path::new(&task.file);
        if let Err(e) = crate::io::task_file::write(path, task) {
            self.set_status(format!("Save error: {}", e));
        }
    }

    // ── Semantic search (~prefix) helpers ──────────────────────────

    /// Returns true if the query contains a `~` semantic search component.
    pub fn is_semantic_query(query: &str) -> bool {
        query.contains('~')
    }

    /// Extracts the semantic search portion (text after `~`), trimmed.
    pub(crate) fn sem_query_text(query: &str) -> &str {
        match query.find('~') {
            Some(pos) => query[pos + 1..].trim_start(),
            None => "",
        }
    }

    /// Extracts the DSL portion (text before `~`), trimmed.
    pub(crate) fn dsl_query_text(query: &str) -> &str {
        match query.find('~') {
            Some(pos) => query[..pos].trim_end(),
            None => query,
        }
    }

    /// Returns true if semantic search is configured (provider is set).
    /// The index is auto-synced on first query if it doesn't exist yet.
    pub(crate) fn sem_available(&self) -> bool {
        !self.cfg.semantic_search.provider.is_empty()
    }

    /// Resets all semantic search state (board scores + detail find).
    pub(crate) fn clear_sem_state(&mut self) {
        self.search.sem_last_key = None;
        self.search.sem_pending = false;
        self.search.sem_loading = false;
        self.search.sem_error = None;
        self.search.sem_search_rx = None;
        self.search.sem_scores.clear();
        self.search.sem_find_rx = None;
    }

    /// Resets only the detail-find semantic state, preserving board-level
    /// `sem_scores` and `sem_search_rx`.
    pub(crate) fn clear_sem_find_state(&mut self) {
        self.search.sem_last_key = None;
        self.search.sem_pending = false;
        self.search.sem_loading = false;
        self.search.sem_error = None;
        self.search.sem_find_rx = None;
    }

    /// Called when the search query changes (board `/` mode).
    /// If query contains `~`, arms debounce for semantic search (DSL tokens
    /// before `~` are applied live by `filtered_tasks`).
    /// Otherwise, semantic state is cleared.
    pub(crate) fn on_search_query_changed(&mut self) {
        if Self::is_semantic_query(&self.search.query) {
            if self.sem_available() {
                self.search.sem_error = None;
                self.search.sem_last_key = Some(Instant::now());
                self.search.sem_pending = true;
            } else {
                self.search.sem_error = Some("semantic search not configured (set semantic_search.provider in config.toml)".into());
                self.search.sem_scores.clear();
            }
        } else {
            // Plain DSL: clear semantic state, filtering is live in rendered view.
            self.clear_sem_state();
        }
    }

    /// Called when the find query changes (detail `/` mode).
    /// If query contains `~`, arms debounce for semantic find.
    /// Otherwise, delegates to `recompute_find_matches()`.
    /// Uses `clear_sem_find_state()` to preserve board-level `sem_scores`.
    pub(crate) fn on_find_query_changed(&mut self) {
        if Self::is_semantic_query(&self.detail.find_query) {
            if self.sem_available() {
                self.search.sem_error = None;
                self.search.sem_last_key = Some(Instant::now());
                self.search.sem_pending = true;
            } else {
                self.search.sem_error = Some("semantic search not configured (set semantic_search.provider in config.toml)".into());
            }
        } else {
            self.clear_sem_find_state();
            self.recompute_find_matches();
        }
    }

    /// Checks debounce timer and async result channels. Called from event loop.
    pub fn tick_semantic_debounce(&mut self) {
        // Check for completed board-level semantic search results.
        if let Some(rx) = &self.search.sem_search_rx {
            if let Ok(result) = rx.try_recv() {
                let current_sem = Self::sem_query_text(&self.search.query);
                if result.query == current_sem {
                    if let Some(err) = result.error {
                        self.search.sem_error = Some(err);
                        self.search.sem_scores.clear();
                    } else {
                        self.search.sem_scores = result.scores;
                        self.search.sem_error = None;
                        // Navigate to the best (highest-scoring) match.
                        if let Some((&best_id, _)) = self.search.sem_scores.iter()
                            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                        {
                            self.select_task_by_id(best_id);
                        }
                    }
                    self.search.sem_loading = false;
                }
                self.debug.needs_redraw = true;
            }
        }

        // Check for completed detail-level semantic find results.
        if let Some(rx) = &self.search.sem_find_rx {
            if let Ok(result) = rx.try_recv() {
                let current_sem = Self::sem_query_text(&self.detail.find_query);
                if result.query == current_sem {
                    if let Some(err) = result.error {
                        self.search.sem_error = Some(err);
                        self.detail.find_matches.clear();
                    } else {
                        self.detail.find_matches = result.line_indices;
                        self.detail.find_current = 0;
                        self.search.sem_error = None;
                        self.scroll_to_find_match();
                    }
                    self.search.sem_loading = false;
                }
                self.debug.needs_redraw = true;
            }
        }

        // Fire debounced search if timer has elapsed.
        if self.search.sem_pending {
            if let Some(last) = self.search.sem_last_key {
                if last.elapsed() >= Duration::from_millis(SEMANTIC_DEBOUNCE_MS) {
                    self.search.sem_pending = false;
                    self.fire_semantic();
                }
            }
        }
    }

    /// Dispatches the appropriate semantic search based on current view.
    pub(crate) fn fire_semantic(&mut self) {
        match self.view {
            AppView::Search | AppView::Board => self.fire_sem_board_search(),
            AppView::Detail => self.fire_sem_detail_find(),
            _ => {}
        }
    }

    /// Launches board-level semantic search in a background thread.
    pub(crate) fn fire_sem_board_search(&mut self) {
        let query = Self::sem_query_text(&self.search.query).to_string();
        if query.is_empty() {
            return;
        }

        self.search.sem_loading = true;
        self.debug.needs_redraw = true;

        let cfg = self.cfg.clone();
        let (tx, rx) = mpsc::channel();
        self.search.sem_search_rx = Some(rx);

        std::thread::spawn(move || {
            let result = match crate::embed::Manager::new(&cfg) {
                Ok(mut mgr) => {
                    // Auto-sync if the index is empty (first use or after clear).
                    if mgr.doc_count() == 0 {
                        let sync_err = match crate::model::task::read_all_lenient(&cfg.tasks_path()) {
                            Ok((tasks, _)) => mgr.sync(&tasks).err().map(|e| format!("sync: {e}")),
                            Err(e) => Some(format!("loading tasks: {e}")),
                        };
                        if let Some(err) = sync_err {
                            let _ = tx.send(SemSearchResult {
                                query,
                                scores: HashMap::new(),
                                error: Some(err),
                            });
                            return;
                        }
                    }
                    match mgr.search(&query, 0) {
                        Ok(results) => SemSearchResult {
                            query,
                            scores: results.iter().map(|r| (r.task_id, r.score)).collect(),
                            error: None,
                        },
                        Err(e) => SemSearchResult {
                            query,
                            scores: HashMap::new(),
                            error: Some(e.to_string()),
                        },
                    }
                }
                Err(e) => SemSearchResult {
                    query,
                    scores: HashMap::new(),
                    error: Some(e.to_string()),
                },
            };
            let _ = tx.send(result);
        });
    }

    /// Launches detail-level semantic find in a background thread.
    pub(crate) fn fire_sem_detail_find(&mut self) {
        let query = Self::sem_query_text(&self.detail.find_query).to_string();
        if query.is_empty() {
            return;
        }

        let task = match self.active_task().cloned() {
            Some(t) => t,
            None => return,
        };
        let task_id = task.id;

        self.search.sem_loading = true;
        self.debug.needs_redraw = true;

        // Ensure heading cache is populated so we have rendered body line texts.
        let w = self.detail_content_width();
        let _ = self.heading_offsets(&task, None, w);

        let cfg = self.cfg.clone();
        // Capture the cached body line texts so we can map chunk headers/lines
        // to rendered detail-view line indices on the background thread.
        let (meta_count, body_line_texts) = {
            let cache = self.detail.heading_cache.borrow();
            match cache.as_ref() {
                Some(c) if c.task_id == task_id as u32 => {
                    (c.meta_count, c.body_line_texts.clone())
                }
                _ => (0, Vec::new()),
            }
        };

        let (tx, rx) = mpsc::channel();
        self.search.sem_find_rx = Some(rx);

        std::thread::spawn(move || {
            let result = match crate::embed::Manager::new(&cfg) {
                Ok(mut mgr) => {
                    // Auto-sync if the index is empty (first use or after clear).
                    if mgr.doc_count() == 0 {
                        let sync_err = match crate::model::task::read_all_lenient(&cfg.tasks_path()) {
                            Ok((tasks, _)) => mgr.sync(&tasks).err().map(|e| format!("sync: {e}")),
                            Err(e) => Some(format!("loading tasks: {e}")),
                        };
                        if let Some(err) = sync_err {
                            let _ = tx.send(SemFindResult {
                                query,
                                line_indices: Vec::new(),
                                error: Some(err),
                            });
                            return;
                        }
                    }
                    // Use a large limit so the current task's chunks aren't excluded
                    // by higher-scoring chunks from other tasks.
                    match mgr.find(&query, 500) {
                        Ok(results) => {
                            // Filter to chunks belonging to this task.
                            let task_results: Vec<_> = results
                                .iter()
                                .filter(|r| r.task_id == task_id)
                                .collect();

                            // Map each matching chunk to rendered detail-view line
                            // indices. A chunk's `r.line` is the raw body line where
                            // its heading starts. Find the closest body_line_text
                            // that contains the heading to get the rendered index.
                            let mut line_indices: Vec<usize> = Vec::new();
                            for r in &task_results {
                                if !r.header.is_empty() {
                                    // Find rendered line matching this section header.
                                    if let Some(idx) =
                                        body_line_texts.iter().position(|t| {
                                            t.trim() == r.header.trim()
                                                || t.trim()
                                                    .starts_with(r.header.trim())
                                        })
                                    {
                                        line_indices.push(meta_count + idx);
                                    }
                                } else {
                                    // Preamble or headerless chunk — match at body start.
                                    line_indices.push(meta_count);
                                }
                            }
                            line_indices.dedup();

                            SemFindResult {
                                query,
                                line_indices,
                                error: None,
                            }
                        }
                        Err(e) => SemFindResult {
                            query,
                            line_indices: Vec::new(),
                            error: Some(e.to_string()),
                        },
                    }
                }
                Err(e) => SemFindResult {
                    query,
                    line_indices: Vec::new(),
                    error: Some(e.to_string()),
                },
            };
            let _ = tx.send(result);
        });
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    // ── Test helpers ─────────────────────────────────────────────────

    /// Build a minimal Config suitable for TUI tests (no file I/O).
    fn test_config() -> Config {
        let mut cfg = Config::new_default("test-board");
        // Point dir at a temp path so InputHistory::with_path doesn't
        // interfere with the real board.
        cfg.set_dir(std::env::temp_dir().join("kbmdx-tui-test"));
        cfg
    }

    /// Build a sample task with the given id, title, and status.
    fn make_task(id: i32, title: &str, status: &str, priority: &str) -> Task {
        let now = Utc::now();
        Task {
            id,
            title: title.to_string(),
            status: status.to_string(),
            priority: priority.to_string(),
            created: now,
            updated: now,
            started: None,
            completed: None,
            assignee: String::new(),
            tags: Vec::new(),
            due: None,
            estimate: String::new(),
            parent: None,
            depends_on: Vec::new(),
            blocked: false,
            block_reason: String::new(),
            claimed_by: String::new(),
            claimed_at: None,
            class: String::new(),
            branch: String::new(),
            worktree: String::new(),
            body: String::new(),
            file: String::new(),
        }
    }

    /// Build a test App with 4 tasks across 3 statuses (backlog has 0,
    /// todo has 2, in-progress has 1, review has 0, done has 1).
    fn test_app() -> App {
        let tasks = vec![
            make_task(1, "First task", "todo", "high"),
            make_task(2, "Second task", "todo", "medium"),
            make_task(3, "Third task", "in-progress", "low"),
            make_task(4, "Done task", "done", "medium"),
        ];
        let cfg = test_config();
        let mut app = App::new(cfg, tasks);
        app.terminal_width = 120;
        app.terminal_height = 40;
        app
    }

    /// Simulate a keypress.
    fn send_key(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        });
    }

    /// Simulate a keypress with modifiers.
    fn send_key_mod(app: &mut App, code: KeyCode, mods: KeyModifiers) {
        app.handle_key(KeyEvent {
            code,
            modifiers: mods,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        });
    }

    // ── Construction tests ───────────────────────────────────────────

    #[test]
    fn app_new_distributes_tasks_to_columns() {
        let app = test_app();
        let names: Vec<&str> = app.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"todo"), "should have 'todo' column");
        assert!(names.contains(&"in-progress"), "should have 'in-progress' column");
        assert!(names.contains(&"done"), "should have 'done' column");

        let todo_col = app.columns.iter().find(|c| c.name == "todo").unwrap();
        assert_eq!(todo_col.tasks.len(), 2);

        let ip_col = app.columns.iter().find(|c| c.name == "in-progress").unwrap();
        assert_eq!(ip_col.tasks.len(), 1);

        let done_col = app.columns.iter().find(|c| c.name == "done").unwrap();
        assert_eq!(done_col.tasks.len(), 1);
    }

    #[test]
    fn app_starts_on_board_view() {
        let app = test_app();
        assert_eq!(app.view, AppView::Board);
        assert!(!app.should_quit);
    }

    // ── Navigation tests ─────────────────────────────────────────────

    #[test]
    fn j_k_navigate_rows() {
        let mut app = test_app();
        // Navigate to todo column (which has 2 tasks).
        let todo_idx = app.columns.iter().position(|c| c.name == "todo").unwrap();
        app.active_col = todo_idx;
        app.active_row = 0;

        send_key(&mut app, KeyCode::Char('j'));
        assert_eq!(app.active_row, 1, "j should move down");

        // j wraps around at the bottom.
        send_key(&mut app, KeyCode::Char('j'));
        assert_eq!(app.active_row, 0, "j at bottom should wrap to top");

        // Move back down.
        send_key(&mut app, KeyCode::Char('j'));
        assert_eq!(app.active_row, 1);

        send_key(&mut app, KeyCode::Char('k'));
        assert_eq!(app.active_row, 0, "k should move up");
    }

    #[test]
    fn h_l_navigate_columns() {
        let mut app = test_app();
        let start_col = app.active_col;

        send_key(&mut app, KeyCode::Char('l'));
        assert!(app.active_col > start_col || app.columns.len() == 1,
                "l should move right");

        send_key(&mut app, KeyCode::Char('h'));
        assert_eq!(app.active_col, start_col, "h should move back left");
    }

    #[test]
    fn arrow_keys_navigate() {
        let mut app = test_app();
        let todo_idx = app.columns.iter().position(|c| c.name == "todo").unwrap();
        app.active_col = todo_idx;
        app.active_row = 0;

        send_key(&mut app, KeyCode::Down);
        assert_eq!(app.active_row, 1, "Down arrow should move down");

        send_key(&mut app, KeyCode::Up);
        assert_eq!(app.active_row, 0, "Up arrow should move up");
    }

    // ── View transition tests ────────────────────────────────────────

    #[test]
    fn enter_opens_detail_view() {
        let mut app = test_app();
        let todo_idx = app.columns.iter().position(|c| c.name == "todo").unwrap();
        app.active_col = todo_idx;
        app.active_row = 0;

        send_key(&mut app, KeyCode::Enter);
        assert_eq!(app.view, AppView::Detail, "Enter should open detail view");
    }

    #[test]
    fn esc_returns_from_detail_to_board() {
        let mut app = test_app();
        let todo_idx = app.columns.iter().position(|c| c.name == "todo").unwrap();
        app.active_col = todo_idx;

        send_key(&mut app, KeyCode::Enter);
        assert_eq!(app.view, AppView::Detail);

        send_key(&mut app, KeyCode::Esc);
        assert_eq!(app.view, AppView::Board, "Esc should return to board");
    }

    #[test]
    fn question_mark_opens_help() {
        let mut app = test_app();
        send_key(&mut app, KeyCode::Char('?'));
        assert_eq!(app.view, AppView::Help, "? should open help overlay");
    }

    #[test]
    fn esc_closes_help() {
        let mut app = test_app();
        send_key(&mut app, KeyCode::Char('?'));
        assert_eq!(app.view, AppView::Help);

        send_key(&mut app, KeyCode::Esc);
        assert_eq!(app.view, AppView::Board, "Esc should close help");
    }

    #[test]
    fn m_opens_move_dialog() {
        let mut app = test_app();
        let todo_idx = app.columns.iter().position(|c| c.name == "todo").unwrap();
        app.active_col = todo_idx;
        app.active_row = 0;

        send_key(&mut app, KeyCode::Char('m'));
        assert_eq!(app.view, AppView::MoveTask, "m should open move dialog");
    }

    #[test]
    fn esc_closes_move_dialog() {
        let mut app = test_app();
        let todo_idx = app.columns.iter().position(|c| c.name == "todo").unwrap();
        app.active_col = todo_idx;

        send_key(&mut app, KeyCode::Char('m'));
        assert_eq!(app.view, AppView::MoveTask);

        send_key(&mut app, KeyCode::Esc);
        assert_eq!(app.view, AppView::Board, "Esc should close move dialog");
    }

    #[test]
    fn d_opens_delete_confirm() {
        let mut app = test_app();
        let todo_idx = app.columns.iter().position(|c| c.name == "todo").unwrap();
        app.active_col = todo_idx;

        send_key(&mut app, KeyCode::Char('d'));
        assert_eq!(app.view, AppView::ConfirmDelete, "d should open delete confirm");
    }

    // ── Search tests ─────────────────────────────────────────────────

    #[test]
    fn slash_opens_search() {
        let mut app = test_app();
        send_key(&mut app, KeyCode::Char('/'));
        assert_eq!(app.view, AppView::Search, "/ should open search");
    }

    #[test]
    fn esc_closes_search() {
        let mut app = test_app();
        send_key(&mut app, KeyCode::Char('/'));
        assert_eq!(app.view, AppView::Search);

        send_key(&mut app, KeyCode::Esc);
        assert_eq!(app.view, AppView::Board);
    }

    // ── Ctrl+C quit test ─────────────────────────────────────────────

    #[test]
    fn ctrl_c_quits() {
        let mut app = test_app();
        send_key_mod(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(app.should_quit, "Ctrl+C should set should_quit");
    }

    #[test]
    fn q_quits_from_board() {
        let mut app = test_app();
        send_key(&mut app, KeyCode::Char('q'));
        assert!(app.should_quit, "q should quit from board view");
    }

    // ── View mode toggle ─────────────────────────────────────────────

    #[test]
    fn shift_v_toggles_view_mode() {
        let mut app = test_app();
        let original = app.view_mode.clone();
        send_key(&mut app, KeyCode::Char('V'));
        assert_ne!(app.view_mode, original, "V should toggle view mode");
    }

    // ── Create wizard ────────────────────────────────────────────────

    #[test]
    fn c_opens_create_wizard() {
        let mut app = test_app();
        send_key(&mut app, KeyCode::Char('c'));
        assert_eq!(app.view, AppView::CreateTask, "c should open create wizard");
    }

    #[test]
    fn esc_closes_create_wizard() {
        let mut app = test_app();
        send_key(&mut app, KeyCode::Char('c'));
        assert_eq!(app.view, AppView::CreateTask);

        send_key(&mut app, KeyCode::Esc);
        assert_eq!(app.view, AppView::Board, "Esc should close create wizard");
    }

    // ── Goto dialog ──────────────────────────────────────────────────

    #[test]
    fn colon_opens_goto_dialog() {
        let mut app = test_app();
        send_key(&mut app, KeyCode::Char(':'));
        assert!(app.goto_active, ": should open goto dialog");
    }

    #[test]
    fn esc_closes_goto() {
        let mut app = test_app();
        send_key(&mut app, KeyCode::Char(':'));
        assert!(app.goto_active);

        send_key(&mut app, KeyCode::Esc);
        assert!(!app.goto_active, "Esc should close goto dialog");
    }

    // ── InputHistory tests ───────────────────────────────────────────

    #[test]
    fn input_history_push_deduplicates() {
        let dir = std::env::temp_dir().join("kbmdx-test-history");
        let _ = std::fs::create_dir_all(&dir);
        let mut h = InputHistory::with_path(dir.join("test_hist"));
        h.push("first");
        h.push("second");
        h.push("first"); // moves to end
        assert_eq!(h.entries(), &["second", "first"]);
    }

    #[test]
    fn input_history_up_down_navigation() {
        let dir = std::env::temp_dir().join("kbmdx-test-history");
        let _ = std::fs::create_dir_all(&dir);
        let mut h = InputHistory::with_path(dir.join("test_hist2"));
        h.push("alpha");
        h.push("beta");
        h.push("gamma");

        // Up navigates to most recent.
        let val = h.up("current");
        assert_eq!(val, Some("gamma"));

        let val = h.up("current");
        assert_eq!(val, Some("beta"));

        let val = h.up("current");
        assert_eq!(val, Some("alpha"));

        // At the top, stays at first entry.
        let val = h.up("current");
        assert_eq!(val, Some("alpha"));

        // Down moves toward recent.
        let val = h.down("current");
        assert_eq!(val, Some("beta"));

        let val = h.down("current");
        assert_eq!(val, Some("gamma"));

        // Past the end, returns the draft.
        let val = h.down("current");
        assert_eq!(val, Some("current"));

        // Now we're in "not browsing" state, down returns None.
        let val = h.down("current");
        assert_eq!(val, None);
    }

    #[test]
    fn input_history_reset() {
        let dir = std::env::temp_dir().join("kbmdx-test-history");
        let _ = std::fs::create_dir_all(&dir);
        let mut h = InputHistory::with_path(dir.join("test_hist3"));
        h.push("a");
        h.push("b");
        h.up("x");
        h.reset();
        // After reset, down should return None (not browsing).
        assert_eq!(h.down("x"), None);
    }

    #[test]
    fn input_history_empty_entries_ignored() {
        let dir = std::env::temp_dir().join("kbmdx-test-history");
        let _ = std::fs::create_dir_all(&dir);
        let mut h = InputHistory::with_path(dir.join("test_hist4"));
        h.push("");
        h.push("   ");
        assert!(h.entries().is_empty());
    }

    #[test]
    fn input_history_completions() {
        let dir = std::env::temp_dir().join("kbmdx-test-history");
        let _ = std::fs::create_dir_all(&dir);
        let mut h = InputHistory::with_path(dir.join("test_hist5"));
        h.push("priority:high");
        h.push("priority:low");
        h.push("status:todo");

        let completions = h.completions("pri");
        assert_eq!(completions, vec!["priority:high", "priority:low"]);

        let completions = h.completions("stat");
        assert_eq!(completions, vec!["status:todo"]);

        let completions = h.completions("");
        assert!(completions.is_empty());
    }

    // ── Sort mode cycling ────────────────────────────────────────────

    #[test]
    fn s_cycles_sort_mode() {
        let mut app = test_app();
        let initial = app.sort_mode.clone();
        send_key(&mut app, KeyCode::Char('s'));
        assert_ne!(app.sort_mode, initial, "s should cycle sort mode");
    }

    // ── All views reachable and dismissible ──────────────────────────

    #[test]
    fn all_dialog_views_dismissible_with_esc() {
        let mut app = test_app();
        let todo_idx = app.columns.iter().position(|c| c.name == "todo").unwrap();
        app.active_col = todo_idx;

        // Detail
        send_key(&mut app, KeyCode::Enter);
        assert_eq!(app.view, AppView::Detail);
        send_key(&mut app, KeyCode::Esc);
        assert_eq!(app.view, AppView::Board);

        // Help
        send_key(&mut app, KeyCode::Char('?'));
        assert_eq!(app.view, AppView::Help);
        send_key(&mut app, KeyCode::Esc);
        assert_eq!(app.view, AppView::Board);

        // Search
        send_key(&mut app, KeyCode::Char('/'));
        assert_eq!(app.view, AppView::Search);
        send_key(&mut app, KeyCode::Esc);
        assert_eq!(app.view, AppView::Board);

        // Move
        send_key(&mut app, KeyCode::Char('m'));
        assert_eq!(app.view, AppView::MoveTask);
        send_key(&mut app, KeyCode::Esc);
        assert_eq!(app.view, AppView::Board);

        // Delete
        send_key(&mut app, KeyCode::Char('d'));
        assert_eq!(app.view, AppView::ConfirmDelete);
        send_key(&mut app, KeyCode::Esc);
        assert_eq!(app.view, AppView::Board);

        // Create
        send_key(&mut app, KeyCode::Char('c'));
        assert_eq!(app.view, AppView::CreateTask);
        send_key(&mut app, KeyCode::Esc);
        assert_eq!(app.view, AppView::Board);

        // Goto
        send_key(&mut app, KeyCode::Char(':'));
        assert!(app.goto_active);
        send_key(&mut app, KeyCode::Esc);
        assert!(!app.goto_active);
    }

    // ── Debug/perf mode ─────────────────────────────────────────────

    #[test]
    fn f12_toggles_perf_mode() {
        let mut app = test_app();
        let initial = app.debug.perf_mode;
        send_key(&mut app, KeyCode::F(12));
        assert_ne!(app.debug.perf_mode, initial, "F12 should toggle perf mode");
        send_key(&mut app, KeyCode::F(12));
        assert_eq!(app.debug.perf_mode, initial, "F12 twice should restore state");
    }
}
