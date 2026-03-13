//! Activity log for tracking board mutations.
//!
//! Uses JSONL format (one JSON object per line). File: "activity.jsonl".
//! Max 10,000 entries with automatic truncation.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const LOG_FILE_NAME: &str = "activity.jsonl";
const MAX_LOG_ENTRIES: usize = 10_000;

/// A logged mutation event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub task_id: i32,
    #[serde(alias = "details")]
    pub detail: String,
}

/// Controls how log entries are filtered when reading.
#[derive(Default)]
pub struct LogFilterOptions {
    /// Only include entries at or after this timestamp.
    pub since: Option<DateTime<Utc>>,
    /// Maximum number of entries to return (most recent).
    pub limit: Option<usize>,
    /// Only include entries with this action.
    pub action: Option<String>,
    /// Only include entries for this task ID.
    pub task_id: Option<i32>,
}

/// Append a log entry to the activity log file (JSONL format).
///
/// If the log exceeds `MAX_LOG_ENTRIES`, the oldest entries are truncated.
pub fn append_log(
    kanban_dir: &Path,
    entry: &LogEntry,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = kanban_dir.join(LOG_FILE_NAME);

    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)?;

    let data = serde_json::to_string(entry)?;
    writeln!(f, "{}", data)?;

    // Truncate if needed (best-effort; errors are non-fatal).
    let _ = truncate_log_if_needed(&path);

    Ok(())
}

/// Append a log entry to a specific log file path.
///
/// This is the simplified API that writes to an explicit path.
pub fn append(log_path: &Path, entry: &LogEntry) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(log_path)?;

    let data = serde_json::to_string(entry)?;
    writeln!(f, "{}", data)?;
    Ok(())
}

/// Load all log entries from a specific log file path.
pub fn load(log_path: &Path) -> Result<Vec<LogEntry>, Box<dyn std::error::Error>> {
    if !log_path.exists() {
        return Ok(Vec::new());
    }

    let f = std::fs::File::open(log_path)?;
    let reader = BufReader::new(f);
    let mut entries = Vec::new();

    for line_result in reader.lines() {
        let line = line_result?;
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<LogEntry>(&line) {
            Ok(entry) => entries.push(entry),
            Err(_) => {
                // Try parsing as JSON array format (backward compat)
                if let Ok(arr) = serde_json::from_str::<Vec<LogEntry>>(&line) {
                    entries.extend(arr);
                }
                // Skip malformed lines
            }
        }
    }

    Ok(entries)
}

/// Reads and filters log entries from the activity log file.
pub fn read_log(
    kanban_dir: &Path,
    opts: &LogFilterOptions,
) -> Result<Vec<LogEntry>, Box<dyn std::error::Error>> {
    let path = kanban_dir.join(LOG_FILE_NAME);

    let f = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };

    let reader = BufReader::new(f);
    let mut entries = Vec::new();

    for line_result in reader.lines() {
        let line = line_result?;
        if line.is_empty() {
            continue;
        }

        let entry: LogEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue, // skip malformed lines
        };

        if !matches_log_filter(&entry, opts) {
            continue;
        }

        entries.push(entry);
    }

    // Apply limit (keep most recent)
    if let Some(limit) = opts.limit {
        if limit > 0 && entries.len() > limit {
            entries = entries[entries.len() - limit..].to_vec();
        }
    }

    Ok(entries)
}

/// Convenience wrapper that appends a mutation log entry.
/// Errors are silently discarded because logging should never fail a command.
pub fn log_mutation(kanban_dir: &Path, action: &str, task_id: i32, detail: &str) {
    let entry = LogEntry {
        timestamp: Utc::now(),
        action: action.to_string(),
        task_id,
        detail: detail.to_string(),
    };
    let _ = append_log(kanban_dir, &entry);
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn matches_log_filter(entry: &LogEntry, opts: &LogFilterOptions) -> bool {
    if let Some(ref since) = opts.since {
        if entry.timestamp < *since {
            return false;
        }
    }
    if let Some(ref action) = opts.action {
        if !action.is_empty() && entry.action != *action {
            return false;
        }
    }
    if let Some(task_id) = opts.task_id {
        if task_id > 0 && entry.task_id != task_id {
            return false;
        }
    }
    true
}

fn truncate_log_if_needed(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let f = std::fs::File::open(path)?;
    let reader = BufReader::new(f);

    let mut lines: Vec<String> = Vec::new();
    for line_result in reader.lines() {
        lines.push(line_result?);
    }

    if lines.len() <= MAX_LOG_ENTRIES {
        return Ok(());
    }

    // Keep only the last MAX_LOG_ENTRIES lines.
    let keep = &lines[lines.len() - MAX_LOG_ENTRIES..];
    let mut buf = String::new();
    for line in keep {
        buf.push_str(line);
        buf.push('\n');
    }
    std::fs::write(path, buf)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use tempfile::tempdir;

    fn make_entry(action: &str, task_id: i32) -> LogEntry {
        LogEntry {
            timestamp: Utc::now(),
            action: action.to_string(),
            task_id,
            detail: format!("{} task {}", action, task_id),
        }
    }

    #[test]
    fn test_append_and_read() {
        let dir = tempdir().unwrap();
        let kanban_dir = dir.path();

        append_log(kanban_dir, &make_entry("create", 1)).unwrap();
        append_log(kanban_dir, &make_entry("move", 2)).unwrap();

        let entries = read_log(kanban_dir, &LogFilterOptions::default()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].action, "create");
        assert_eq!(entries[1].action, "move");
    }

    #[test]
    fn test_filter_by_action() {
        let dir = tempdir().unwrap();
        let kanban_dir = dir.path();

        append_log(kanban_dir, &make_entry("create", 1)).unwrap();
        append_log(kanban_dir, &make_entry("move", 2)).unwrap();
        append_log(kanban_dir, &make_entry("move", 3)).unwrap();

        let opts = LogFilterOptions {
            action: Some("move".to_string()),
            ..Default::default()
        };
        let entries = read_log(kanban_dir, &opts).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_filter_by_task_id() {
        let dir = tempdir().unwrap();
        let kanban_dir = dir.path();

        append_log(kanban_dir, &make_entry("create", 1)).unwrap();
        append_log(kanban_dir, &make_entry("move", 2)).unwrap();

        let opts = LogFilterOptions {
            task_id: Some(1),
            ..Default::default()
        };
        let entries = read_log(kanban_dir, &opts).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].task_id, 1);
    }

    #[test]
    fn test_filter_with_limit() {
        let dir = tempdir().unwrap();
        let kanban_dir = dir.path();

        for i in 1..=5 {
            append_log(kanban_dir, &make_entry("create", i)).unwrap();
        }

        let opts = LogFilterOptions {
            limit: Some(2),
            ..Default::default()
        };
        let entries = read_log(kanban_dir, &opts).unwrap();
        assert_eq!(entries.len(), 2);
        // Most recent entries
        assert_eq!(entries[0].task_id, 4);
        assert_eq!(entries[1].task_id, 5);
    }

    #[test]
    fn test_filter_by_since() {
        let dir = tempdir().unwrap();
        let kanban_dir = dir.path();

        let mut old_entry = make_entry("create", 1);
        old_entry.timestamp = Utc::now() - Duration::hours(48);
        append_log(kanban_dir, &old_entry).unwrap();
        append_log(kanban_dir, &make_entry("create", 2)).unwrap();

        let opts = LogFilterOptions {
            since: Some(Utc::now() - Duration::hours(24)),
            ..Default::default()
        };
        let entries = read_log(kanban_dir, &opts).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].task_id, 2);
    }

    #[test]
    fn test_empty_log() {
        let dir = tempdir().unwrap();
        let entries = read_log(dir.path(), &LogFilterOptions::default()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_log_mutation_does_not_fail() {
        let dir = tempdir().unwrap();
        log_mutation(dir.path(), "create", 1, "created task 1");
        let entries = read_log(dir.path(), &LogFilterOptions::default()).unwrap();
        assert_eq!(entries.len(), 1);
    }
}
