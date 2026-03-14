//! `kbmdx worktrees` — list git worktrees.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::process::Command;

use serde::Serialize;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct WorktreesArgs {
    /// Check for stale metadata and orphan worktrees
    #[arg(long)]
    pub check: bool,
}

/// A parsed git worktree entry.
struct GitWorktree {
    path: String,
    branch: String,
    bare: bool,
}

/// A stale metadata finding: task references a worktree that doesn't exist.
#[derive(Debug, Serialize)]
struct StaleEntry {
    task_id: i32,
    task_title: String,
    worktree_path: String,
}

/// An orphan worktree finding: worktree not referenced by any task.
#[derive(Debug, Serialize)]
struct OrphanEntry {
    path: String,
    branch: String,
}

/// The check report for JSON output.
#[derive(Debug, Serialize)]
struct CheckReport {
    stale: Vec<StaleEntry>,
    orphan: Vec<OrphanEntry>,
}

pub fn run(cli: &Cli, args: WorktreesArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);

    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("failed to run git: {e}")))?;

    if !output.status.success() {
        return Err(CliError::new(
            ErrorCode::InternalError,
            "git worktree list failed (not in a git repo?)",
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout);

    // Parse porcelain output into structured entries and JSON values.
    let mut worktrees_json: Vec<serde_json::Value> = Vec::new();
    let mut worktrees_parsed: Vec<GitWorktree> = Vec::new();
    let mut current: Option<serde_json::Map<String, serde_json::Value>> = None;
    let mut cur_path = String::new();
    let mut cur_branch = String::new();
    let mut cur_bare = false;

    for line in text.lines() {
        if line.is_empty() {
            if let Some(wt) = current.take() {
                worktrees_json.push(serde_json::Value::Object(wt));
                worktrees_parsed.push(GitWorktree {
                    path: std::mem::take(&mut cur_path),
                    branch: std::mem::take(&mut cur_branch),
                    bare: cur_bare,
                });
                cur_bare = false;
            }
            continue;
        }
        if line.starts_with("worktree ") {
            let mut map = serde_json::Map::new();
            let p = line[9..].to_string();
            map.insert(
                "path".to_string(),
                serde_json::Value::String(p.clone()),
            );
            cur_path = p;
            current = Some(map);
        } else if line.starts_with("branch ") {
            if let Some(ref mut map) = current {
                let b = line[7..].to_string();
                map.insert(
                    "branch".to_string(),
                    serde_json::Value::String(b.clone()),
                );
                cur_branch = b;
            }
        } else if line.starts_with("HEAD ") {
            if let Some(ref mut map) = current {
                map.insert(
                    "head".to_string(),
                    serde_json::Value::String(line[5..].to_string()),
                );
            }
        } else if line == "bare" {
            if let Some(ref mut map) = current {
                map.insert("bare".to_string(), serde_json::Value::Bool(true));
                cur_bare = true;
            }
        }
    }
    if let Some(wt) = current.take() {
        worktrees_json.push(serde_json::Value::Object(wt));
        worktrees_parsed.push(GitWorktree {
            path: cur_path,
            branch: cur_branch,
            bare: cur_bare,
        });
    }

    if !args.check {
        // Normal listing mode — same as before.
        let mut stdout = std::io::stdout();
        match format {
            Format::Json => {
                crate::output::json::json(&mut stdout, &worktrees_json)
                    .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
            }
            Format::Compact => {
                for wt in &worktrees_json {
                    let path = wt.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                    let branch = wt.get("branch").and_then(|v| v.as_str()).unwrap_or("?");
                    writeln!(stdout, "{path} {branch}").unwrap_or(());
                }
            }
            Format::Table => {
                for wt in &worktrees_json {
                    let path = wt.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                    let branch = wt
                        .get("branch")
                        .and_then(|v| v.as_str())
                        .unwrap_or("(detached)");
                    writeln!(stdout, "{path}  {branch}").unwrap_or(());
                }
            }
        }
        return Ok(());
    }

    // --check mode: cross-reference worktrees with tasks.
    let cfg = crate::cli::root::load_config(cli)?;
    let tasks_dir = cfg.tasks_path();
    let (tasks, _warnings) = crate::model::task::read_all_lenient(&tasks_dir)
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("reading tasks: {e}")))?;

    // Build set of git worktree paths (excluding main/bare worktree).
    let git_wt_paths: HashSet<String> = worktrees_parsed
        .iter()
        .filter(|wt| !wt.bare)
        .skip(1) // skip the first worktree (main checkout)
        .map(|wt| wt.path.clone())
        .collect();

    // Build a map from git worktree path -> branch for orphan reporting.
    let git_wt_branch: HashMap<String, String> = worktrees_parsed
        .iter()
        .filter(|wt| !wt.bare)
        .skip(1)
        .map(|wt| (wt.path.clone(), wt.branch.clone()))
        .collect();

    // Also build a set of all git worktree paths for the full set (including main)
    // to check stale metadata against.
    let all_git_wt_paths: HashSet<String> = worktrees_parsed
        .iter()
        .map(|wt| wt.path.clone())
        .collect();

    // Build a map from branch ref -> worktree path for branch-based matching.
    let git_branch_to_path: HashMap<String, String> = worktrees_parsed
        .iter()
        .filter(|wt| !wt.branch.is_empty())
        .map(|wt| (wt.branch.clone(), wt.path.clone()))
        .collect();

    // Detect stale metadata: tasks pointing to non-existent worktrees.
    let mut stale: Vec<StaleEntry> = Vec::new();
    // Track which git worktree paths are referenced by at least one task.
    let mut referenced_paths: HashSet<String> = HashSet::new();

    for task in &tasks {
        // Check worktree field.
        if !task.worktree.is_empty() {
            if all_git_wt_paths.contains(&task.worktree) {
                referenced_paths.insert(task.worktree.clone());
            } else {
                stale.push(StaleEntry {
                    task_id: task.id,
                    task_title: task.title.clone(),
                    worktree_path: task.worktree.clone(),
                });
            }
        }

        // Check branch field: if a task has a branch, mark the corresponding
        // worktree path as referenced (if one exists).
        if !task.branch.is_empty() {
            // Try both bare branch name and refs/heads/ prefixed form.
            let ref_form = if task.branch.starts_with("refs/") {
                task.branch.clone()
            } else {
                format!("refs/heads/{}", task.branch)
            };
            if let Some(path) = git_branch_to_path.get(&ref_form) {
                referenced_paths.insert(path.clone());
            }
            // Also try the bare form directly.
            if let Some(path) = git_branch_to_path.get(&task.branch) {
                referenced_paths.insert(path.clone());
            }
        }
    }

    // Detect orphan worktrees: git worktrees not referenced by any task.
    let mut orphan: Vec<OrphanEntry> = Vec::new();
    for wt_path in &git_wt_paths {
        if !referenced_paths.contains(wt_path) {
            let branch = git_wt_branch
                .get(wt_path)
                .cloned()
                .unwrap_or_default();
            orphan.push(OrphanEntry {
                path: wt_path.clone(),
                branch,
            });
        }
    }

    // Sort for deterministic output.
    stale.sort_by(|a, b| a.task_id.cmp(&b.task_id));
    orphan.sort_by(|a, b| a.path.cmp(&b.path));

    let report = CheckReport { stale, orphan };

    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            crate::output::json::json(&mut stdout, &report)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        Format::Compact | Format::Table => {
            if report.stale.is_empty() && report.orphan.is_empty() {
                writeln!(stdout, "No issues found.").unwrap_or(());
            } else {
                for entry in &report.stale {
                    writeln!(
                        stdout,
                        "STALE: task #{} ({}) references missing worktree {}",
                        entry.task_id, entry.task_title, entry.worktree_path
                    )
                    .unwrap_or(());
                }
                for entry in &report.orphan {
                    writeln!(
                        stdout,
                        "ORPHAN: worktree {} (branch {}) not referenced by any task",
                        entry.path,
                        if entry.branch.is_empty() {
                            "(detached)"
                        } else {
                            &entry.branch
                        }
                    )
                    .unwrap_or(());
                }
            }
        }
    }

    Ok(())
}
