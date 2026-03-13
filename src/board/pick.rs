//! Pick the highest-priority unclaimed task.

use chrono::Duration;

use crate::board::filter::{self, FilterOptions};
use crate::model::config::Config;
use crate::model::task::Task;

const CLASS_STANDARD: &str = "standard";

/// Options for picking a task.
pub struct PickOptions {
    /// Status column to filter by. If None, uses all non-terminal statuses.
    pub status: Option<String>,
    /// Statuses to pick from (overrides `status` if non-empty).
    pub statuses: Vec<String>,
    /// Tag filter using OR logic: task must have at least one matching tag.
    pub tags: Vec<String>,
    /// Claim expiration for filtering out actively-claimed tasks.
    pub claim_timeout: Option<Duration>,
    /// Whether to exclude the body from the result.
    pub no_body: bool,
}

impl Default for PickOptions {
    fn default() -> Self {
        Self {
            status: None,
            statuses: Vec::new(),
            tags: Vec::new(),
            claim_timeout: None,
            no_body: false,
        }
    }
}

/// Pick the best unclaimed task matching the criteria.
///
/// Selection order:
/// 1. Filter by status (or all non-terminal if empty), unclaimed, not blocked
/// 2. Filter by tags (OR: task has at least one matching tag)
/// 3. Filter by dependency satisfaction (all deps at terminal status)
/// 4. Sort by class priority then task priority
/// 5. Return first candidate or None
pub fn pick<'a>(tasks: &'a [Task], cfg: &Config, opts: &PickOptions) -> Option<&'a Task> {
    let statuses: Vec<String> = if !opts.statuses.is_empty() {
        opts.statuses.clone()
    } else if let Some(ref status) = opts.status {
        vec![status.clone()]
    } else {
        cfg.active_statuses()
    };

    let filter_opts = FilterOptions {
        unclaimed: true,
        statuses,
        claim_timeout: opts.claim_timeout,
        ..Default::default()
    };

    let candidates = filter::filter(tasks, &filter_opts);

    // Further filter by tags if specified (OR logic: task must have at least one).
    let candidates: Vec<&Task> = if opts.tags.is_empty() {
        candidates
    } else {
        candidates
            .into_iter()
            .filter(|t| has_any_tag(&t.tags, &opts.tags))
            .collect()
    };

    // Filter out blocked tasks explicitly.
    let candidates: Vec<&Task> = candidates
        .into_iter()
        .filter(|t| !t.blocked)
        .collect();

    // Filter out tasks with unmet dependencies.
    let unblocked = filter::filter_unblocked(&candidates, tasks, cfg);

    if unblocked.is_empty() {
        return None;
    }

    // Sort by class priority then task priority.
    let mut sorted = unblocked;
    sort_pick_candidates(&mut sorted, cfg);

    sorted.into_iter().next()
}

/// Sorts candidates by class priority then task priority.
fn sort_pick_candidates(candidates: &mut [&Task], cfg: &Config) {
    candidates.sort_by(|a, b| {
        let ca = class_order(a, cfg);
        let cb = class_order(b, cfg);
        if ca != cb {
            return ca.cmp(&cb);
        }

        // Within the same class, if both are fixed-date, sort by due date
        let a_class = if a.class.is_empty() { "" } else { a.class.as_str() };
        let b_class = if b.class.is_empty() { "" } else { b.class.as_str() };
        if a_class == "fixed-date" && b_class == "fixed-date" {
            match (&a.due, &b.due) {
                (Some(da), Some(db)) => {
                    let cmp = da.cmp(db);
                    if cmp != std::cmp::Ordering::Equal {
                        return cmp;
                    }
                }
                (Some(_), None) => return std::cmp::Ordering::Less,
                (None, Some(_)) => return std::cmp::Ordering::Greater,
                (None, None) => {}
            }
        }

        // Higher priority index = higher priority, so reverse the comparison.
        // Then break ties by ID (oldest first).
        cfg.priority_index(&b.priority)
            .cmp(&cfg.priority_index(&a.priority))
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// Returns true if the task has at least one of the given tags.
fn has_any_tag(task_tags: &[String], filter_tags: &[String]) -> bool {
    filter_tags.iter().any(|ft| task_tags.contains(ft))
}

/// Returns a sort key for a task's class. Lower values = higher priority.
fn class_order(t: &Task, cfg: &Config) -> usize {
    let class_name = if t.class.is_empty() {
        CLASS_STANDARD
    } else {
        t.class.as_str()
    };

    cfg.class_index(class_name)
        .unwrap_or_else(|| cfg.class_index(CLASS_STANDARD).unwrap_or(usize::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_task(id: i32, status: &str, priority: &str) -> Task {
        Task {
            id,
            title: format!("Task {}", id),
            status: status.to_string(),
            priority: priority.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_pick_skips_blocked() {
        let cfg = Config::new_default("test");
        let mut t1 = make_task(1, "todo", "high");
        t1.blocked = true;
        let t2 = make_task(2, "todo", "medium");
        let tasks = vec![t1, t2];

        let opts = PickOptions {
            statuses: vec!["todo".to_string()],
            ..Default::default()
        };

        let picked = pick(&tasks, &cfg, &opts);
        assert!(picked.is_some());
        assert_eq!(picked.unwrap().id, 2);
    }

    #[test]
    fn test_pick_skips_claimed() {
        let cfg = Config::new_default("test");
        let mut t1 = make_task(1, "todo", "high");
        t1.claimed_by = "agent-x".to_string();
        t1.claimed_at = Some(Utc::now());
        let t2 = make_task(2, "todo", "medium");
        let tasks = vec![t1, t2];

        let opts = PickOptions {
            statuses: vec!["todo".to_string()],
            ..Default::default()
        };

        let picked = pick(&tasks, &cfg, &opts);
        assert!(picked.is_some());
        assert_eq!(picked.unwrap().id, 2);
    }

    #[test]
    fn test_pick_returns_none_when_empty() {
        let cfg = Config::new_default("test");
        let tasks: Vec<Task> = Vec::new();
        let opts = PickOptions::default();
        assert!(pick(&tasks, &cfg, &opts).is_none());
    }

    #[test]
    fn test_pick_prefers_higher_priority() {
        let cfg = Config::new_default("test");
        let t1 = make_task(1, "todo", "low");
        let t2 = make_task(2, "todo", "high");
        let t3 = make_task(3, "todo", "medium");
        let tasks = vec![t1, t2, t3];

        let opts = PickOptions {
            statuses: vec!["todo".to_string()],
            ..Default::default()
        };

        let picked = pick(&tasks, &cfg, &opts);
        assert!(picked.is_some());
        assert_eq!(picked.unwrap().id, 2);
    }

    #[test]
    fn test_pick_with_tag_filter() {
        let cfg = Config::new_default("test");
        let mut t1 = make_task(1, "todo", "high");
        t1.tags = vec!["feature".to_string()];
        let mut t2 = make_task(2, "todo", "high");
        t2.tags = vec!["bug".to_string()];
        let tasks = vec![t1, t2];

        let opts = PickOptions {
            statuses: vec!["todo".to_string()],
            tags: vec!["bug".to_string()],
            ..Default::default()
        };

        let picked = pick(&tasks, &cfg, &opts);
        assert!(picked.is_some());
        assert_eq!(picked.unwrap().id, 2);
    }
}
