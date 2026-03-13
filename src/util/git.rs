//! Git utility functions.

use std::path::Path;
use std::process::Command;

/// Returns the current git branch name, or None if not in a git repo.
pub fn current_branch() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// Returns the git root directory, or None if not in a git repo.
pub fn root_dir() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// List all local git branches.
pub fn list_branches() -> Vec<String> {
    let output = Command::new("git")
        .args(["branch", "--format=%(refname:short)"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        }
        _ => vec![],
    }
}

/// List branches that have active worktrees.
pub fn list_worktree_branches() -> Vec<String> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.lines()
                .filter_map(|line| {
                    line.strip_prefix("branch refs/heads/")
                        .map(|s| s.trim().to_string())
                })
                .collect()
        }
        _ => vec![],
    }
}

/// List all local branch names using `git for-each-ref` (machine-friendly).
///
/// Returns a `HashSet` for O(1) lookup (used for missing-branch detection).
/// Returns an empty set on failure (e.g. not inside a git repo).
pub fn local_branches() -> std::collections::HashSet<String> {
    let output = Command::new("git")
        .args(["for-each-ref", "--format=%(refname:short)", "refs/heads/"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        }
        _ => std::collections::HashSet::new(),
    }
}

/// Returns true if the given path is inside a git worktree (not the main repo).
pub fn is_worktree(path: &Path) -> bool {
    let output = Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(path)
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let common = String::from_utf8_lossy(&o.stdout).trim().to_string();
            // In a worktree, git-common-dir differs from git-dir.
            let git_dir_output = Command::new("git")
                .args(["rev-parse", "--git-dir"])
                .current_dir(path)
                .output();
            if let Ok(gd) = git_dir_output {
                if gd.status.success() {
                    let git_dir = String::from_utf8_lossy(&gd.stdout).trim().to_string();
                    return git_dir != common;
                }
            }
            false
        }
        _ => false,
    }
}
