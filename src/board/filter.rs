use chrono::{Duration, Utc};
use regex::RegexBuilder;

use crate::model::config::Config;
use crate::model::task::Task;

/// Criteria for filtering tasks. All non-empty fields use AND logic:
/// a task must match every specified criterion to be included.
#[derive(Default, Clone, Debug)]
pub struct FilterOptions {
    /// Only include tasks with these IDs.
    pub ids: Vec<i32>,
    /// Only include tasks whose status is in this list.
    pub statuses: Vec<String>,
    /// Exclude tasks whose status is in this list.
    pub exclude_statuses: Vec<String>,
    /// Only include tasks whose priority is in this list.
    pub priorities: Vec<String>,
    /// Only include tasks assigned to this person (case-insensitive).
    pub assignee: Option<String>,
    /// Only include tasks that carry this tag.
    pub tag: Option<String>,
    /// Case-insensitive substring search across title, body, and tags.
    pub search: Option<String>,
    /// `Some(true)` = only blocked, `Some(false)` = only unblocked, `None` = all.
    pub blocked: Option<bool>,
    /// Only include tasks whose parent matches this ID.
    pub parent_id: Option<i32>,
    /// Only include unclaimed (or expired-claim) tasks.
    pub unclaimed: bool,
    /// Only include tasks claimed by this specific agent.
    pub claimed_by: Option<String>,
    /// Claim expiration duration for the unclaimed filter.
    pub claim_timeout: Option<Duration>,
    /// Only include tasks with this class of service.
    pub class: Option<String>,
    /// Glob pattern matched against `task.branch`.
    pub branch: Option<String>,
    /// `Some(true)` = only tasks with worktree, `Some(false)` = only without.
    pub has_worktree: Option<bool>,
}

/// Returns references to tasks matching all specified criteria (AND logic).
pub fn filter<'a>(tasks: &'a [Task], opts: &FilterOptions) -> Vec<&'a Task> {
    tasks.iter().filter(|t| matches_filter(t, opts)).collect()
}

/// Returns `true` if the task has no active claim (unclaimed or expired).
pub fn is_unclaimed(task: &Task, timeout: Option<Duration>) -> bool {
    if task.claimed_by.is_empty() {
        return true;
    }
    // If there is a timeout and a claimed_at timestamp, check expiry.
    if let Some(timeout) = timeout {
        if timeout > Duration::zero() {
            if let Some(claimed_at) = task.claimed_at {
                return Utc::now().signed_duration_since(claimed_at) > timeout;
            }
        }
    }
    false
}

/// Returns tasks from `candidates` whose dependencies are all at a terminal status.
/// `all_tasks` is used for dependency status lookups and may include tasks not in
/// `candidates` (e.g. archived tasks).
pub fn filter_unblocked<'a>(
    candidates: &[&'a Task],
    all_tasks: &[Task],
    cfg: &Config,
) -> Vec<&'a Task> {
    let status_by_id: std::collections::HashMap<i32, &str> = all_tasks
        .iter()
        .map(|t| (t.id, t.status.as_str()))
        .collect();

    candidates
        .iter()
        .filter(|t| all_deps_satisfied(&t.depends_on, &status_by_id, cfg))
        .copied()
        .collect()
}

/// Returns true if all dependency IDs are at a terminal status.
/// Missing IDs (e.g. from legacy hard-deletes) are treated as satisfied.
pub(crate) fn all_deps_satisfied(
    deps: &[i32],
    status_by_id: &std::collections::HashMap<i32, &str>,
    cfg: &Config,
) -> bool {
    for dep_id in deps {
        if let Some(status) = status_by_id.get(dep_id) {
            if !cfg.is_terminal_status(status) {
                return false;
            }
        }
        // Missing dep IDs are treated as satisfied so dependents are recoverable.
    }
    true
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn matches_filter(t: &Task, opts: &FilterOptions) -> bool {
    matches_core_filter(t, opts) && matches_extended_filter(t, opts)
}

fn matches_core_filter(t: &Task, opts: &FilterOptions) -> bool {
    // IDs filter
    if !opts.ids.is_empty() && !opts.ids.contains(&t.id) {
        return false;
    }

    // Status include/exclude
    if !matches_status(&t.status, &opts.statuses, &opts.exclude_statuses) {
        return false;
    }

    // Priority
    if !opts.priorities.is_empty() && !opts.priorities.contains(&t.priority) {
        return false;
    }

    // Assignee (case-insensitive match against bare String)
    if let Some(ref assignee) = opts.assignee {
        if !assignee.is_empty() {
            if t.assignee.is_empty() || !t.assignee.eq_ignore_ascii_case(assignee) {
                return false;
            }
        }
    }

    // Tag
    if let Some(ref tag) = opts.tag {
        if !tag.is_empty() && !t.tags.contains(tag) {
            return false;
        }
    }

    // Blocked
    if let Some(want_blocked) = opts.blocked {
        if t.blocked != want_blocked {
            return false;
        }
    }

    // Parent ID
    if let Some(parent_id) = opts.parent_id {
        match t.parent {
            Some(p) if p == parent_id => {}
            _ => return false,
        }
    }

    // Branch glob (bare String, empty means no branch)
    if let Some(ref pattern) = opts.branch {
        if !pattern.is_empty() {
            let branch_val = if t.branch.is_empty() {
                None
            } else {
                Some(t.branch.as_str())
            };
            if !matches_branch_glob(branch_val, pattern) {
                return false;
            }
        }
    }

    // Has worktree (bare String, empty means no worktree)
    if let Some(want_worktree) = opts.has_worktree {
        let has = !t.worktree.is_empty();
        if has != want_worktree {
            return false;
        }
    }

    true
}

fn matches_status(status: &str, include: &[String], exclude: &[String]) -> bool {
    if !include.is_empty() && !include.iter().any(|s| s == status) {
        return false;
    }
    if !exclude.is_empty() && exclude.iter().any(|s| s == status) {
        return false;
    }
    true
}

fn matches_extended_filter(t: &Task, opts: &FilterOptions) -> bool {
    // Search
    if let Some(ref query) = opts.search {
        if !query.is_empty() && !matches_search(t, query) {
            return false;
        }
    }

    // Unclaimed
    if opts.unclaimed && !is_unclaimed(t, opts.claim_timeout) {
        return false;
    }

    // Claimed by (bare String)
    if let Some(ref claimed_by) = opts.claimed_by {
        if !claimed_by.is_empty() {
            if t.claimed_by != *claimed_by {
                return false;
            }
        }
    }

    // Class (bare String)
    if let Some(ref class) = opts.class {
        if !class.is_empty() {
            if t.class != *class {
                return false;
            }
        }
    }

    true
}

/// Case-insensitive search across title, body, and tags.
/// Tries to compile the query as a regex first; falls back to substring matching
/// if the query is not a valid regex pattern.
fn matches_search(t: &Task, query: &str) -> bool {
    if let Ok(re) = RegexBuilder::new(query)
        .case_insensitive(true)
        .build()
    {
        if re.is_match(&t.title) {
            return true;
        }
        if !t.body.is_empty() && re.is_match(&t.body) {
            return true;
        }
        for tag in &t.tags {
            if re.is_match(tag) {
                return true;
            }
        }
        false
    } else {
        // Fall back to case-insensitive substring matching
        let q = query.to_lowercase();
        if t.title.to_lowercase().contains(&q) {
            return true;
        }
        if !t.body.is_empty() && t.body.to_lowercase().contains(&q) {
            return true;
        }
        for tag in &t.tags {
            if tag.to_lowercase().contains(&q) {
                return true;
            }
        }
        false
    }
}

/// Matches a task branch against a glob pattern using `Path::match` semantics.
/// Returns `false` if the task has no branch set.
fn matches_branch_glob(branch: Option<&str>, pattern: &str) -> bool {
    match branch {
        None | Some("") => false,
        Some(branch) => {
            // Use std glob-style matching compatible with filepath.Match
            glob_match(pattern, branch)
        }
    }
}

/// Simple glob matcher compatible with Go's `filepath.Match` patterns.
/// Supports `*` (any sequence of non-separator chars) and `?` (any single char).
fn glob_match(pattern: &str, name: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let nam: Vec<char> = name.chars().collect();
    glob_match_inner(&pat, &nam)
}

fn glob_match_inner(pattern: &[char], name: &[char]) -> bool {
    let mut pi = 0;
    let mut ni = 0;

    while pi < pattern.len() {
        match pattern[pi] {
            '?' => {
                if ni >= name.len() || name[ni] == '/' {
                    return false;
                }
                pi += 1;
                ni += 1;
            }
            '*' => {
                // Try matching zero or more non-separator chars.
                let rest_pattern = &pattern[pi + 1..];
                // Try matching with 0, 1, 2, ... chars consumed from name.
                for k in 0..=(name.len() - ni) {
                    if ni + k <= name.len() {
                        // '*' does not match path separators in filepath.Match
                        if k > 0 && name[ni + k - 1] == '/' {
                            break;
                        }
                        if glob_match_inner(rest_pattern, &name[ni + k..]) {
                            return true;
                        }
                    }
                }
                return false;
            }
            '[' => {
                // Character class - simplified support
                if ni >= name.len() {
                    return false;
                }
                let ch = name[ni];
                pi += 1; // skip '['
                let negated = pi < pattern.len() && pattern[pi] == '^';
                if negated {
                    pi += 1;
                }
                let mut matched = false;
                let mut first = true;
                while pi < pattern.len() && (first || pattern[pi] != ']') {
                    first = false;
                    if pi + 2 < pattern.len() && pattern[pi + 1] == '-' {
                        let lo = pattern[pi];
                        let hi = pattern[pi + 2];
                        if ch >= lo && ch <= hi {
                            matched = true;
                        }
                        pi += 3;
                    } else {
                        if pattern[pi] == ch {
                            matched = true;
                        }
                        pi += 1;
                    }
                }
                if pi < pattern.len() {
                    pi += 1; // skip ']'
                }
                if matched == negated {
                    return false;
                }
                ni += 1;
            }
            '\\' => {
                // Escaped character
                pi += 1;
                if pi >= pattern.len() || ni >= name.len() || pattern[pi] != name[ni] {
                    return false;
                }
                pi += 1;
                ni += 1;
            }
            c => {
                if ni >= name.len() || name[ni] != c {
                    return false;
                }
                pi += 1;
                ni += 1;
            }
        }
    }

    pi == pattern.len() && ni == name.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match() {
        assert!(glob_match("task/4-*", "task/4-foo"));
        assert!(glob_match("task/4-*", "task/4-"));
        assert!(!glob_match("task/4-*", "task/5-foo"));
        assert!(glob_match("*", "anything"));
        assert!(!glob_match("*", "has/slash"));
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "abbc"));
    }

    #[test]
    fn test_is_unclaimed_no_claim() {
        let t = Task {
            id: 1,
            ..Default::default()
        };
        assert!(is_unclaimed(&t, None));
    }

    #[test]
    fn test_is_unclaimed_with_claim() {
        let t = Task {
            id: 1,
            claimed_by: "agent".to_string(),
            claimed_at: Some(Utc::now()),
            ..Default::default()
        };
        assert!(!is_unclaimed(&t, None));
    }

    #[test]
    fn test_is_unclaimed_expired() {
        let t = Task {
            id: 1,
            claimed_by: "agent".to_string(),
            claimed_at: Some(Utc::now() - Duration::hours(25)),
            ..Default::default()
        };
        assert!(is_unclaimed(&t, Some(Duration::hours(24))));
    }
}
