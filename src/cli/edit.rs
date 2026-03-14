//! `kbmdx edit` — edit one or more existing tasks.

use std::io::Write;
use chrono::{NaiveTime, TimeZone, Utc};

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::task;
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct EditArgs {
    /// Task ID(s)
    pub ids: Vec<String>,
    /// New title
    #[arg(long)]
    pub title: Option<String>,
    /// New priority
    #[arg(long, short = 'p')]
    pub priority: Option<String>,
    /// New assignee
    #[arg(long)]
    pub assignee: Option<String>,
    /// Set tags (comma-separated, replaces existing)
    #[arg(long, value_delimiter = ',')]
    pub tags: Option<Vec<String>>,
    /// Set due date (YYYY-MM-DD)
    #[arg(long)]
    pub due: Option<String>,
    /// Set estimate
    #[arg(long)]
    pub estimate: Option<String>,
    /// Set parent task ID
    #[arg(long)]
    pub parent: Option<i32>,
    /// Set class of service
    #[arg(long)]
    pub class: Option<String>,
    /// Set body text
    #[arg(long, conflicts_with = "append_body")]
    pub body: Option<String>,
    /// Set branch
    #[arg(long)]
    pub branch: Option<String>,
    /// Set worktree
    #[arg(long)]
    pub worktree: Option<String>,
    /// Claim for agent
    #[arg(long, env = "KANBAN_AGENT")]
    pub claim: Option<String>,
    /// Release current claim
    #[arg(long)]
    pub release: bool,
    /// Block with reason
    #[arg(long)]
    pub block: Option<String>,
    /// Unblock
    #[arg(long)]
    pub unblock: bool,
    /// Add dependency
    #[arg(long)]
    pub depend: Option<i32>,
    /// Remove dependency
    #[arg(long)]
    pub undepend: Option<i32>,

    // --- Task #11: --append-body / --timestamp ---
    /// Append text to the body (instead of replacing)
    #[arg(long, short = 'a', conflicts_with = "body")]
    pub append_body: Option<String>,
    /// When used with --append-body, prefix appended text with [YYYY-MM-DD HH:MM] timestamp
    #[arg(long, short = 't', requires = "append_body")]
    pub timestamp: bool,

    // --- Task #12: --set-section / --section-body ---
    /// Create or replace a named ## section in the body
    #[arg(long, requires = "section_body")]
    pub set_section: Option<String>,
    /// The content for the section (required with --set-section)
    #[arg(long, requires = "set_section")]
    pub section_body: Option<String>,

    // --- Task #13: --add-tag / --remove-tag ---
    /// Add a single tag without replacing existing tags (comma-separated for multiple)
    #[arg(long, value_delimiter = ',')]
    pub add_tag: Option<Vec<String>>,
    /// Remove a single tag (comma-separated for multiple)
    #[arg(long, value_delimiter = ',')]
    pub remove_tag: Option<Vec<String>>,

    // --- Task #14: --clear-* flags ---
    /// Clear the due date
    #[arg(long)]
    pub clear_due: bool,
    /// Clear the parent task ID
    #[arg(long)]
    pub clear_parent: bool,
    /// Clear the branch
    #[arg(long)]
    pub clear_branch: bool,
    /// Clear the worktree
    #[arg(long)]
    pub clear_worktree: bool,

    // --- Task #15: --started / --completed / --clear-started / --clear-completed ---
    /// Set the started timestamp (YYYY-MM-DD)
    #[arg(long)]
    pub started: Option<String>,
    /// Set the completed timestamp (YYYY-MM-DD)
    #[arg(long)]
    pub completed: Option<String>,
    /// Clear the started timestamp
    #[arg(long)]
    pub clear_started: bool,
    /// Clear the completed timestamp
    #[arg(long)]
    pub clear_completed: bool,

    // --- Task #70: --status ---
    /// New status (runs same checks as `move`)
    #[arg(long, short = 's')]
    pub status: Option<String>,

    // --- Task #16: --force ---
    /// Override require_branch enforcement when editing tasks in branch-enforced status columns
    #[arg(long)]
    pub force: bool,
}

/// Result of a single edit operation.
struct EditResult {
    id: i32,
    title: String,
    task: task::Task,
}

pub fn run(cli: &Cli, args: EditArgs) -> Result<(), CliError> {
    if args.ids.is_empty() {
        return Err(CliError::new(
            ErrorCode::InvalidInput,
            "at least one task ID is required",
        ));
    }

    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;

    let mut results: Vec<EditResult> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for id_str in &args.ids {
        match edit_one(&cfg, &args, id_str) {
            Ok(result) => results.push(result),
            Err(e) => errors.push(format!("#{}: {}", id_str.trim_start_matches('#'), e)),
        }
    }

    // Output results.
    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            if results.len() == 1 {
                // Single task: output the task directly for backward compatibility.
                crate::output::json::json(&mut stdout, &results[0].task)
                    .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
            } else {
                let tasks: Vec<&task::Task> = results.iter().map(|r| &r.task).collect();
                crate::output::json::json(&mut stdout, &tasks)
                    .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
            }
        }
        Format::Compact => {
            for r in &results {
                writeln!(stdout, "#{} updated", r.id).unwrap_or(());
            }
        }
        Format::Table => {
            for r in &results {
                writeln!(stdout, "Updated task #{}: {}", r.id, r.title).unwrap_or(());
            }
        }
    }

    if !errors.is_empty() {
        for e in &errors {
            eprintln!("error: {e}");
        }
        return Err(CliError::newf(
            ErrorCode::InternalError,
            format!(
                "{} of {} edit(s) failed",
                errors.len(),
                args.ids.len()
            ),
        ));
    }

    Ok(())
}

fn edit_one(
    cfg: &crate::model::config::Config,
    args: &EditArgs,
    id_str: &str,
) -> Result<EditResult, CliError> {
    let id = super::helpers::parse_task_id(id_str)?;

    let (file_path, mut t) = super::helpers::load_task(cfg, id)?;

    // --- Task #16: --force branch enforcement check ---
    if cfg.status_requires_branch(&t.status) && !args.force {
        crate::cli::branch_check::enforce_branch_match(id, &t.branch, &t.status)?;
    }

    // --- Existing field updates ---
    if let Some(ref title) = args.title {
        t.title = title.clone();
    }
    if let Some(ref priority) = args.priority {
        t.priority = priority.clone();
    }
    if let Some(ref assignee) = args.assignee {
        t.assignee = assignee.clone();
    }
    if let Some(ref tags) = args.tags {
        t.tags = tags.clone();
    }
    if let Some(ref due) = args.due {
        t.due = Some(super::helpers::parse_date(due)?);
    }
    if let Some(ref estimate) = args.estimate {
        t.estimate = estimate.clone();
    }
    if let Some(parent) = args.parent {
        t.parent = Some(parent);
    }
    if let Some(ref class) = args.class {
        t.class = class.clone();
    }
    if let Some(ref body) = args.body {
        t.body = body.clone();
    }
    if let Some(ref branch) = args.branch {
        t.branch = branch.clone();
    }
    if let Some(ref worktree) = args.worktree {
        t.worktree = worktree.clone();
    }
    // Check claim conflict (warn to stderr, don't block).
    if let Some(ref claim) = args.claim {
        if !t.claimed_by.is_empty() && t.claimed_by != *claim {
            eprintln!("warning: task #{} is claimed by {:?}, overriding with {:?}", id, t.claimed_by, claim);
        }
        t.claimed_by = claim.clone();
        t.claimed_at = Some(Utc::now());
    }
    if args.release {
        t.claimed_by.clear();
        t.claimed_at = None;
    }
    if let Some(ref reason) = args.block {
        t.blocked = true;
        t.block_reason = reason.clone();
    }
    if args.unblock {
        t.blocked = false;
        t.block_reason.clear();
    }
    if let Some(dep) = args.depend {
        if !t.depends_on.contains(&dep) {
            t.depends_on.push(dep);
        }
    }
    if let Some(dep) = args.undepend {
        t.depends_on.retain(|d| *d != dep);
    }

    // --- Task #11: --append-body (after --body, mutually exclusive via clap) ---
    if let Some(ref text) = args.append_body {
        let line = if args.timestamp {
            let now = Utc::now().format("%Y-%m-%d %H:%M");
            format!("[{now}] {text}")
        } else {
            text.clone()
        };
        if t.body.is_empty() {
            t.body = line;
        } else {
            t.body.push('\n');
            t.body.push_str(&line);
        }
    }

    // --- Task #12: --set-section + --section-body ---
    if let Some(ref section_name) = args.set_section {
        let content = args.section_body.as_deref().unwrap_or("");
        t.body = task::set_section(&t.body, section_name, content);
    }

    // --- Task #13: --add-tag / --remove-tag (after --tags) ---
    if let Some(ref add_tags) = args.add_tag {
        for tag in add_tags {
            if !t.tags.contains(tag) {
                t.tags.push(tag.clone());
            }
        }
    }
    if let Some(ref remove_tags) = args.remove_tag {
        for tag in remove_tags {
            t.tags.retain(|existing| existing != tag);
        }
    }

    // --- Task #14: --clear-* flags (after their corresponding set operations) ---
    if args.clear_due {
        t.due = None;
    }
    if args.clear_parent {
        t.parent = None;
    }
    if args.clear_branch {
        t.branch.clear();
    }
    if args.clear_worktree {
        t.worktree.clear();
    }

    // --- Task #15: --started / --completed and --clear-started / --clear-completed ---
    if let Some(ref started_str) = args.started {
        let date = super::helpers::parse_date(started_str)?;
        let dt = date.and_time(NaiveTime::MIN);
        t.started = Some(Utc.from_utc_datetime(&dt));
    }
    if let Some(ref completed_str) = args.completed {
        let date = super::helpers::parse_date(completed_str)?;
        let dt = date.and_time(NaiveTime::MIN);
        t.completed = Some(Utc.from_utc_datetime(&dt));
    }
    if args.clear_started {
        t.started = None;
    }
    if args.clear_completed {
        t.completed = None;
    }

    // --- Task #70: --status (same enforcement checks as `move`) ---
    if let Some(ref new_status) = args.status {
        apply_status_change(cfg, &mut t, id, new_status, args.force, args.claim.is_some())?;
    }

    t.updated = Utc::now();
    super::helpers::save_task(&file_path, &t)?;

    Ok(EditResult {
        id,
        title: t.title.clone(),
        task: t,
    })
}

/// Validates and applies a status transition with full enforcement checks.
fn apply_status_change(
    cfg: &crate::model::config::Config,
    t: &mut task::Task,
    id: i32,
    new_status: &str,
    force: bool,
    has_claim: bool,
) -> Result<(), CliError> {
    let valid_statuses = cfg.status_names();
    if !valid_statuses.contains(&new_status.to_string()) {
        return Err(CliError::newf(
            ErrorCode::InvalidStatus,
            format!(
                "unknown status {:?} (valid: {})",
                new_status,
                valid_statuses.join(", ")
            ),
        ));
    }

    let old_status = t.status.clone();

    if cfg.status_requires_branch(new_status) && !force {
        crate::cli::branch_check::enforce_branch_match(id, &t.branch, new_status)?;
    }

    crate::cli::wip::enforce_wip_limits(cfg, &t.class, new_status, id)?;

    if cfg.status_requires_claim(new_status) && t.claimed_by.is_empty() && !has_claim {
        return Err(CliError::newf(
            ErrorCode::ClaimRequired,
            format!("status {:?} requires a claim (use --claim)", new_status),
        ));
    }

    if cfg.status_requires_branch(new_status) && t.branch.is_empty() && !force {
        return Err(CliError::newf(
            ErrorCode::StatusConflict,
            format!(
                "status {:?} requires a branch (use --force to override)",
                new_status
            ),
        ));
    }

    task::update_timestamps(t, &old_status, new_status, cfg);
    t.status = new_status.to_string();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::config::Config;

    fn test_config() -> Config {
        let dir = std::env::temp_dir().join(format!("kbmdx-test-edit-{}", std::process::id()));
        let tasks_dir = dir.join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        let mut cfg = Config::new_default("test");
        cfg.set_dir(dir);
        cfg
    }

    #[test]
    fn apply_status_change_valid() {
        let cfg = test_config();
        let mut t = task::Task {
            id: 1,
            status: "todo".to_string(),
            claimed_by: "agent".to_string(),
            ..Default::default()
        };
        // in-progress requires claim — task has one
        let result = apply_status_change(&cfg, &mut t, 1, "in-progress", false, false);
        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
        assert_eq!(t.status, "in-progress");
    }

    #[test]
    fn apply_status_change_invalid_status() {
        let cfg = test_config();
        let mut t = task::Task {
            id: 1,
            status: "todo".to_string(),
            ..Default::default()
        };
        let result = apply_status_change(&cfg, &mut t, 1, "nonexistent", false, false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidStatus);
    }

    #[test]
    fn apply_status_requires_claim() {
        let cfg = test_config();
        let mut t = task::Task {
            id: 1,
            status: "todo".to_string(),
            ..Default::default()
        };
        // in-progress requires claim — task has none
        let result = apply_status_change(&cfg, &mut t, 1, "in-progress", false, false);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::ClaimRequired);
    }

    #[test]
    fn apply_status_sets_started_timestamp() {
        let cfg = test_config();
        // "backlog" is the initial (first) status in default config
        let mut t = task::Task {
            id: 1,
            status: "backlog".to_string(),
            claimed_by: "agent".to_string(),
            ..Default::default()
        };
        assert!(t.started.is_none());
        // Moving from initial → non-initial sets started
        let result = apply_status_change(&cfg, &mut t, 1, "in-progress", false, false);
        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
        assert!(t.started.is_some(), "started should be set on first move out of initial status");
    }

    #[test]
    fn apply_status_sets_completed_on_terminal() {
        let cfg = test_config();
        let mut t = task::Task {
            id: 1,
            status: "in-progress".to_string(),
            started: Some(Utc::now()),
            ..Default::default()
        };
        // done doesn't require claim in default config
        let result = apply_status_change(&cfg, &mut t, 1, "done", false, false);
        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
        assert!(t.completed.is_some(), "completed should be set on terminal status");
    }
}
