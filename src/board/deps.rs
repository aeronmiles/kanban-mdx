//! Dependency graph traversal for tasks.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::Serialize;

use crate::model::task::Task;

/// Direction for dependency traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepDirection {
    /// Traverse both upstream and downstream.
    Both,
    /// Traverse only upstream (what this task depends on).
    Upstream,
    /// Traverse only downstream (what depends on this task).
    Downstream,
}

/// A single dependency result entry.
#[derive(Debug, Clone, Serialize)]
pub struct DepResult {
    pub id: i32,
    pub title: String,
    pub status: String,
}

/// Structured output of the deps command.
#[derive(Debug, Clone, Serialize)]
pub struct DepsOutput {
    pub task_id: i32,
    pub task_title: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub upstream: Vec<DepResult>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub downstream: Vec<DepResult>,
}

/// Returns all upstream (blocking) dependencies for the given task ID.
pub fn upstream<'a>(tasks: &'a [Task], id: i32, transitive: bool) -> Vec<&'a Task> {
    let task = match tasks.iter().find(|t| t.id == id) {
        Some(t) => t,
        None => return Vec::new(),
    };

    if !transitive {
        return tasks
            .iter()
            .filter(|t| task.depends_on.contains(&t.id))
            .collect();
    }

    let mut visited = HashSet::new();
    let mut result = Vec::new();
    collect_upstream(tasks, id, &mut visited, &mut result);
    result
}

/// Returns all downstream (dependent) tasks for the given task ID.
pub fn downstream<'a>(tasks: &'a [Task], id: i32, transitive: bool) -> Vec<&'a Task> {
    if !transitive {
        return tasks
            .iter()
            .filter(|t| t.depends_on.contains(&id))
            .collect();
    }

    let mut visited = HashSet::new();
    let mut result = Vec::new();
    collect_downstream(tasks, id, &mut visited, &mut result);
    result
}

/// Computes upstream and/or downstream dependencies for a given task.
///
/// - **Upstream**: tasks that this task depends on (via `depends_on` field).
/// - **Downstream**: tasks that depend on this task.
/// - If `transitive` is `true`, follows the full ancestor/descendant chain via BFS.
/// - If `transitive` is `false`, only direct (one-hop) dependencies.
///
/// Returns `None` if `target_id` is not found in `all_tasks`.
pub fn deps(
    all_tasks: &[Task],
    target_id: i32,
    direction: DepDirection,
    transitive: bool,
) -> Option<DepsOutput> {
    let by_id: HashMap<i32, &Task> = all_tasks.iter().map(|t| (t.id, t)).collect();

    let target = by_id.get(&target_id)?;

    let mut out = DepsOutput {
        task_id: target_id,
        task_title: target.title.clone(),
        upstream: Vec::new(),
        downstream: Vec::new(),
    };

    if direction == DepDirection::Both || direction == DepDirection::Upstream {
        out.upstream = if transitive {
            transitive_upstream(target_id, &by_id)
        } else {
            direct_upstream(target, &by_id)
        };
    }

    if direction == DepDirection::Both || direction == DepDirection::Downstream {
        let downstream_index = build_downstream_index(all_tasks);
        out.downstream = if transitive {
            transitive_downstream(target_id, &downstream_index, &by_id)
        } else {
            direct_downstream(target_id, &downstream_index, &by_id)
        };
    }

    Some(out)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn collect_upstream<'a>(
    tasks: &'a [Task],
    id: i32,
    visited: &mut HashSet<i32>,
    result: &mut Vec<&'a Task>,
) {
    if !visited.insert(id) {
        return;
    }
    if let Some(task) = tasks.iter().find(|t| t.id == id) {
        for dep_id in &task.depends_on {
            if let Some(dep) = tasks.iter().find(|t| t.id == *dep_id) {
                result.push(dep);
                collect_upstream(tasks, *dep_id, visited, result);
            }
        }
    }
}

fn collect_downstream<'a>(
    tasks: &'a [Task],
    id: i32,
    visited: &mut HashSet<i32>,
    result: &mut Vec<&'a Task>,
) {
    if !visited.insert(id) {
        return;
    }
    for task in tasks {
        if task.depends_on.contains(&id) {
            result.push(task);
            collect_downstream(tasks, task.id, visited, result);
        }
    }
}

/// Returns the immediate dependencies of the target task.
fn direct_upstream(target: &Task, by_id: &HashMap<i32, &Task>) -> Vec<DepResult> {
    target
        .depends_on
        .iter()
        .filter_map(|dep_id| {
            by_id.get(dep_id).map(|t| DepResult {
                id: t.id,
                title: t.title.clone(),
                status: t.status.clone(),
            })
        })
        .collect()
}

/// Follows the full ancestor chain via BFS.
fn transitive_upstream(start_id: i32, by_id: &HashMap<i32, &Task>) -> Vec<DepResult> {
    let mut visited: HashSet<i32> = HashSet::new();
    visited.insert(start_id);
    let mut queue: VecDeque<i32> = VecDeque::new();
    queue.push_back(start_id);
    let mut results = Vec::new();

    while let Some(current) = queue.pop_front() {
        if let Some(t) = by_id.get(&current) {
            for dep_id in &t.depends_on {
                if visited.contains(dep_id) {
                    continue;
                }
                visited.insert(*dep_id);
                if let Some(dep) = by_id.get(dep_id) {
                    results.push(DepResult {
                        id: dep.id,
                        title: dep.title.clone(),
                        status: dep.status.clone(),
                    });
                    queue.push_back(*dep_id);
                }
            }
        }
    }

    results
}

/// Builds a map from task ID to list of task IDs that depend on it.
fn build_downstream_index(all_tasks: &[Task]) -> HashMap<i32, Vec<i32>> {
    let mut downstream: HashMap<i32, Vec<i32>> = HashMap::new();
    for t in all_tasks {
        for dep_id in &t.depends_on {
            downstream.entry(*dep_id).or_default().push(t.id);
        }
    }
    downstream
}

/// Returns tasks that directly depend on the target.
fn direct_downstream(
    target_id: i32,
    downstream_index: &HashMap<i32, Vec<i32>>,
    by_id: &HashMap<i32, &Task>,
) -> Vec<DepResult> {
    downstream_index
        .get(&target_id)
        .map(|ids| {
            ids.iter()
                .filter_map(|id| {
                    by_id.get(id).map(|t| DepResult {
                        id: t.id,
                        title: t.title.clone(),
                        status: t.status.clone(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Follows the full descendant chain via BFS.
fn transitive_downstream(
    start_id: i32,
    downstream_index: &HashMap<i32, Vec<i32>>,
    by_id: &HashMap<i32, &Task>,
) -> Vec<DepResult> {
    let mut visited: HashSet<i32> = HashSet::new();
    visited.insert(start_id);
    let mut queue: VecDeque<i32> = VecDeque::new();
    queue.push_back(start_id);
    let mut results = Vec::new();

    while let Some(current) = queue.pop_front() {
        if let Some(ids) = downstream_index.get(&current) {
            for id in ids {
                if visited.contains(id) {
                    continue;
                }
                visited.insert(*id);
                if let Some(t) = by_id.get(id) {
                    results.push(DepResult {
                        id: t.id,
                        title: t.title.clone(),
                        status: t.status.clone(),
                    });
                    queue.push_back(*id);
                }
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(id: i32, depends_on: Vec<i32>) -> Task {
        Task {
            id,
            title: format!("Task {}", id),
            status: "todo".to_string(),
            depends_on,
            ..Default::default()
        }
    }

    #[test]
    fn test_upstream_direct() {
        let tasks = vec![
            make_task(1, vec![]),
            make_task(2, vec![1]),
            make_task(3, vec![2]),
        ];
        let result = upstream(&tasks, 2, false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 1);
    }

    #[test]
    fn test_upstream_transitive() {
        let tasks = vec![
            make_task(1, vec![]),
            make_task(2, vec![1]),
            make_task(3, vec![2]),
        ];
        let result = upstream(&tasks, 3, true);
        assert_eq!(result.len(), 2);
        let ids: Vec<i32> = result.iter().map(|t| t.id).collect();
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
    }

    #[test]
    fn test_downstream_direct() {
        let tasks = vec![
            make_task(1, vec![]),
            make_task(2, vec![1]),
            make_task(3, vec![2]),
        ];
        let result = downstream(&tasks, 1, false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 2);
    }

    #[test]
    fn test_downstream_transitive() {
        let tasks = vec![
            make_task(1, vec![]),
            make_task(2, vec![1]),
            make_task(3, vec![2]),
        ];
        let result = downstream(&tasks, 1, true);
        assert_eq!(result.len(), 2);
        let ids: Vec<i32> = result.iter().map(|t| t.id).collect();
        assert!(ids.contains(&2));
        assert!(ids.contains(&3));
    }

    #[test]
    fn test_deps_structured_both() {
        let tasks = vec![
            make_task(1, vec![]),
            make_task(2, vec![1]),
            make_task(3, vec![2]),
        ];
        let result = deps(&tasks, 2, DepDirection::Both, false).unwrap();
        assert_eq!(result.task_id, 2);
        assert_eq!(result.upstream.len(), 1);
        assert_eq!(result.upstream[0].id, 1);
        assert_eq!(result.downstream.len(), 1);
        assert_eq!(result.downstream[0].id, 3);
    }

    #[test]
    fn test_deps_missing_target() {
        let tasks = vec![make_task(1, vec![])];
        assert!(deps(&tasks, 99, DepDirection::Both, false).is_none());
    }

    #[test]
    fn test_deps_transitive_upstream() {
        let tasks = vec![
            make_task(1, vec![]),
            make_task(2, vec![1]),
            make_task(3, vec![2]),
        ];
        let result = deps(&tasks, 3, DepDirection::Upstream, true).unwrap();
        assert_eq!(result.upstream.len(), 2);
        assert!(result.downstream.is_empty());
    }

    #[test]
    fn test_deps_transitive_downstream() {
        let tasks = vec![
            make_task(1, vec![]),
            make_task(2, vec![1]),
            make_task(3, vec![2]),
        ];
        let result = deps(&tasks, 1, DepDirection::Downstream, true).unwrap();
        assert!(result.upstream.is_empty());
        assert_eq!(result.downstream.len(), 2);
    }
}
