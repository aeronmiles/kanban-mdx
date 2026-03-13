//! Task model — mirrors the Go `internal/task` package.
//!
//! Provides the core `Task` struct, lifecycle helpers (timestamps, claims),
//! slug/filename generation, section parsing, file I/O, and consistency checks.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::config::Config;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("task not found: #{0}")]
    NotFound(i32),

    #[error("invalid status {status:?}")]
    InvalidStatus {
        status: String,
        allowed: Vec<String>,
    },

    #[error("invalid priority {priority:?}")]
    InvalidPriority {
        priority: String,
        allowed: Vec<String>,
    },

    #[error("invalid class {class:?}")]
    InvalidClass {
        class: String,
        allowed: Vec<String>,
    },

    #[error("invalid {field} date: {message}")]
    InvalidDate { field: String, message: String },

    #[error("invalid task ID {input:?}")]
    InvalidTaskID { input: String },

    #[error("task cannot depend on itself (ID {0})")]
    SelfReference(i32),

    #[error("dependency task #{0} not found")]
    DependencyNotFound(i32),

    #[error("WIP limit reached for {status:?} ({current}/{limit})")]
    WIPLimitExceeded {
        status: String,
        limit: i32,
        current: i32,
    },

    #[error("class {class:?} WIP limit reached ({current}/{limit} board-wide)")]
    ClassWIPExceeded {
        class: String,
        limit: i32,
        current: i32,
    },

    #[error("task #{id} is already at the {direction} status ({status})")]
    BoundaryError {
        id: i32,
        status: String,
        direction: String,
    },

    #[error("status {status:?} requires --claim <name>")]
    ClaimRequired { status: String },

    #[error(
        "task #{id} is claimed by {claimed_by:?} (expires in {remaining}). \
         If this is you, add: --claim {claimed_by}"
    )]
    TaskClaimed {
        id: i32,
        claimed_by: String,
        remaining: String,
    },

    #[error("missing required field: {0}")]
    MissingField(&'static str),

    #[error("{0}")]
    Io(#[from] std::io::Error),

    #[error("parsing frontmatter in {path}: {source}")]
    Yaml {
        path: String,
        source: serde_yml::Error,
    },

    #[error("{0}")]
    Parse(String),

    #[error("cannot extract ID from filename {0:?}")]
    FilenameParseError(String),
}

// ---------------------------------------------------------------------------
// Helper predicates for serde skip_serializing_if
// ---------------------------------------------------------------------------

fn is_false(v: &bool) -> bool {
    !v
}

fn is_empty_string(s: &str) -> bool {
    s.is_empty()
}

fn is_empty_vec<T>(v: &[T]) -> bool {
    v.is_empty()
}

// ---------------------------------------------------------------------------
// Task struct
// ---------------------------------------------------------------------------

/// A kanban task parsed from a markdown file with YAML frontmatter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i32,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started: Option<DateTime<Utc>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed: Option<DateTime<Utc>>,

    #[serde(default, skip_serializing_if = "is_empty_string")]
    pub assignee: String,

    #[serde(default, skip_serializing_if = "is_empty_vec")]
    pub tags: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<NaiveDate>,

    #[serde(default, skip_serializing_if = "is_empty_string")]
    pub estimate: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<i32>,

    #[serde(default, skip_serializing_if = "is_empty_vec")]
    pub depends_on: Vec<i32>,

    #[serde(default, skip_serializing_if = "is_false")]
    pub blocked: bool,

    #[serde(default, skip_serializing_if = "is_empty_string")]
    pub block_reason: String,

    #[serde(default, skip_serializing_if = "is_empty_string")]
    pub claimed_by: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_at: Option<DateTime<Utc>>,

    #[serde(default, skip_serializing_if = "is_empty_string")]
    pub class: String,

    #[serde(default, skip_serializing_if = "is_empty_string")]
    pub branch: String,

    #[serde(default, skip_serializing_if = "is_empty_string")]
    pub worktree: String,

    /// Markdown content below the frontmatter (not serialized to YAML).
    /// Included in JSON output when non-empty; the IO layer handles YAML
    /// separation so we use skip_deserializing + skip_serializing_if here.
    #[serde(skip_deserializing, skip_serializing_if = "is_empty_string")]
    pub body: String,

    /// Path to the task file (not serialized to YAML).
    /// Included in JSON output when non-empty.
    #[serde(skip_deserializing, skip_serializing_if = "is_empty_string")]
    pub file: String,
}

impl Default for Task {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: 0,
            title: String::new(),
            status: String::new(),
            priority: String::new(),
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
}

// ---------------------------------------------------------------------------
// Lifecycle — timestamp management
// ---------------------------------------------------------------------------

/// Sets `started` and `completed` based on a status transition.
///
/// - Sets `started` on first move out of the initial status (never overwrites).
/// - Sets `completed` on move to a terminal status; also sets `started` if `None`.
/// - Clears `completed` when moving away from a terminal status (reopening).
pub fn update_timestamps(task: &mut Task, old_status: &str, new_status: &str, cfg: &Config) {
    let now = Utc::now();
    let initial_status = cfg
        .status_names()
        .into_iter()
        .next()
        .unwrap_or_default();

    // Set Started on first move out of initial status (never overwrite).
    if task.started.is_none() && old_status == initial_status && new_status != initial_status {
        task.started = Some(now);
    }

    // Set/clear Completed based on terminal status.
    if cfg.is_terminal_status(new_status) {
        task.completed = Some(now);
        // Direct move to terminal: also set Started if None.
        if task.started.is_none() {
            task.started = Some(now);
        }
    } else if cfg.is_terminal_status(old_status) {
        // Reopening: clear Completed, preserve Started.
        task.completed = None;
    }
}

// ---------------------------------------------------------------------------
// Claim checking
// ---------------------------------------------------------------------------

/// Verifies that a mutating operation is allowed on a claimed task.
///
/// If the task is unclaimed, claimed by the same agent, or the claim has
/// expired, the operation proceeds (returns `Ok(())`). For expired claims
/// the claim fields are cleared as a side-effect. Otherwise returns
/// `TaskClaimed`.
pub fn check_claim(
    task: &mut Task,
    claimant: &str,
    timeout: chrono::Duration,
) -> Result<(), TaskError> {
    if task.claimed_by.is_empty() {
        return Ok(());
    }
    if !claimant.is_empty() && task.claimed_by == claimant {
        return Ok(());
    }
    if timeout > chrono::Duration::zero() {
        if let Some(claimed_at) = task.claimed_at {
            if Utc::now() - claimed_at > timeout {
                task.claimed_by.clear();
                task.claimed_at = None;
                return Ok(());
            }
        }
    }

    let remaining = if timeout > chrono::Duration::zero() {
        if let Some(claimed_at) = task.claimed_at {
            let left = timeout - (Utc::now() - claimed_at);
            let mins = left.num_minutes();
            if mins >= 60 {
                format!("{}h{}m", mins / 60, mins % 60)
            } else {
                format!("{}m", mins)
            }
        } else {
            "unknown".to_string()
        }
    } else {
        "unknown".to_string()
    };

    Err(TaskError::TaskClaimed {
        id: task.id,
        claimed_by: task.claimed_by.clone(),
        remaining,
    })
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

pub fn validate_status(status: &str, allowed: &[String]) -> Result<(), TaskError> {
    if allowed.iter().any(|s| s == status) {
        Ok(())
    } else {
        Err(TaskError::InvalidStatus {
            status: status.to_string(),
            allowed: allowed.to_vec(),
        })
    }
}

pub fn validate_priority(priority: &str, allowed: &[String]) -> Result<(), TaskError> {
    if allowed.iter().any(|p| p == priority) {
        Ok(())
    } else {
        Err(TaskError::InvalidPriority {
            priority: priority.to_string(),
            allowed: allowed.to_vec(),
        })
    }
}

pub fn validate_class(class: &str, allowed: &[String]) -> Result<(), TaskError> {
    if allowed.iter().any(|c| c == class) {
        Ok(())
    } else {
        Err(TaskError::InvalidClass {
            class: class.to_string(),
            allowed: allowed.to_vec(),
        })
    }
}

pub fn validate_dependency_ids(
    tasks_dir: &Path,
    self_id: i32,
    ids: &[i32],
) -> Result<(), TaskError> {
    for &dep_id in ids {
        if dep_id == self_id {
            return Err(TaskError::SelfReference(dep_id));
        }
        find_by_id(tasks_dir, dep_id)?;
    }
    Ok(())
}

fn validate_required_fields(t: &Task) -> Result<(), TaskError> {
    if t.id < 1 {
        return Err(TaskError::MissingField("id"));
    }
    if t.title.trim().is_empty() {
        return Err(TaskError::MissingField("title"));
    }
    if t.status.trim().is_empty() {
        return Err(TaskError::MissingField("status"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Slug & filename generation
// ---------------------------------------------------------------------------

const MAX_SLUG_LENGTH: usize = 50;

/// Converts a title to a URL-friendly slug: lowercase, non-alphanumeric
/// replaced with hyphens, max 50 chars truncated at a word boundary.
pub fn generate_slug(title: &str) -> String {
    let lower = title.to_lowercase();
    // Replace runs of non-alphanumeric characters with a single hyphen.
    let mut slug = String::with_capacity(lower.len());
    let mut prev_hyphen = true; // avoid leading hyphen
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            prev_hyphen = false;
        } else if !prev_hyphen {
            slug.push('-');
            prev_hyphen = true;
        }
    }
    // Trim trailing hyphen.
    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.len() > MAX_SLUG_LENGTH {
        let truncated = &slug[..MAX_SLUG_LENGTH];
        // Only trim to last hyphen if we cut mid-word.
        let next_byte = slug.as_bytes().get(MAX_SLUG_LENGTH).copied().unwrap_or(b'-');
        let result = if next_byte != b'-' {
            if let Some(idx) = truncated.rfind('-') {
                if idx > 0 {
                    &truncated[..idx]
                } else {
                    truncated
                }
            } else {
                truncated
            }
        } else {
            truncated
        };
        result.trim_end_matches('-').to_string()
    } else {
        slug
    }
}

/// Creates a task filename from an ID and slug: `NNN-slug.md`.
pub fn generate_filename(id: i32, slug: &str) -> String {
    let id_str = id.to_string();
    let pad_width = std::cmp::max(3, id_str.len());
    format!("{:0>width$}-{}.md", id, slug, width = pad_width)
}

// ---------------------------------------------------------------------------
// Section parsing
// ---------------------------------------------------------------------------

/// A named section within a task body, delimited by `##` headings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub name: String,
    pub body: String,
}

/// Splits a task body into sections delimited by `##` headings.
/// Content before the first `##` heading is returned with an empty name.
pub fn parse_sections(body: &str) -> Vec<Section> {
    if body.is_empty() {
        return Vec::new();
    }

    let mut sections: Vec<Section> = Vec::new();
    let mut current_name = String::new();
    let mut current_lines: Vec<&str> = Vec::new();

    for line in body.split('\n') {
        if let Some(name) = parse_section_heading(line) {
            flush_section(&mut sections, &current_name, &current_lines);
            current_name = name;
            current_lines.clear();
        } else {
            current_lines.push(line);
        }
    }
    flush_section(&mut sections, &current_name, &current_lines);
    sections
}

/// Extracts a single named section from the body (case-insensitive).
pub fn get_section(body: &str, name: &str) -> Option<String> {
    parse_sections(body)
        .into_iter()
        .find(|s| s.name.eq_ignore_ascii_case(name))
        .map(|s| s.body)
}

/// Creates or replaces a named section in the body (case-insensitive match).
pub fn set_section(body: &str, name: &str, content: &str) -> String {
    let mut sections = parse_sections(body);

    let mut found = false;
    for s in &mut sections {
        if s.name.eq_ignore_ascii_case(name) {
            s.body = content.to_string();
            found = true;
            break;
        }
    }

    if !found {
        sections.push(Section {
            name: name.to_string(),
            body: content.to_string(),
        });
    }

    render_sections(&sections)
}

fn parse_section_heading(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("## ") {
        let name = rest.trim();
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    None
}

fn flush_section(sections: &mut Vec<Section>, name: &str, lines: &[&str]) {
    let joined = lines.join("\n");
    let body = joined.trim_end_matches('\n').trim_start_matches('\n');

    // Skip completely empty unnamed preamble sections.
    if name.is_empty() && body.is_empty() {
        return;
    }

    sections.push(Section {
        name: name.to_string(),
        body: body.to_string(),
    });
}

fn render_sections(sections: &[Section]) -> String {
    let mut buf = String::new();
    for (i, s) in sections.iter().enumerate() {
        if i > 0 {
            buf.push_str("\n\n");
        }
        if !s.name.is_empty() {
            buf.push_str("## ");
            buf.push_str(&s.name);
            buf.push('\n');
        }
        if !s.body.is_empty() {
            buf.push_str(&s.body);
        }
    }
    buf
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

const TASK_FILE_EXT: &str = ".md";

/// Splits a markdown file into YAML frontmatter and body string.
fn split_frontmatter(data: &str) -> Result<(&str, String), TaskError> {
    if !data.starts_with("---\n") {
        return Err(TaskError::Parse(
            "file does not start with YAML frontmatter (---)".to_string(),
        ));
    }

    let rest = &data[4..]; // skip opening "---\n"

    let idx = if let Some(i) = rest.find("\n---\n") {
        i
    } else if rest.ends_with("\n---") {
        rest.len() - 3
    } else {
        return Err(TaskError::Parse(
            "unclosed frontmatter (missing closing ---)".to_string(),
        ));
    };

    let fm = &rest[..idx];
    let closing_end = idx + "\n---\n".len();
    let body = if closing_end < rest.len() {
        rest[closing_end..].trim_start_matches('\n').to_string()
    } else {
        String::new()
    };

    Ok((fm, body))
}

/// Reads a task file, parsing YAML frontmatter and body.
pub fn read(path: &Path) -> Result<Task, TaskError> {
    let data = fs::read_to_string(path)?;
    let (fm, body) = split_frontmatter(&data)?;

    let mut task: Task = serde_yml::from_str(fm).map_err(|e| TaskError::Yaml {
        path: path.display().to_string(),
        source: e,
    })?;
    validate_required_fields(&task)?;

    task.body = body;
    task.file = path.to_string_lossy().into_owned();
    Ok(task)
}

/// Writes a task to a markdown file with YAML frontmatter.
pub fn write(path: &Path, task: &Task) -> Result<(), TaskError> {
    let fm = serde_yml::to_string(task).map_err(|e| TaskError::Yaml {
        path: path.display().to_string(),
        source: e,
    })?;

    let mut buf = String::new();
    buf.push_str("---\n");
    buf.push_str(&fm);
    // serde_yml to_string already appends a trailing newline.
    buf.push_str("---\n");
    if !task.body.is_empty() {
        buf.push('\n');
        buf.push_str(&task.body);
        if !task.body.ends_with('\n') {
            buf.push('\n');
        }
    }

    fs::write(path, buf)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Find helpers
// ---------------------------------------------------------------------------

/// Scans the tasks directory for a file matching the given ID.
///
/// Uses a two-phase strategy optimized for the common case where the
/// filename prefix matches the task ID:
///
/// 1. **Filename-prefix pass**: iterate all entries and only read files whose
///    `NNN-` prefix matches the target ID.  If the frontmatter confirms the
///    ID, return immediately (single file read).
/// 2. **Frontmatter fallback pass**: only executed when the first pass found
///    no confirmed match.  Reads files whose prefix did *not* match (skipping
///    those already examined) to handle the rare renamed-file case.
pub fn find_by_id(tasks_dir: &Path, id: i32) -> Result<PathBuf, TaskError> {
    let entries = match fs::read_dir(tasks_dir) {
        Ok(e) => e,
        Err(e) => return Err(TaskError::Io(e)),
    };

    let id_str = id.to_string();
    let mut prefix_fallback: Option<PathBuf> = None;

    let mut dir_entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    dir_entries.sort_by_key(|e| e.file_name());

    // Track indices of entries already examined in the prefix pass so we can
    // skip them during the fallback pass.
    let mut examined = HashSet::new();

    // Phase 1: match by filename prefix — only reads files whose NNN- prefix
    // equals the target ID.
    for (idx, entry) in dir_entries.iter().enumerate() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(true) {
            continue;
        }
        if !name_str.ends_with(TASK_FILE_EXT) {
            continue;
        }

        if let Some(dash_pos) = name_str.find('-') {
            if dash_pos < 1 {
                continue;
            }
            let prefix = name_str[..dash_pos].trim_start_matches('0');
            if prefix != id_str {
                continue;
            }

            examined.insert(idx);
            let path = tasks_dir.join(&*name_str);
            match read(&path) {
                Ok(t) if t.id == id => return Ok(path),
                Ok(_) => {}
                Err(_) => {
                    if prefix_fallback.is_none() {
                        prefix_fallback = Some(path);
                    }
                }
            }
        }
    }

    // Phase 2: fallback — check frontmatter ID for files not yet examined.
    // This handles the rare case where a file was renamed but still contains
    // the target ID in its frontmatter.
    for (idx, entry) in dir_entries.iter().enumerate() {
        if examined.contains(&idx) {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(true) {
            continue;
        }
        if !name_str.ends_with(TASK_FILE_EXT) {
            continue;
        }

        let path = tasks_dir.join(&*name_str);
        if let Ok(t) = read(&path) {
            if t.id == id {
                return Ok(path);
            }
        }
    }

    if let Some(path) = prefix_fallback {
        return Ok(path);
    }

    Err(TaskError::NotFound(id))
}

/// Reads all task files from the given directory (strict -- fails on any parse error).
pub fn read_all(tasks_dir: &Path) -> Result<Vec<Task>, TaskError> {
    let entries = match fs::read_dir(tasks_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(TaskError::Io(e)),
    };

    let mut dir_entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    dir_entries.sort_by_key(|e| e.file_name());

    let mut tasks = Vec::new();
    for entry in dir_entries {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(true) {
            continue;
        }
        if !name_str.ends_with(TASK_FILE_EXT) {
            continue;
        }

        let path = tasks_dir.join(&*name_str);
        let task = read(&path).map_err(|e| {
            TaskError::Parse(format!("reading {}: {}", name_str, e))
        })?;
        tasks.push(task);
    }

    Ok(tasks)
}

/// Warning for a file that could not be parsed during lenient reading.
#[derive(Debug, Clone)]
pub struct ReadWarning {
    pub file: String,
    pub err: String,
}

impl fmt::Display for ReadWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.file, self.err)
    }
}

/// Reads all task files, skipping malformed files instead of aborting.
pub fn read_all_lenient(tasks_dir: &Path) -> Result<(Vec<Task>, Vec<ReadWarning>), TaskError> {
    let entries = match fs::read_dir(tasks_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Vec::new(), Vec::new()));
        }
        Err(e) => return Err(TaskError::Io(e)),
    };

    let mut dir_entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    dir_entries.sort_by_key(|e| e.file_name());

    let mut tasks = Vec::new();
    let mut warnings = Vec::new();

    for entry in dir_entries {
        let name = entry.file_name();
        let name_str = name.to_string_lossy().to_string();
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(true) {
            continue;
        }
        if !name_str.ends_with(TASK_FILE_EXT) {
            continue;
        }

        let path = tasks_dir.join(&name_str);
        match read(&path) {
            Ok(task) => tasks.push(task),
            Err(e) => warnings.push(ReadWarning {
                file: name_str,
                err: e.to_string(),
            }),
        }
    }

    Ok((tasks, warnings))
}

/// Extracts the numeric ID prefix from a task filename (e.g. `"003-foo.md"` -> `3`).
pub fn extract_id_from_filename(filename: &str) -> Option<i32> {
    let dash = filename.find('-')?;
    if dash < 1 {
        return None;
    }
    filename[..dash].parse::<i32>().ok()
}

/// Returns the highest task ID found by scanning filenames. Returns 0 if empty.
pub fn max_id_from_files(tasks_dir: &Path) -> Result<i32, TaskError> {
    let entries = match fs::read_dir(tasks_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(TaskError::Io(e)),
    };

    let mut max_id: i32 = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(true) {
            continue;
        }
        if !name_str.ends_with(TASK_FILE_EXT) {
            continue;
        }
        if let Some(id) = extract_id_from_filename(&name_str) {
            if id > max_id {
                max_id = id;
            }
        }
    }

    Ok(max_id)
}

// ---------------------------------------------------------------------------
// Consistency checking
// ---------------------------------------------------------------------------

/// Summary of consistency warnings and repairs.
#[derive(Debug, Clone, Default)]
pub struct ConsistencyReport {
    pub warnings: Vec<ReadWarning>,
    pub repairs: Vec<String>,
}

/// Checks tasks for ID/filename inconsistencies and repairs them in place.
/// Also advances `next_id` to avoid future collisions.
pub fn ensure_consistency(cfg: &mut Config) -> Result<ConsistencyReport, TaskError> {
    let tasks_path = cfg.tasks_path();
    let (mut tasks, warnings) = read_all_lenient(&tasks_path)?;

    let mut report = ConsistencyReport {
        warnings,
        ..Default::default()
    };

    if tasks.is_empty() {
        return Ok(report);
    }

    // Sort by file path for deterministic processing.
    tasks.sort_by(|a, b| a.file.cmp(&b.file));

    let (mut used_ids, mut next_id) = initialize_id_state(&tasks, cfg.next_id);

    // Repair duplicate IDs.
    let duplicate_ids = find_duplicate_ids(&tasks);
    for dup_id in &duplicate_ids {
        let group_indices: Vec<usize> = tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.id == *dup_id)
            .map(|(i, _)| i)
            .collect();

        let keeper = select_duplicate_keeper(&tasks, &group_indices, *dup_id);

        for &idx in &group_indices {
            if idx == keeper {
                continue;
            }
            let new_id = next_available_id(next_id, &mut used_ids);
            next_id = new_id + 1;
            let old_id = tasks[idx].id;
            tasks[idx].id = new_id;
            tasks[idx].updated = Utc::now();

            let base = Path::new(&tasks[idx].file)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            report.repairs.push(format!(
                "reassigned duplicate ID {} in {} to {}",
                old_id, base, new_id
            ));
        }
    }

    // Repair filename mismatches.
    let occupied = occupied_task_paths(&tasks_path)?;
    let mut occupied_set = occupied;

    // Re-sort after potential ID changes.
    tasks.sort_by(|a, b| a.file.cmp(&b.file));

    for task in &mut tasks {
        let file_id = extract_id_from_filename(
            Path::new(&task.file)
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or(""),
        );
        let needs_repair = match file_id {
            Some(fid) => fid != task.id,
            None => true,
        };

        if !needs_repair {
            continue;
        }

        let old_path = PathBuf::from(&task.file);
        let old_name = old_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let target_path = choose_task_path(&tasks_path, task, &old_path, &occupied_set);
        task.file = target_path.to_string_lossy().to_string();
        task.updated = Utc::now();

        write(&target_path, task)?;
        if old_path != target_path {
            let _ = fs::remove_file(&old_path);
        }

        occupied_set.remove(&old_path);
        occupied_set.insert(target_path.clone());

        let new_name = target_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        report.repairs.push(format!(
            "renamed {} to {} to match task ID {}",
            old_name, new_name, task.id
        ));
    }

    // Sync next_id.
    let max_id = tasks.iter().map(|t| t.id).max().unwrap_or(0);
    let mut desired_next = cfg.next_id;
    if desired_next <= max_id {
        desired_next = max_id + 1;
    }
    if next_id > desired_next {
        desired_next = next_id;
    }
    if desired_next != cfg.next_id {
        let old_next = cfg.next_id;
        cfg.next_id = desired_next;
        report.repairs.push(format!(
            "updated next_id from {} to {}",
            old_next, desired_next
        ));
    }

    Ok(report)
}

fn initialize_id_state(tasks: &[Task], cfg_next_id: i32) -> (HashSet<i32>, i32) {
    let mut used_ids = HashSet::with_capacity(tasks.len());
    let mut max_id = 0;
    for t in tasks {
        used_ids.insert(t.id);
        if t.id > max_id {
            max_id = t.id;
        }
    }
    let next_id = std::cmp::max(cfg_next_id, max_id + 1);
    (used_ids, next_id)
}

fn find_duplicate_ids(tasks: &[Task]) -> Vec<i32> {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for t in tasks {
        *counts.entry(t.id).or_insert(0) += 1;
    }
    let mut ids: Vec<i32> = counts
        .into_iter()
        .filter(|&(_, count)| count > 1)
        .map(|(id, _)| id)
        .collect();
    ids.sort();
    ids
}

fn select_duplicate_keeper(tasks: &[Task], group_indices: &[usize], id: i32) -> usize {
    for &idx in group_indices {
        let file_id = extract_id_from_filename(
            Path::new(&tasks[idx].file)
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or(""),
        );
        if file_id == Some(id) {
            return idx;
        }
    }
    group_indices[0]
}

fn next_available_id(start: i32, used: &mut HashSet<i32>) -> i32 {
    let mut id = start;
    while used.contains(&id) {
        id += 1;
    }
    used.insert(id);
    id
}

fn occupied_task_paths(tasks_dir: &Path) -> Result<HashSet<PathBuf>, TaskError> {
    let entries = match fs::read_dir(tasks_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashSet::new()),
        Err(e) => return Err(TaskError::Io(e)),
    };

    let mut set = HashSet::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(true) {
            continue;
        }
        if !name_str.ends_with(TASK_FILE_EXT) {
            continue;
        }
        set.insert(tasks_dir.join(&*name_str));
    }
    Ok(set)
}

fn choose_task_path(
    tasks_dir: &Path,
    task: &Task,
    current_path: &Path,
    occupied: &HashSet<PathBuf>,
) -> PathBuf {
    let slug = {
        let s = generate_slug(&task.title);
        if s.is_empty() {
            "task".to_string()
        } else {
            s
        }
    };
    let base = generate_filename(task.id, &slug);
    let candidate = tasks_dir.join(&base);
    if candidate == current_path || !occupied.contains(&candidate) {
        return candidate;
    }
    for i in 1.. {
        let name = format!(
            "{:03}-{}-{}{}",
            task.id, slug, i, TASK_FILE_EXT
        );
        let candidate = tasks_dir.join(&name);
        if candidate == current_path || !occupied.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_slug_basic() {
        assert_eq!(generate_slug("Hello World"), "hello-world");
    }

    #[test]
    fn test_generate_slug_special_chars() {
        assert_eq!(
            generate_slug("Fix: the bug (urgent!)"),
            "fix-the-bug-urgent"
        );
    }

    #[test]
    fn test_generate_slug_truncation() {
        let long = "this is a very long title that should be truncated at word boundary to fifty chars";
        let slug = generate_slug(long);
        assert!(slug.len() <= MAX_SLUG_LENGTH);
        assert!(!slug.ends_with('-'));
    }

    #[test]
    fn test_generate_filename() {
        assert_eq!(generate_filename(1, "hello-world"), "001-hello-world.md");
        assert_eq!(generate_filename(42, "foo"), "042-foo.md");
        assert_eq!(generate_filename(1000, "bar"), "1000-bar.md");
    }

    #[test]
    fn test_extract_id_from_filename() {
        assert_eq!(extract_id_from_filename("003-hello.md"), Some(3));
        assert_eq!(extract_id_from_filename("42-foo.md"), Some(42));
        assert_eq!(extract_id_from_filename("bad.md"), None);
    }

    #[test]
    fn test_parse_sections_empty() {
        assert!(parse_sections("").is_empty());
    }

    #[test]
    fn test_parse_sections_with_headings() {
        let body = "preamble\n\n## Notes\nsome notes\n\n## Log\nentry 1";
        let sections = parse_sections(body);
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].name, "");
        assert_eq!(sections[0].body, "preamble");
        assert_eq!(sections[1].name, "Notes");
        assert!(sections[1].body.contains("some notes"));
        assert_eq!(sections[2].name, "Log");
    }

    #[test]
    fn test_get_section() {
        let body = "## Notes\nhello\n\n## Log\nworld";
        assert_eq!(get_section(body, "Notes"), Some("hello".to_string()));
        assert_eq!(get_section(body, "log"), Some("world".to_string()));
        assert_eq!(get_section(body, "missing"), None);
    }

    #[test]
    fn test_set_section_replace() {
        let body = "## Notes\nold\n\n## Log\nentry";
        let result = set_section(body, "Notes", "new");
        assert!(result.contains("new"));
        assert!(!result.contains("old"));
        assert!(result.contains("entry"));
    }

    #[test]
    fn test_set_section_append() {
        let body = "## Notes\nhello";
        let result = set_section(body, "Log", "world");
        assert!(result.contains("## Notes"));
        assert!(result.contains("## Log"));
        assert!(result.contains("world"));
    }

    #[test]
    fn test_task_default() {
        let t = Task::default();
        assert_eq!(t.id, 0);
        assert!(t.title.is_empty());
        assert!(t.tags.is_empty());
        assert!(!t.blocked);
    }

    #[test]
    fn test_split_frontmatter() {
        let data = "---\nid: 1\ntitle: hello\n---\n\nSome body text\n";
        let (fm, body) = split_frontmatter(data).unwrap();
        assert!(fm.contains("id: 1"));
        assert_eq!(body, "Some body text\n");
    }

    #[test]
    fn test_split_frontmatter_no_body() {
        let data = "---\nid: 1\n---\n";
        let (fm, body) = split_frontmatter(data).unwrap();
        assert!(fm.contains("id: 1"));
        assert!(body.is_empty());
    }

    #[test]
    fn test_split_frontmatter_error() {
        let data = "no frontmatter";
        assert!(split_frontmatter(data).is_err());
    }

    // -----------------------------------------------------------------------
    // find_by_id tests
    // -----------------------------------------------------------------------

    /// Helper: write a minimal valid task file to disk.
    fn write_task_file(dir: &Path, filename: &str, id: i32, title: &str) {
        let now = Utc::now().to_rfc3339();
        let content = format!(
            "---\nid: {}\ntitle: {}\nstatus: todo\npriority: medium\ncreated: {}\nupdated: {}\n---\n",
            id, title, now, now
        );
        fs::write(dir.join(filename), content).unwrap();
    }

    #[test]
    fn test_find_by_id_filename_prefix_match() {
        let dir = tempfile::tempdir().unwrap();
        write_task_file(dir.path(), "007-implement-search.md", 7, "Implement search");
        write_task_file(dir.path(), "012-other-task.md", 12, "Other task");

        let result = find_by_id(dir.path(), 7).unwrap();
        assert_eq!(result.file_name().unwrap(), "007-implement-search.md");
    }

    #[test]
    fn test_find_by_id_renamed_file_frontmatter_fallback() {
        let dir = tempfile::tempdir().unwrap();
        // Filename says 099 but frontmatter says id: 5 — simulates a renamed file.
        write_task_file(dir.path(), "099-renamed-task.md", 5, "Renamed task");
        write_task_file(dir.path(), "010-normal.md", 10, "Normal task");

        let result = find_by_id(dir.path(), 5).unwrap();
        assert_eq!(result.file_name().unwrap(), "099-renamed-task.md");
    }

    #[test]
    fn test_find_by_id_not_found() {
        let dir = tempfile::tempdir().unwrap();
        write_task_file(dir.path(), "001-only-task.md", 1, "Only task");

        let result = find_by_id(dir.path(), 999);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), TaskError::NotFound(999)),
            "expected NotFound(999)"
        );
    }
}
