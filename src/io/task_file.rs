//! Task file I/O: reading and writing markdown files with YAML frontmatter.
//!
//! Task files follow this format:
//! ```text
//! ---
//! id: 1
//! title: My Task
//! status: todo
//! priority: medium
//! created: 2024-01-15T10:30:00Z
//! updated: 2024-01-15T10:30:00Z
//! tags:
//!   - feature
//! ---
//!
//! Task body content here in Markdown.
//! ```

use std::fs;
use std::io::Write;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use crate::error::{CliError, ErrorCode};
use crate::model::task::Task;

const FILE_MODE: u32 = 0o600;

/// Read a task file, parsing YAML frontmatter and body.
pub fn read(path: &Path) -> Result<Task, CliError> {
    let data = fs::read_to_string(path).map_err(|e| {
        CliError::newf(
            ErrorCode::InternalError,
            format!("reading task file: {}", e),
        )
    })?;

    let (fm, body) = split_frontmatter(&data).map_err(|e| {
        CliError::newf(
            ErrorCode::InvalidInput,
            format!("parsing {}: {}", path.display(), e),
        )
    })?;

    let mut task: Task = serde_yml::from_str(fm).map_err(|e| {
        CliError::newf(
            ErrorCode::InvalidInput,
            format!("parsing frontmatter in {}: {}", path.display(), e),
        )
    })?;

    validate_required_fields(&task).map_err(|e| {
        CliError::newf(
            ErrorCode::InvalidInput,
            format!("parsing frontmatter in {}: {}", path.display(), e),
        )
    })?;

    task.body = body.trim_end().to_string();
    task.file = path.to_string_lossy().to_string();

    Ok(task)
}

/// Write a task to a file with YAML frontmatter and body.
///
/// The body and file fields are excluded from YAML serialization via `#[serde(skip)]`
/// on the Task struct. File is written with mode 0o600 on Unix.
pub fn write(path: &Path, task: &Task) -> Result<(), CliError> {
    // Clone and clear body/file so they don't appear in YAML frontmatter.
    // These fields use skip_serializing_if="is_empty_string" on the struct
    // so they're included in JSON output but excluded from YAML when empty.
    let mut yaml_task = task.clone();
    yaml_task.body.clear();
    yaml_task.file.clear();

    let fm = serde_yml::to_string(&yaml_task).map_err(|e| {
        CliError::newf(
            ErrorCode::InternalError,
            format!("marshaling frontmatter: {}", e),
        )
    })?;

    let mut buf = String::new();
    buf.push_str("---\n");
    buf.push_str(&fm);
    // serde_yml::to_string output does not include "---" delimiters and ends with "\n".
    buf.push_str("---\n");

    if !task.body.is_empty() {
        buf.push('\n');
        buf.push_str(&task.body);
        if !task.body.ends_with('\n') {
            buf.push('\n');
        }
    }

    write_with_mode(path, buf.as_bytes())
}

/// Split a file into YAML frontmatter string and body string.
///
/// Rules:
/// 1. File must start with `"---\n"`
/// 2. Find closing `"\n---\n"` (or `"\n---"` at EOF)
/// 3. Everything between is YAML frontmatter
/// 4. Everything after closing `---` (trimming leading newlines) is the body
fn split_frontmatter(data: &str) -> Result<(&str, &str), String> {
    if !data.starts_with("---\n") {
        return Err("file does not start with YAML frontmatter (---)".into());
    }

    let rest = &data[4..]; // skip opening "---\n"

    // Find the closing "---".
    let idx = if let Some(pos) = rest.find("\n---\n") {
        pos
    } else if rest.ends_with("\n---") {
        rest.len() - 4
    } else {
        return Err("unclosed frontmatter (missing closing ---)".into());
    };

    let fm = &rest[..idx];

    // Body is everything after the closing "---\n".
    let closing_end = idx + "\n---\n".len();
    let body = if closing_end <= rest.len() {
        rest[closing_end..].trim_start_matches('\n')
    } else {
        ""
    };

    Ok((fm, body))
}

/// Validate required fields: id > 0, title non-empty, status non-empty.
fn validate_required_fields(task: &Task) -> Result<(), String> {
    if task.id < 1 {
        return Err("missing required field: id".into());
    }
    if task.title.trim().is_empty() {
        return Err("missing required field: title".into());
    }
    if task.status.trim().is_empty() {
        return Err("missing required field: status".into());
    }
    Ok(())
}

/// Write bytes to a file with mode 0o600 on Unix.
fn write_with_mode(path: &Path, data: &[u8]) -> Result<(), CliError> {
    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                CliError::newf(
                    ErrorCode::InternalError,
                    format!("creating directory: {}", e),
                )
            })?;
        }
    }

    #[cfg(unix)]
    {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(FILE_MODE)
            .open(path)
            .map_err(|e| {
                CliError::newf(
                    ErrorCode::InternalError,
                    format!("writing task file: {}", e),
                )
            })?;
        file.write_all(data).map_err(|e| {
            CliError::newf(
                ErrorCode::InternalError,
                format!("writing task file: {}", e),
            )
        })?;
    }

    #[cfg(not(unix))]
    {
        fs::write(path, data).map_err(|e| {
            CliError::newf(
                ErrorCode::InternalError,
                format!("writing task file: {}", e),
            )
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use std::fs as stdfs;

    fn sample_task() -> Task {
        Task {
            id: 1,
            title: "Test task".into(),
            status: "todo".into(),
            priority: "medium".into(),
            created: Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap(),
            updated: Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap(),
            tags: vec!["feature".into()],
            body: "Task body content here.".into(),
            ..Default::default()
        }
    }

    #[test]
    fn test_split_frontmatter_basic() {
        let data = "---\nid: 1\ntitle: Test\n---\n\nBody here.\n";
        let (fm, body) = split_frontmatter(data).unwrap();
        assert_eq!(fm, "id: 1\ntitle: Test");
        assert_eq!(body, "Body here.\n");
    }

    #[test]
    fn test_split_frontmatter_no_body() {
        let data = "---\nid: 1\ntitle: Test\n---\n";
        let (fm, body) = split_frontmatter(data).unwrap();
        assert_eq!(fm, "id: 1\ntitle: Test");
        assert_eq!(body, "");
    }

    #[test]
    fn test_split_frontmatter_eof_closing() {
        let data = "---\nid: 1\ntitle: Test\n---";
        let (fm, body) = split_frontmatter(data).unwrap();
        assert_eq!(fm, "id: 1\ntitle: Test");
        assert_eq!(body, "");
    }

    #[test]
    fn test_split_frontmatter_no_opening() {
        let data = "id: 1\ntitle: Test\n---\n";
        assert!(split_frontmatter(data).is_err());
    }

    #[test]
    fn test_split_frontmatter_unclosed() {
        let data = "---\nid: 1\ntitle: Test\n";
        assert!(split_frontmatter(data).is_err());
    }

    #[test]
    fn test_split_frontmatter_strips_leading_newlines_from_body() {
        let data = "---\nid: 1\n---\n\n\n\nBody.\n";
        let (_, body) = split_frontmatter(data).unwrap();
        assert_eq!(body, "Body.\n");
    }

    #[test]
    fn test_validate_required_fields_ok() {
        let task = sample_task();
        assert!(validate_required_fields(&task).is_ok());
    }

    #[test]
    fn test_validate_required_fields_missing_id() {
        let mut task = sample_task();
        task.id = 0;
        let err = validate_required_fields(&task).unwrap_err();
        assert!(err.contains("id"));
    }

    #[test]
    fn test_validate_required_fields_missing_title() {
        let mut task = sample_task();
        task.title = "   ".into();
        let err = validate_required_fields(&task).unwrap_err();
        assert!(err.contains("title"));
    }

    #[test]
    fn test_validate_required_fields_missing_status() {
        let mut task = sample_task();
        task.status = "".into();
        let err = validate_required_fields(&task).unwrap_err();
        assert!(err.contains("status"));
    }

    #[test]
    fn test_roundtrip_write_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("task-1.md");

        let task = sample_task();
        write(&path, &task).unwrap();

        let loaded = read(&path).unwrap();
        assert_eq!(loaded.id, 1);
        assert_eq!(loaded.title, "Test task");
        assert_eq!(loaded.status, "todo");
        assert_eq!(loaded.priority, "medium");
        assert_eq!(loaded.tags, vec!["feature"]);
        assert_eq!(loaded.body, "Task body content here.");
        assert_eq!(loaded.file, path.to_string_lossy().as_ref());
    }

    #[test]
    fn test_roundtrip_no_body() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("task-2.md");

        let mut task = sample_task();
        task.body = String::new();
        write(&path, &task).unwrap();

        let loaded = read(&path).unwrap();
        assert!(loaded.body.is_empty());
    }

    #[test]
    fn test_write_body_no_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("task-3.md");

        let mut task = sample_task();
        task.body = "No trailing newline".into();
        write(&path, &task).unwrap();

        let content = stdfs::read_to_string(&path).unwrap();
        assert!(content.ends_with('\n'), "file should end with newline");
    }

    #[test]
    fn test_write_body_with_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("task-4.md");

        let mut task = sample_task();
        task.body = "Has trailing newline\n".into();
        write(&path, &task).unwrap();

        let content = stdfs::read_to_string(&path).unwrap();
        // Should not double the trailing newline.
        assert!(content.ends_with("Has trailing newline\n"));
        assert!(!content.ends_with("Has trailing newline\n\n"));
    }

    #[cfg(unix)]
    #[test]
    fn test_write_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("task-5.md");

        let task = sample_task();
        write(&path, &task).unwrap();

        let metadata = stdfs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, FILE_MODE, "file should have mode 0o600");
    }

    #[test]
    fn test_write_skips_body_and_file_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("task-6.md");

        let mut task = sample_task();
        task.body = "The body".into();
        task.file = "/some/path.md".into();
        write(&path, &task).unwrap();

        let content = stdfs::read_to_string(&path).unwrap();
        // The YAML frontmatter should not contain "body:" or "file:" keys.
        let (fm, _) = split_frontmatter(&content).unwrap();
        assert!(!fm.contains("body:"), "frontmatter should not contain body");
        assert!(!fm.contains("file:"), "frontmatter should not contain file");
    }

    #[test]
    fn test_read_nonexistent_file() {
        let path = Path::new("/tmp/nonexistent-task-file-12345.md");
        assert!(read(path).is_err());
    }

    #[test]
    fn test_read_invalid_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.md");
        stdfs::write(&path, "---\n[invalid yaml\n---\n").unwrap();
        assert!(read(&path).is_err());
    }

    #[test]
    fn test_read_missing_required_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no-id.md");
        stdfs::write(
            &path,
            "---\ntitle: Test\nstatus: todo\npriority: medium\ncreated: 2024-01-15T10:30:00Z\nupdated: 2024-01-15T10:30:00Z\n---\n",
        )
        .unwrap();
        let err = read(&path).unwrap_err();
        assert!(err.to_string().contains("id"));
    }

    #[test]
    fn test_read_all_optional_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("full.md");
        let content = "\
---
id: 42
title: Full task
status: in-progress
priority: high
created: 2024-06-01T08:00:00Z
updated: 2024-06-15T12:00:00Z
started: 2024-06-02T09:00:00Z
assignee: alice
tags:
  - bug
  - urgent
estimate: 2h
parent: 10
depends_on:
  - 5
  - 7
blocked: true
block_reason: Waiting on API
claimed_by: agent-fox
claimed_at: 2024-06-15T10:00:00Z
class: expedite
branch: task/42-full-task
worktree: ../kbmdx-task-42
---

This is the full body.
";
        stdfs::write(&path, content).unwrap();

        let task = read(&path).unwrap();
        assert_eq!(task.id, 42);
        assert_eq!(task.title, "Full task");
        assert_eq!(task.status, "in-progress");
        assert_eq!(task.priority, "high");
        assert_eq!(task.assignee, "alice");
        assert_eq!(task.tags, vec!["bug", "urgent"]);
        assert_eq!(task.estimate, "2h");
        assert_eq!(task.parent, Some(10));
        assert_eq!(task.depends_on, vec![5, 7]);
        assert!(task.blocked);
        assert_eq!(task.block_reason, "Waiting on API");
        assert_eq!(task.claimed_by, "agent-fox");
        assert!(task.claimed_at.is_some());
        assert_eq!(task.class, "expedite");
        assert_eq!(task.branch, "task/42-full-task");
        assert_eq!(task.worktree, "../kbmdx-task-42");
        assert_eq!(task.body, "This is the full body.");
    }
}
