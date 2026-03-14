//! `kbmdx move` — move a task to a new status.

use std::io::Write;
use chrono::Utc;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::task;
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct MoveArgs {
    /// Task ID
    pub id: String,
    /// Target status (optional when --next or --prev is used)
    pub status: Option<String>,
    /// Move to the next status in configured order
    #[arg(long, conflicts_with = "prev")]
    pub next: bool,
    /// Move to the previous status in configured order
    #[arg(long, conflicts_with = "next")]
    pub prev: bool,
    /// Claim the task during the move (set claimed_by and claimed_at)
    #[arg(long, env = "KANBAN_AGENT")]
    pub claim: Option<String>,
    /// Override require_branch enforcement on the target status
    #[arg(long)]
    pub force: bool,
}

pub fn run(cli: &Cli, args: MoveArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;

    let id = super::helpers::parse_task_id(&args.id)?;

    // Validate flag combinations: status arg conflicts with --next/--prev.
    if args.status.is_some() && (args.next || args.prev) {
        return Err(CliError::new(
            ErrorCode::InvalidInput,
            "cannot use positional status with --next or --prev",
        ));
    }
    if args.status.is_none() && !args.next && !args.prev {
        return Err(CliError::new(
            ErrorCode::InvalidInput,
            "target status is required (use a positional argument, --next, or --prev)",
        ));
    }

    let (file_path, mut t) = super::helpers::load_task(&cfg, id)?;

    // Resolve the target status.
    let valid_statuses = cfg.status_names();
    let new_status = if let Some(ref status) = args.status {
        // Explicit status — validate it.
        if !valid_statuses.contains(status) {
            return Err(CliError::newf(
                ErrorCode::InvalidStatus,
                format!(
                    "unknown status {:?} (valid: {})",
                    status,
                    valid_statuses.join(", ")
                ),
            ));
        }
        status.clone()
    } else {
        // --next or --prev: resolve relative to current status.
        let current_idx = cfg.status_index(&t.status).ok_or_else(|| {
            CliError::newf(
                ErrorCode::InvalidStatus,
                format!(
                    "task's current status {:?} not found in config",
                    t.status
                ),
            )
        })?;

        if args.next {
            if current_idx + 1 >= valid_statuses.len() {
                return Err(CliError::newf(
                    ErrorCode::BoundaryError,
                    format!(
                        "task #{} is already at the last status ({})",
                        id, t.status
                    ),
                ));
            }
            valid_statuses[current_idx + 1].clone()
        } else {
            // args.prev
            if current_idx == 0 {
                return Err(CliError::newf(
                    ErrorCode::BoundaryError,
                    format!(
                        "task #{} is already at the first status ({})",
                        id, t.status
                    ),
                ));
            }
            valid_statuses[current_idx - 1].clone()
        }
    };

    // Branch enforcement: check require_branch on the target status.
    if cfg.status_requires_branch(&new_status) && !args.force {
        let current_branch = crate::util::git::current_branch();
        let branch_ok = match current_branch.as_deref() {
            Some(branch) if !branch.is_empty() => {
                // Convention match: task/<ID>-<description>
                let convention_match = branch.strip_prefix("task/").and_then(|rest| {
                    let num_end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
                    if num_end == 0 {
                        return None;
                    }
                    if num_end < rest.len() && rest.as_bytes()[num_end] != b'-' {
                        return None;
                    }
                    rest[..num_end].parse::<i32>().ok()
                });
                if convention_match == Some(id) {
                    true
                } else if !t.branch.is_empty() && t.branch == branch {
                    // Exact match against task's branch field.
                    true
                } else if t.branch.is_empty() {
                    // No task branch set — cannot enforce.
                    true
                } else {
                    false
                }
            }
            _ => {
                // Not in a git repo or can't detect branch — skip enforcement.
                true
            }
        };

        if !branch_ok {
            let current = current_branch.unwrap_or_else(|| "<unknown>".to_string());
            return Err(CliError::newf(
                ErrorCode::StatusConflict,
                format!(
                    "task #{} requires branch match (status {:?} has require_branch); \
                     you're on {}, task is on {}. Use --force to override",
                    id, new_status, current, t.branch,
                ),
            ));
        }
    }

    // WIP limit enforcement (class-level board-wide + column-level).
    crate::cli::wip::enforce_wip_limits(&cfg, &t.class, &new_status, id)?;

    // Claim requirement enforcement on the target status.
    if cfg.status_requires_claim(&new_status) && t.claimed_by.is_empty() && args.claim.is_none() {
        return Err(CliError::newf(
            ErrorCode::ClaimRequired,
            format!("status {:?} requires a claim (use --claim)", new_status),
        ));
    }

    // Claim conflict warning.
    if let Some(ref claim) = args.claim {
        if !t.claimed_by.is_empty() && t.claimed_by != *claim {
            eprintln!("warning: task #{} is claimed by {:?}, overriding with {:?}", id, t.claimed_by, claim);
        }
    }

    // Branch requirement enforcement on the target status (separate from branch match check).
    if cfg.status_requires_branch(&new_status) && t.branch.is_empty() && !args.force {
        return Err(CliError::newf(
            ErrorCode::InvalidInput,
            format!("status {:?} requires a branch (use --force to override)", new_status),
        ));
    }

    let old_status = t.status.clone();
    task::update_timestamps(&mut t, &old_status, &new_status, &cfg);
    t.status = new_status.clone();
    t.updated = Utc::now();

    // Apply claim if requested.
    if let Some(ref claim) = args.claim {
        t.claimed_by = claim.clone();
        t.claimed_at = Some(Utc::now());
    }

    super::helpers::save_task(&file_path, &t)?;

    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            crate::output::json::json(&mut stdout, &t)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        Format::Compact => {
            writeln!(stdout, "#{id} {old_status} -> {}", new_status).unwrap_or(());
        }
        Format::Table => {
            writeln!(
                stdout,
                "Moved task #{id} from {} to {}",
                old_status, new_status
            )
            .unwrap_or(());
        }
    }
    Ok(())
}
