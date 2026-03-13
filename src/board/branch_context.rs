//! Branch context resolution and expansion.
//!
//! Maps git branches to tasks and expands a "root" task into a set of
//! related task IDs (parent, siblings, upstream/downstream deps, same
//! claimant) for context-aware board views.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;

use crate::model::task::Task;

/// Regex for the `task/<ID>-*` branch naming convention.
static TASK_BRANCH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^task/(\d+)(?:-|$)").expect("invalid task branch regex"));

/// Find the task matching a branch name.
///
/// Fallback chain:
/// 1. Exact match — task whose `branch` field equals the branch name.
/// 2. Convention match — parse `task/<ID>-*` from the branch and look up by ID.
/// 3. `None`.
pub fn resolve_context_task<'a>(branch: &str, tasks: &'a [Task]) -> Option<&'a Task> {
    // 1. Exact match on the branch field.
    for t in tasks {
        if !t.branch.is_empty() && t.branch == branch {
            return Some(t);
        }
    }

    // 2. Convention match via task/<ID>-* pattern.
    if let Some(id) = parse_task_id_branch(branch) {
        for t in tasks {
            if t.id == id {
                return Some(t);
            }
        }
    }

    None
}

/// Parse a task ID from a branch name like `task/4-description`.
///
/// Returns `None` if the branch does not match the `task/<ID>(-…)` pattern.
pub fn parse_task_id_branch(branch: &str) -> Option<i32> {
    let caps = TASK_BRANCH_RE.captures(branch)?;
    caps.get(1)?.as_str().parse::<i32>().ok()
}

/// Expand a root task ID into the set of related task IDs.
///
/// The returned set includes:
/// - The root task itself.
/// - Its parent (if any) and siblings (tasks sharing the same parent).
/// - Upstream dependencies (`depends_on` of the root).
/// - Downstream dependents (tasks whose `depends_on` contains the root).
/// - Tasks claimed by the same `agent` (when non-empty).
///
/// The result is sorted and deduplicated.
pub fn expand_context(root_id: i32, all_tasks: &[Task], agent: &str) -> Vec<i32> {
    let root = all_tasks.iter().find(|t| t.id == root_id);

    let root = match root {
        Some(r) => r,
        None => return vec![root_id],
    };

    let mut set = BTreeSet::new();
    set.insert(root_id);

    // Parent + siblings.
    if let Some(parent_id) = root.parent {
        set.insert(parent_id);
        for t in all_tasks {
            if t.parent == Some(parent_id) {
                set.insert(t.id);
            }
        }
    }

    // Upstream dependencies.
    for &dep in &root.depends_on {
        set.insert(dep);
    }

    // Downstream dependents.
    for t in all_tasks {
        if t.depends_on.contains(&root_id) {
            set.insert(t.id);
        }
    }

    // Same-claimant tasks.
    if !agent.is_empty() {
        for t in all_tasks {
            if t.claimed_by == agent {
                set.insert(t.id);
            }
        }
    }

    set.into_iter().collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a minimal task with just the fields we need.
    fn make_task(id: i32) -> Task {
        Task {
            id,
            ..Default::default()
        }
    }

    // -- parse_task_id_branch -------------------------------------------------

    #[test]
    fn test_parse_task_id_branch_with_description() {
        assert_eq!(parse_task_id_branch("task/4-add-feature"), Some(4));
    }

    #[test]
    fn test_parse_task_id_branch_bare_id() {
        assert_eq!(parse_task_id_branch("task/12"), Some(12));
    }

    #[test]
    fn test_parse_task_id_branch_no_match() {
        assert_eq!(parse_task_id_branch("feature/cool-stuff"), None);
        assert_eq!(parse_task_id_branch("main"), None);
        assert_eq!(parse_task_id_branch("task/"), None);
    }

    #[test]
    fn test_parse_task_id_branch_trailing_hyphen() {
        assert_eq!(parse_task_id_branch("task/7-"), Some(7));
    }

    // -- resolve_context_task -------------------------------------------------

    #[test]
    fn test_resolve_exact_branch_match() {
        let mut t = make_task(1);
        t.branch = "feature/my-branch".to_string();
        let tasks = vec![t];

        let found = resolve_context_task("feature/my-branch", &tasks);
        assert_eq!(found.map(|t| t.id), Some(1));
    }

    #[test]
    fn test_resolve_convention_match() {
        let tasks = vec![make_task(4), make_task(5)];
        let found = resolve_context_task("task/4-some-desc", &tasks);
        assert_eq!(found.map(|t| t.id), Some(4));
    }

    #[test]
    fn test_resolve_exact_takes_priority() {
        let mut t1 = make_task(4);
        t1.branch = "task/5-override".to_string(); // exact match on different branch
        let t2 = make_task(5);
        let tasks = vec![t1, t2];

        // The branch name matches the convention for task 5, but task 4 has an
        // exact branch match, so task 4 wins.
        let found = resolve_context_task("task/5-override", &tasks);
        assert_eq!(found.map(|t| t.id), Some(4));
    }

    #[test]
    fn test_resolve_no_match() {
        let tasks = vec![make_task(1)];
        assert!(resolve_context_task("unrelated-branch", &tasks).is_none());
    }

    // -- expand_context -------------------------------------------------------

    #[test]
    fn test_expand_context_root_only() {
        let tasks = vec![make_task(1)];
        let ids = expand_context(1, &tasks, "");
        assert_eq!(ids, vec![1]);
    }

    #[test]
    fn test_expand_context_missing_root() {
        let tasks = vec![make_task(1)];
        let ids = expand_context(99, &tasks, "");
        assert_eq!(ids, vec![99]);
    }

    #[test]
    fn test_expand_context_parent_and_siblings() {
        let mut root = make_task(2);
        root.parent = Some(1);
        let mut sibling = make_task(3);
        sibling.parent = Some(1);
        let parent = make_task(1);

        let tasks = vec![parent, root, sibling];
        let ids = expand_context(2, &tasks, "");
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn test_expand_context_upstream_deps() {
        let mut root = make_task(1);
        root.depends_on = vec![10, 20];
        let tasks = vec![root, make_task(10), make_task(20)];

        let ids = expand_context(1, &tasks, "");
        assert_eq!(ids, vec![1, 10, 20]);
    }

    #[test]
    fn test_expand_context_downstream_dependents() {
        let root = make_task(1);
        let mut dep = make_task(5);
        dep.depends_on = vec![1];

        let tasks = vec![root, dep];
        let ids = expand_context(1, &tasks, "");
        assert_eq!(ids, vec![1, 5]);
    }

    #[test]
    fn test_expand_context_same_claimant() {
        let mut root = make_task(1);
        root.claimed_by = "agent-a".to_string();
        let mut other = make_task(7);
        other.claimed_by = "agent-a".to_string();
        let unrelated = make_task(8);

        let tasks = vec![root, other, unrelated];
        let ids = expand_context(1, &tasks, "agent-a");
        assert_eq!(ids, vec![1, 7]);
    }

    #[test]
    fn test_expand_context_combined() {
        let mut root = make_task(10);
        root.parent = Some(1);
        root.depends_on = vec![20];
        root.claimed_by = "bot".to_string();

        let parent = make_task(1);
        let mut sibling = make_task(11);
        sibling.parent = Some(1);
        let upstream = make_task(20);
        let mut downstream = make_task(30);
        downstream.depends_on = vec![10];
        let mut same_claim = make_task(40);
        same_claim.claimed_by = "bot".to_string();

        let tasks = vec![root, parent, sibling, upstream, downstream, same_claim];
        let ids = expand_context(10, &tasks, "bot");
        assert_eq!(ids, vec![1, 10, 11, 20, 30, 40]);
    }
}
