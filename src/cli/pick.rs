//! `kbmdx pick` — pick the highest-priority unclaimed task.

use chrono::Utc;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::task;
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct PickArgs {
    /// Claim for agent
    #[arg(long, env = "KANBAN_AGENT")]
    pub claim: Option<String>,
    /// Preferred status to pick from
    #[arg(long)]
    pub status: Option<String>,
    /// Move picked task to this status
    #[arg(long, name = "move")]
    pub move_to: Option<String>,
    /// Required tags (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub tags: Vec<String>,
    /// Suppress body in output
    #[arg(long)]
    pub no_body: bool,
}

pub fn run(cli: &Cli, args: PickArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;

    let (all_tasks, _) = task::read_all_lenient(&cfg.tasks_path())
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;

    // Build filter
    let mut filter_opts = crate::board::filter::FilterOptions {
        unclaimed: true,
        exclude_statuses: vec![crate::model::config::ARCHIVED_STATUS.to_string()],
        ..Default::default()
    };

    if let Some(ref status) = args.status {
        filter_opts.statuses = vec![status.clone()];
    }

    let candidates = crate::board::filter::filter(&all_tasks, &filter_opts);

    // Filter by tags
    let candidates: Vec<&task::Task> = if args.tags.is_empty() {
        candidates
    } else {
        candidates
            .into_iter()
            .filter(|t| args.tags.iter().all(|tag| t.tags.contains(tag)))
            .collect()
    };

    // Filter out blocked
    let unblocked = crate::board::filter::filter_unblocked(&candidates, &all_tasks, &cfg);

    if unblocked.is_empty() {
        return Err(CliError::new(ErrorCode::NothingToPick, "no tasks available to pick"));
    }

    // Sort by priority (highest first), then by ID (oldest first)
    let mut sorted = unblocked;
    sorted.sort_by(|a, b| {
        cfg.priority_index(&b.priority)
            .cmp(&cfg.priority_index(&a.priority))
            .then_with(|| a.id.cmp(&b.id))
    });

    let picked = sorted[0];
    let id = picked.id;

    // Apply claim and move if requested
    let (file_path, mut t) = super::helpers::load_task(&cfg, id)?;

    if let Some(ref claim) = args.claim {
        t.claimed_by = claim.clone();
        t.claimed_at = Some(Utc::now());
    }

    // Auto-populate branch and worktree from git context.
    if t.branch.is_empty() {
        if let Some(branch) = crate::util::git::current_branch() {
            t.branch = branch;
        }
    }
    if t.worktree.is_empty() {
        if let Ok(cwd) = std::env::current_dir() {
            if crate::util::git::is_worktree(&cwd) {
                t.worktree = cwd.to_string_lossy().to_string();
            }
        }
    }

    if let Some(ref move_to) = args.move_to {
        let old_status = t.status.clone();
        task::update_timestamps(&mut t, &old_status, move_to, &cfg);
        t.status = move_to.clone();
    }

    t.updated = Utc::now();
    super::helpers::save_task(&file_path, &t)?;

    if args.no_body {
        t.body.clear();
    }

    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            crate::output::json::json(&mut stdout, &t)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        Format::Compact => {
            crate::output::compact::task_detail_compact(&mut stdout, &t);
        }
        Format::Table => {
            crate::output::table::task_detail(&mut stdout, &t);
        }
    }
    Ok(())
}
