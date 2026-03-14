//! `kbmdx log` — show mutation log.

use std::io::Write;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct LogArgs {
    /// Show log entries since this date
    #[arg(long)]
    pub since: Option<String>,
    /// Limit number of entries
    #[arg(long)]
    pub limit: Option<usize>,
    /// Filter by action type
    #[arg(long)]
    pub action: Option<String>,
    /// Filter by task ID
    #[arg(long)]
    pub task: Option<i32>,
}

pub fn run(cli: &Cli, args: LogArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;

    let log_path = cfg.dir().join(".log.json");
    let entries = crate::board::log::load(&log_path)
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;

    let mut filtered: Vec<&crate::board::log::LogEntry> = entries.iter().collect();

    if let Some(ref action) = args.action {
        filtered.retain(|e| e.action == *action);
    }
    if let Some(task_id) = args.task {
        filtered.retain(|e| e.task_id == task_id);
    }

    // Reverse chronological
    filtered.reverse();

    if let Some(limit) = args.limit {
        filtered.truncate(limit);
    }

    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            crate::output::json::json(&mut stdout, &filtered)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        Format::Compact => {
            for entry in &filtered {
                writeln!(
                    stdout,
                    "{} #{} {} {}",
                    entry.timestamp.format("%Y-%m-%d %H:%M"),
                    entry.task_id,
                    entry.action,
                    entry.detail,
                )
                .unwrap_or(());
            }
        }
        Format::Table => {
            if filtered.is_empty() {
                writeln!(stdout, "No log entries.").unwrap_or(());
            } else {
                for entry in &filtered {
                    writeln!(
                        stdout,
                        "{} | #{:<4} | {:<12} | {}",
                        entry.timestamp.format("%Y-%m-%d %H:%M"),
                        entry.task_id,
                        entry.action,
                        entry.detail,
                    )
                    .unwrap_or(());
                }
            }
        }
    }
    Ok(())
}
