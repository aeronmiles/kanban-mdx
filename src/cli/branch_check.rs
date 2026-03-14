//! `kbmdx branch-check` — validate branch setup for the current worktree/branch.
//!
//! Checks whether the current git branch matches the task's expected branch,
//! using both convention matching (task/<ID>-<description>) and exact branch match.

use std::io::Write;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::task;
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct BranchCheckArgs {
    /// Task ID to check
    pub id: String,
    /// Force override (skip enforcement, only warn)
    #[arg(long)]
    pub force: bool,
}

pub fn run(cli: &Cli, args: BranchCheckArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;

    let id: i32 = args.id.trim_start_matches('#').parse().map_err(|_| {
        CliError::newf(
            ErrorCode::InvalidTaskId,
            format!("invalid task ID: {}", args.id),
        )
    })?;

    let file_path = task::find_by_id(&cfg.tasks_path(), id)
        .map_err(|e| CliError::newf(ErrorCode::TaskNotFound, format!("{e}")))?;
    let t = task::read(&file_path)
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;

    let branch = crate::util::git::current_branch();

    let match_result = check_branch_match(id, &t.branch, branch.as_deref());

    // Enforce if the status requires a branch and --force is not set.
    if cfg.status_requires_branch(&t.status) && !args.force {
        if let BranchMatch::Mismatch { task_branch, current } = &match_result {
            return Err(CliError::newf(
                ErrorCode::StatusConflict,
                format!(
                    "task #{id} requires branch match (status {:?} has require_branch); \
                     you're on {current}, task is on {task_branch}. Use --force to override",
                    t.status,
                ),
            ));
        }
    }

    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            let result = serde_json::json!({
                "task_id": id,
                "task_branch": t.branch,
                "current_branch": branch,
                "match": match_result.is_match(),
                "require_branch": cfg.status_requires_branch(&t.status),
            });
            crate::output::json::json(&mut stdout, &result)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        Format::Compact => {
            if match_result.is_match() {
                writeln!(stdout, "#{id} branch ok").unwrap_or(());
            } else {
                writeln!(stdout, "#{id} branch mismatch").unwrap_or(());
            }
        }
        Format::Table => {
            match &match_result {
                BranchMatch::Match => {
                    writeln!(stdout, "Branch check passed for task #{id}").unwrap_or(());
                }
                BranchMatch::NoBranch => {
                    writeln!(stdout, "Not in a git repo or cannot detect branch").unwrap_or(());
                }
                BranchMatch::NoTaskBranch => {
                    writeln!(stdout, "Task #{id} has no branch set").unwrap_or(());
                }
                BranchMatch::Mismatch { task_branch, current } => {
                    writeln!(
                        stdout,
                        "Warning: task #{id} is on branch {task_branch} but you're on {current}",
                    )
                    .unwrap_or(());
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Branch matching logic (ported from Go branch_check.go)
// ---------------------------------------------------------------------------

enum BranchMatch {
    /// Current branch matches the task (by convention or exact match).
    Match,
    /// Cannot detect current branch.
    NoBranch,
    /// Task has no branch set.
    NoTaskBranch,
    /// Branch mismatch.
    Mismatch {
        task_branch: String,
        current: String,
    },
}

impl BranchMatch {
    fn is_match(&self) -> bool {
        matches!(self, BranchMatch::Match | BranchMatch::NoBranch | BranchMatch::NoTaskBranch)
    }
}

fn check_branch_match(task_id: i32, task_branch: &str, current: Option<&str>) -> BranchMatch {
    let current = match current {
        Some(b) if !b.is_empty() => b,
        _ => return BranchMatch::NoBranch,
    };

    // Convention match: task/<ID>-<description>
    if let Some(id) = parse_task_id_branch(current) {
        if id == task_id {
            return BranchMatch::Match;
        }
    }

    // Exact match.
    if !task_branch.is_empty() && task_branch == current {
        return BranchMatch::Match;
    }

    if task_branch.is_empty() {
        return BranchMatch::NoTaskBranch;
    }

    BranchMatch::Mismatch {
        task_branch: task_branch.to_string(),
        current: current.to_string(),
    }
}

/// Extracts a task ID from a branch name following the `task/<ID>-<description>` convention.
/// Returns `None` if the branch doesn't match this pattern.
fn parse_task_id_branch(branch: &str) -> Option<i32> {
    let rest = branch.strip_prefix("task/")?;
    // Extract digits until we hit '-' or end of string.
    let num_end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    if num_end == 0 {
        return None;
    }
    // The character after digits must be '-' or end of string.
    if num_end < rest.len() && rest.as_bytes()[num_end] != b'-' {
        return None;
    }
    rest[..num_end].parse().ok()
}

// ---------------------------------------------------------------------------
// Standalone helpers for use by other commands
// ---------------------------------------------------------------------------

/// Warns on stderr if the current branch doesn't match the task's branch.
/// Silent if not in a git repo or task has no branch set.
#[allow(dead_code)]
pub fn warn_branch_mismatch(task_id: i32, task_branch: &str) {
    let branch = crate::util::git::current_branch();
    let result = check_branch_match(task_id, task_branch, branch.as_deref());
    if let BranchMatch::Mismatch { task_branch, current } = result {
        eprintln!(
            "Warning: task #{task_id} is on branch {task_branch} but you're on {current}",
        );
    }
}

/// Enforces that the current git branch matches the task's branch.
///
/// Returns `Ok(())` if the branch matches or can't be determined.
/// Returns `Err` with a descriptive message if there's a mismatch.
pub fn enforce_branch_match(task_id: i32, task_branch: &str, status: &str) -> Result<(), CliError> {
    let branch = crate::util::git::current_branch();
    let result = check_branch_match(task_id, task_branch, branch.as_deref());
    if let BranchMatch::Mismatch { task_branch, current } = result {
        return Err(CliError::newf(
            ErrorCode::StatusConflict,
            format!(
                "task #{task_id} requires branch match (status {status:?} has require_branch); \
                 you're on {current}, task is on {task_branch}. Use --force to override",
            ),
        ));
    }
    Ok(())
}

/// Returns the agent name from the flag value or the KANBAN_AGENT env var.
#[allow(dead_code)]
pub fn resolve_agent_name(flag_value: &str) -> String {
    if !flag_value.is_empty() {
        return flag_value.to_string();
    }
    std::env::var("KANBAN_AGENT").unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_task_id_branch() {
        assert_eq!(parse_task_id_branch("task/42-compact"), Some(42));
        assert_eq!(parse_task_id_branch("task/1-"), Some(1));
        assert_eq!(parse_task_id_branch("task/100"), Some(100));
        assert_eq!(parse_task_id_branch("task/abc"), None);
        assert_eq!(parse_task_id_branch("main"), None);
        assert_eq!(parse_task_id_branch(""), None);
        assert_eq!(parse_task_id_branch("task/"), None);
    }

    #[test]
    fn test_check_branch_match_convention() {
        let result = check_branch_match(42, "feat/something", Some("task/42-compact"));
        assert!(result.is_match());
    }

    #[test]
    fn test_check_branch_match_exact() {
        let result = check_branch_match(42, "feat/my-branch", Some("feat/my-branch"));
        assert!(result.is_match());
    }

    #[test]
    fn test_check_branch_mismatch() {
        let result = check_branch_match(42, "feat/expected", Some("feat/other"));
        assert!(!result.is_match());
    }

    #[test]
    fn test_check_branch_no_current() {
        let result = check_branch_match(42, "feat/expected", None);
        assert!(result.is_match()); // NoBranch is treated as ok (can't enforce)
    }

    #[test]
    fn test_check_branch_no_task_branch() {
        let result = check_branch_match(42, "", Some("main"));
        assert!(result.is_match()); // NoTaskBranch is treated as ok
    }

    #[test]
    fn test_resolve_agent_name_flag() {
        assert_eq!(resolve_agent_name("agent-fox"), "agent-fox");
    }

    #[test]
    fn test_resolve_agent_name_empty() {
        // When flag is empty, falls back to env var (which may or may not be set).
        let result = resolve_agent_name("");
        // Just verify it doesn't panic; value depends on environment.
        let _ = result;
    }
}
