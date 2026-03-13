//! List tasks with filtering, sorting, and limiting.

use crate::model::config::Config;
use crate::model::task::Task;

use super::filter::{filter, filter_unblocked, FilterOptions};
use super::sort::{sort, SortField};

/// Options controlling how tasks are listed.
pub struct ListOptions {
    /// Filter criteria (AND logic).
    pub filter: FilterOptions,
    /// Sort field.
    pub sort_by: SortField,
    /// Reverse the sort order.
    pub reverse: bool,
    /// Maximum number of tasks to return.
    pub limit: Option<usize>,
    /// Only include tasks with all dependencies at terminal status.
    pub unblocked: bool,
}

impl Default for ListOptions {
    fn default() -> Self {
        Self {
            filter: FilterOptions::default(),
            sort_by: SortField::default(),
            reverse: false,
            limit: None,
            unblocked: false,
        }
    }
}

/// Applies filters, dependency checks, sorting, and limiting to a slice of tasks.
///
/// Steps:
/// 1. Apply filter criteria.
/// 2. If `unblocked` is set, filter to tasks with all deps satisfied.
/// 3. Sort by the specified field (default `SortField::Id`).
/// 4. Apply limit if set.
pub fn list<'a>(cfg: &Config, all_tasks: &'a [Task], opts: &ListOptions) -> Vec<&'a Task> {
    let mut tasks = filter(all_tasks, &opts.filter);

    if opts.unblocked {
        tasks = filter_unblocked(&tasks, all_tasks, cfg);
    }

    sort(&mut tasks, opts.sort_by, opts.reverse, cfg);

    if let Some(limit) = opts.limit {
        if limit > 0 && tasks.len() > limit {
            tasks.truncate(limit);
        }
    }

    tasks
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_list_with_limit() {
        let cfg = Config::new_default("test");
        let tasks = vec![
            make_task(1, "todo", "medium"),
            make_task(2, "todo", "high"),
            make_task(3, "todo", "low"),
        ];

        let opts = ListOptions {
            limit: Some(2),
            ..Default::default()
        };

        let result = list(&cfg, &tasks, &opts);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_list_with_status_filter() {
        let cfg = Config::new_default("test");
        let tasks = vec![
            make_task(1, "todo", "medium"),
            make_task(2, "in-progress", "high"),
            make_task(3, "done", "low"),
        ];

        let opts = ListOptions {
            filter: FilterOptions {
                statuses: vec!["todo".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };

        let result = list(&cfg, &tasks, &opts);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 1);
    }

    #[test]
    fn test_list_default_sort_is_by_id() {
        let cfg = Config::new_default("test");
        let tasks = vec![
            make_task(3, "todo", "low"),
            make_task(1, "todo", "high"),
            make_task(2, "todo", "medium"),
        ];

        let opts = ListOptions::default();
        let result = list(&cfg, &tasks, &opts);
        assert_eq!(
            result.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }
}
