//! `kanban-md handoff` — hand off a task to another agent.

use std::io::Write;
use chrono::Utc;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::task;
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct HandoffArgs {
    /// Task ID
    pub id: String,
    /// New claim owner
    #[arg(long, env = "KANBAN_AGENT")]
    pub claim: Option<String>,
    /// Handoff note to append to body
    #[arg(long)]
    pub note: Option<String>,
    /// Prepend timestamp to note
    #[arg(long, short = 't')]
    pub timestamp: bool,
    /// Block reason
    #[arg(long)]
    pub block: Option<String>,
    /// Release current claim
    #[arg(long)]
    pub release: bool,
}

pub fn run(cli: &Cli, args: HandoffArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;
    let tasks_path = cfg.tasks_path();

    let id: i32 = args.id.trim_start_matches('#').parse().map_err(|_| {
        CliError::newf(ErrorCode::InvalidTaskId, format!("invalid task ID: {}", args.id))
    })?;

    let file_path = task::find_by_id(&tasks_path, id)
        .map_err(|e| CliError::newf(ErrorCode::TaskNotFound, format!("{e}")))?;
    let mut t = task::read(&file_path)
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;

    let now = Utc::now();

    if let Some(ref note) = args.note {
        let text = if args.timestamp {
            format!("\n\n**{}** {}", now.format("%Y-%m-%d %H:%M UTC"), note)
        } else {
            format!("\n\n{note}")
        };
        t.body.push_str(&text);
    }

    if let Some(ref reason) = args.block {
        t.blocked = true;
        t.block_reason = reason.clone();
    }

    if args.release {
        t.claimed_by.clear();
        t.claimed_at = None;
    }

    if let Some(ref claim) = args.claim {
        t.claimed_by = claim.clone();
        t.claimed_at = Some(now);
    }

    t.updated = now;
    task::write(&file_path, &t)
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("failed to write: {e}")))?;

    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            crate::output::json::json(&mut stdout, &t)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        Format::Compact => {
            writeln!(stdout, "#{id} handoff ok").unwrap_or(());
        }
        Format::Table => {
            writeln!(stdout, "Handed off task #{id}").unwrap_or(());
        }
    }
    Ok(())
}
