//! Shared WIP limit enforcement helpers.
//!
//! Performs a two-level WIP check matching the Go `enforceWIPLimitForClass()`:
//! 1. **Class WIP** (board-wide): counts ALL tasks with the same class across
//!    ALL statuses, rejects if count >= class.wip_limit.
//! 2. **Column WIP** (per-status): checks per-column limit, SKIPPED if
//!    class.bypass_column_wip is true.

use crate::error::{CliError, ErrorCode};
use crate::model::config::Config;
use crate::model::task;

/// Enforces both class-level board-wide WIP limits and column-level WIP limits.
///
/// `exclude_id` is the ID of the task being moved/created — it is excluded from
/// the count (for moves, the task is already counted; for creates, pass 0 or a
/// negative value since the task doesn't exist yet).
///
/// Returns `Ok(())` if the move/create is allowed, or a `CliError` if a WIP
/// limit would be exceeded.
pub fn enforce_wip_limits(
    cfg: &Config,
    class: &str,
    target_status: &str,
    exclude_id: i32,
) -> Result<(), CliError> {
    // 1. Class-level board-wide WIP check.
    let class_conf = if !class.is_empty() {
        cfg.class_by_name(class)
    } else {
        None
    };

    if let Some(cc) = class_conf {
        if cc.wip_limit > 0 {
            let (all_tasks, _) = task::read_all_lenient(&cfg.tasks_path())
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
            let count = count_by_class(&all_tasks, class, exclude_id);
            if count >= cc.wip_limit {
                return Err(CliError::newf(
                    ErrorCode::ClassWipExceeded,
                    format!(
                        "class {:?} WIP limit reached ({}/{} board-wide)",
                        class, count, cc.wip_limit
                    ),
                ));
            }
        }
    }

    // 2. If class bypasses column WIP, skip column check.
    if let Some(cc) = class_conf {
        if cc.bypass_column_wip {
            return Ok(());
        }
    }

    // 3. Column-level WIP check.
    let wip_limit = cfg.wip_limit(target_status);
    if wip_limit > 0 {
        let (all_tasks, _) = task::read_all_lenient(&cfg.tasks_path())
            .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        let current_count = all_tasks
            .iter()
            .filter(|t| t.status == target_status && t.id != exclude_id)
            .count() as i32;
        if current_count >= wip_limit {
            return Err(CliError::newf(
                ErrorCode::WipLimitExceeded,
                format!(
                    "WIP limit exceeded for {}: {}/{}",
                    target_status, current_count, wip_limit
                ),
            ));
        }
    }

    Ok(())
}

/// Same as `enforce_wip_limits` but returns a `String` error for use in
/// `edit_one()` which uses `Result<_, String>`.
pub fn enforce_wip_limits_str(
    cfg: &Config,
    class: &str,
    target_status: &str,
    exclude_id: i32,
) -> Result<(), String> {
    enforce_wip_limits(cfg, class, target_status, exclude_id).map_err(|e| e.message)
}

/// Counts all tasks with the given class, excluding a specific task ID.
fn count_by_class(tasks: &[task::Task], class: &str, exclude_id: i32) -> i32 {
    tasks
        .iter()
        .filter(|t| t.class == class && t.id != exclude_id)
        .count() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_by_class() {
        let tasks = vec![
            task::Task {
                id: 1,
                class: "expedite".into(),
                ..Default::default()
            },
            task::Task {
                id: 2,
                class: "expedite".into(),
                ..Default::default()
            },
            task::Task {
                id: 3,
                class: "standard".into(),
                ..Default::default()
            },
        ];

        // Exclude task #2 from count.
        assert_eq!(count_by_class(&tasks, "expedite", 2), 1);
        // Exclude a non-existent ID.
        assert_eq!(count_by_class(&tasks, "expedite", 99), 2);
        // Standard class.
        assert_eq!(count_by_class(&tasks, "standard", 99), 1);
        // Empty class matches nothing.
        assert_eq!(count_by_class(&tasks, "", 99), 0);
    }
}
