//! Shared helpers for CLI command implementations.

use std::path::PathBuf;

use crate::error::{CliError, ErrorCode};
use crate::io::task_file;
use crate::model::config::Config;
use crate::model::task::{self, Task};

/// Parses a task ID string (with optional `#` prefix) into an `i32`.
pub fn parse_task_id(s: &str) -> Result<i32, CliError> {
    s.trim_start_matches('#')
        .parse()
        .map_err(|_| CliError::newf(ErrorCode::InvalidTaskId, format!("invalid task ID: {s}")))
}

/// Parses a `YYYY-MM-DD` date string into a `NaiveDate`.
pub fn parse_date(s: &str) -> Result<chrono::NaiveDate, CliError> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| CliError::newf(ErrorCode::InvalidDate, format!("invalid date: {s}")))
}

/// Loads a task by ID: finds the file path then reads the task.
pub fn load_task(cfg: &Config, id: i32) -> Result<(PathBuf, Task), CliError> {
    let path = task::find_by_id(&cfg.tasks_path(), id)
        .map_err(|e| CliError::newf(ErrorCode::TaskNotFound, format!("{e}")))?;
    let t = task_file::read(&path)
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
    Ok((path, t))
}

/// Writes a task back to disk.
pub fn save_task(path: &std::path::Path, task: &Task) -> Result<(), CliError> {
    task_file::write(path, task)
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("failed to write: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_task_id_plain() {
        assert_eq!(parse_task_id("42").unwrap(), 42);
    }

    #[test]
    fn parse_task_id_with_hash() {
        assert_eq!(parse_task_id("#7").unwrap(), 7);
    }

    #[test]
    fn parse_task_id_invalid() {
        assert!(parse_task_id("abc").is_err());
    }

    #[test]
    fn parse_date_valid() {
        let d = parse_date("2024-03-15").unwrap();
        assert_eq!(d.to_string(), "2024-03-15");
    }

    #[test]
    fn parse_date_invalid() {
        assert!(parse_date("not-a-date").is_err());
    }
}
