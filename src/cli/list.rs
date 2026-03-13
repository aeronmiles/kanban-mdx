//! `kanban-md list` — list tasks with optional filters.

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::config::ARCHIVED_STATUS;
use crate::model::task;
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct ListArgs {
    /// Filter by status
    #[arg(long, short = 's')]
    pub status: Option<String>,
    /// Filter by priority
    #[arg(long, short = 'p')]
    pub priority: Option<String>,
    /// Filter by assignee
    #[arg(long)]
    pub assignee: Option<String>,
    /// Filter by tag
    #[arg(long)]
    pub tag: Option<String>,
    /// Filter by claimed agent
    #[arg(long)]
    pub claimed: Option<String>,
    /// Show only unclaimed tasks
    #[arg(long)]
    pub unclaimed: bool,
    /// Filter by parent task ID
    #[arg(long)]
    pub parent: Option<i32>,
    /// Search string (substring match in title/body)
    #[arg(long)]
    pub search: Option<String>,
    /// Suppress body in output
    #[arg(long)]
    pub no_body: bool,

    // -- Task #18: Advanced filter flags --

    /// Filter by specific task IDs (comma-separated, e.g. --id 1,2,3)
    #[arg(long, value_delimiter = ',')]
    pub id: Vec<i32>,
    /// Show only blocked tasks
    #[arg(long, conflicts_with = "not_blocked")]
    pub blocked: bool,
    /// Show only unblocked tasks (not flagged as blocked)
    #[arg(long, conflicts_with = "blocked")]
    pub not_blocked: bool,
    /// Show only tasks with all dependencies satisfied
    #[arg(long)]
    pub unblocked: bool,
    /// Filter by class of service
    #[arg(long)]
    pub class: Option<String>,
    /// Include archived tasks (by default, archived are excluded)
    #[arg(long)]
    pub archived: bool,

    // -- Task #19: Sort / group / limit flags --

    /// Group output by field (assignee, tag, class, priority, status)
    #[arg(long)]
    pub group_by: Option<String>,
    /// Sort by field (id, status, priority, created, updated, due). Default: id
    #[arg(long, default_value = "id")]
    pub sort: String,
    /// Reverse the sort order
    #[arg(long)]
    pub reverse: bool,
    /// Maximum number of results to return
    #[arg(long)]
    pub limit: Option<usize>,

    // -- Task #20: Worktree filter flags --

    /// Auto-filter to current worktree's tasks (uses current branch)
    #[arg(long, short = 'C')]
    pub context: bool,
    /// Filter by branch glob pattern (e.g. task/42-*)
    #[arg(long)]
    pub branch: Option<String>,
    /// Only show tasks with a worktree set
    #[arg(long, conflicts_with = "no_worktree")]
    pub has_worktree: bool,
    /// Only show tasks without a worktree set
    #[arg(long, conflicts_with = "has_worktree")]
    pub no_worktree: bool,
}

pub fn run(cli: &Cli, args: ListArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;

    let (all_tasks, _) = task::read_all_lenient(&cfg.tasks_path())
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;

    // -- Validate --group-by field if provided --
    let group_field = if let Some(ref field) = args.group_by {
        Some(field.parse::<crate::board::group::GroupField>().map_err(|e| {
            CliError::newf(ErrorCode::InvalidInput, e)
        })?)
    } else {
        None
    };

    // -- Resolve --context flag to a branch pattern --
    let branch_filter = if args.context {
        match crate::util::git::current_branch() {
            Some(b) if !b.is_empty() => Some(b),
            _ => {
                return Err(CliError::newf(
                    ErrorCode::InternalError,
                    "could not determine current git branch for --context".to_string(),
                ));
            }
        }
    } else {
        args.branch.clone()
    };

    // -- Resolve blocked filter --
    let blocked_filter = if args.blocked {
        Some(true)
    } else if args.not_blocked {
        Some(false)
    } else {
        None
    };

    // -- Resolve has_worktree filter --
    let has_worktree_filter = if args.has_worktree {
        Some(true)
    } else if args.no_worktree {
        Some(false)
    } else {
        None
    };

    // -- Exclude archived by default unless --archived is set --
    let exclude_statuses = if args.archived {
        vec![]
    } else {
        vec![ARCHIVED_STATUS.to_string()]
    };

    // -- Parse --sort field --
    let sort_field = args.sort.parse::<crate::board::sort::SortField>().map_err(|e| {
        CliError::newf(ErrorCode::InvalidInput, e)
    })?;

    // -- Build ListOptions (filter + sort + limit + unblocked) --
    let list_opts = crate::board::list::ListOptions {
        filter: crate::board::filter::FilterOptions {
            ids: args.id,
            statuses: args.status.into_iter().collect(),
            exclude_statuses,
            priorities: args.priority.into_iter().collect(),
            assignee: args.assignee,
            tag: args.tag,
            claimed_by: args.claimed,
            unclaimed: args.unclaimed,
            parent_id: args.parent,
            search: args.search,
            blocked: blocked_filter,
            class: args.class,
            branch: branch_filter,
            has_worktree: has_worktree_filter,
            ..Default::default()
        },
        sort_by: sort_field,
        reverse: args.reverse,
        limit: args.limit,
        unblocked: args.unblocked,
    };

    // -- If --group-by is specified, use the grouped output path --
    if let Some(field) = group_field {
        let filtered = crate::board::list::list(&cfg, &all_tasks, &list_opts);
        let owned: Vec<task::Task> = filtered.into_iter().cloned().collect();
        let board_grouped = crate::board::group::group_by_summary(&owned, field, &cfg);

        // Convert board::group::GroupedSummary -> output::types::GroupedSummary
        let grouped = convert_grouped_summary(&board_grouped);

        let mut stdout = std::io::stdout();
        match format {
            Format::Json => {
                crate::output::json::json(&mut stdout, &grouped)
                    .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
            }
            Format::Compact => {
                crate::output::compact::grouped_compact(&mut stdout, &grouped);
            }
            Format::Table => {
                crate::output::table::grouped_table(&mut stdout, &grouped);
            }
        }
        return Ok(());
    }

    // -- Standard (non-grouped) output path --
    let filtered = crate::board::list::list(&cfg, &all_tasks, &list_opts);

    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            crate::output::json::json(&mut stdout, &filtered)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        Format::Compact => {
            let owned: Vec<task::Task> = filtered.into_iter().cloned().collect();
            crate::output::compact::task_compact(&mut stdout, &owned);
        }
        Format::Table => {
            let owned: Vec<task::Task> = filtered.into_iter().cloned().collect();
            crate::output::table::task_table(&mut stdout, &owned);
        }
    }
    Ok(())
}

/// Converts `board::group::GroupedSummary` to `output::types::GroupedSummary`.
///
/// The board module's `StatusSummary` has `{status, count, wip_limit}` while
/// the output module's version adds `blocked` and `overdue` fields (set to 0
/// here since they are not tracked at the group level).
fn convert_grouped_summary(
    src: &crate::board::group::GroupedSummary,
) -> crate::output::types::GroupedSummary {
    crate::output::types::GroupedSummary {
        groups: src
            .groups
            .iter()
            .map(|g| crate::output::types::GroupSummary {
                key: g.key.clone(),
                total: g.total,
                statuses: g
                    .statuses
                    .iter()
                    .map(|ss| crate::output::types::StatusSummary {
                        status: ss.status.clone(),
                        count: ss.count,
                        wip_limit: ss.wip_limit,
                        blocked: 0,
                        overdue: 0,
                    })
                    .collect(),
            })
            .collect(),
    }
}
