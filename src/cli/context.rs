//! `kanban-md context` — generate board context markdown for agents.

use std::fmt::Write as FmtWrite;
use std::io::Write;

use chrono::{Duration as ChronoDuration, NaiveDate, Utc};

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::task;

/// Valid section names for the `--sections` flag.
const VALID_SECTIONS: &[&str] = &[
    "in-progress",
    "blocked",
    "overdue",
    "recently-completed",
];

#[derive(clap::Args, Clone)]
pub struct ContextArgs {
    /// Write output to a file instead of stdout.
    #[arg(long, value_name = "FILE")]
    pub write_to: Option<String>,

    /// Comma-separated list of sections to include.
    /// Valid: in-progress, blocked, overdue, recently-completed.
    /// Default: show all.
    #[arg(long, value_name = "SECTIONS", value_delimiter = ',')]
    pub sections: Option<Vec<String>>,

    /// Lookback period in days for "recently completed" section (default: 7).
    #[arg(long, value_name = "N", default_value = "7")]
    pub days: i64,
}

pub fn run(cli: &Cli, args: ContextArgs) -> Result<(), CliError> {
    let cfg = crate::cli::root::load_config(cli)?;

    let (all_tasks, _) = task::read_all_lenient(&cfg.tasks_path())
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;

    // Determine which sections to render.
    let selected_sections = resolve_sections(&args)?;

    let today = Utc::now().date_naive();
    let lookback = ChronoDuration::days(args.days);
    let cutoff = Utc::now() - lookback;

    let active_statuses = cfg.active_statuses();

    let mut buf = String::new();

    writeln!(buf, "# Board: {}", cfg.board.name)
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
    writeln!(buf).unwrap_or(());

    // -- In Progress --
    if selected_sections.contains(&"in-progress".to_string()) {
        let in_progress: Vec<&task::Task> = all_tasks
            .iter()
            .filter(|t| active_statuses.contains(&t.status))
            .collect();

        writeln!(buf, "## In Progress ({})", in_progress.len()).unwrap_or(());
        writeln!(buf).unwrap_or(());

        if in_progress.is_empty() {
            writeln!(buf, "_none_").unwrap_or(());
        } else {
            for t in &in_progress {
                let claimed = if t.claimed_by.is_empty() {
                    String::new()
                } else {
                    format!(" @{}", t.claimed_by)
                };
                let tags = if t.tags.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", t.tags.join(", "))
                };
                writeln!(buf, "- #{} {} [{}]{}{}", t.id, t.title, t.priority, claimed, tags)
                    .unwrap_or(());
            }
        }
        writeln!(buf).unwrap_or(());
    }

    // -- Blocked --
    if selected_sections.contains(&"blocked".to_string()) {
        let blocked: Vec<&task::Task> = all_tasks.iter().filter(|t| t.blocked).collect();

        writeln!(buf, "## Blocked ({})", blocked.len()).unwrap_or(());
        writeln!(buf).unwrap_or(());

        if blocked.is_empty() {
            writeln!(buf, "_none_").unwrap_or(());
        } else {
            for t in &blocked {
                let reason = if t.block_reason.is_empty() {
                    String::new()
                } else {
                    format!(" -- {}", t.block_reason)
                };
                writeln!(buf, "- #{} {} [{}]{}", t.id, t.title, t.priority, reason)
                    .unwrap_or(());
            }
        }
        writeln!(buf).unwrap_or(());
    }

    // -- Overdue --
    if selected_sections.contains(&"overdue".to_string()) {
        let overdue: Vec<&task::Task> = all_tasks
            .iter()
            .filter(|t| {
                if cfg.is_terminal_status(&t.status) {
                    return false;
                }
                match t.due {
                    Some(due) => due < today,
                    None => false,
                }
            })
            .collect();

        writeln!(buf, "## Overdue ({})", overdue.len()).unwrap_or(());
        writeln!(buf).unwrap_or(());

        if overdue.is_empty() {
            writeln!(buf, "_none_").unwrap_or(());
        } else {
            for t in &overdue {
                let due_str = format_due(t.due);
                writeln!(buf, "- #{} {} [{}] due:{}", t.id, t.title, t.priority, due_str)
                    .unwrap_or(());
            }
        }
        writeln!(buf).unwrap_or(());
    }

    // -- Recently Completed --
    if selected_sections.contains(&"recently-completed".to_string()) {
        let recent: Vec<&task::Task> = all_tasks
            .iter()
            .filter(|t| match t.completed {
                Some(completed) => completed >= cutoff,
                None => false,
            })
            .collect();

        writeln!(
            buf,
            "## Recently Completed ({}, last {} days)",
            recent.len(),
            args.days
        )
        .unwrap_or(());
        writeln!(buf).unwrap_or(());

        if recent.is_empty() {
            writeln!(buf, "_none_").unwrap_or(());
        } else {
            for t in &recent {
                let completed_str = match t.completed {
                    Some(dt) => dt.format("%Y-%m-%d").to_string(),
                    None => "unknown".to_string(),
                };
                writeln!(
                    buf,
                    "- #{} {} [completed {}]",
                    t.id, t.title, completed_str
                )
                .unwrap_or(());
            }
        }
        writeln!(buf).unwrap_or(());
    }

    // Output the result.
    if let Some(ref path) = args.write_to {
        std::fs::write(path, &buf)
            .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("writing to {path}: {e}")))?;

        let mut stdout = std::io::stdout();
        writeln!(stdout, "{path}")
            .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
    } else {
        let mut stdout = std::io::stdout();
        write!(stdout, "{buf}")
            .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
    }

    Ok(())
}

/// Resolves the requested sections from CLI args. If `--sections` is not set,
/// returns all valid sections.
fn resolve_sections(args: &ContextArgs) -> Result<Vec<String>, CliError> {
    match &args.sections {
        None => Ok(VALID_SECTIONS.iter().map(|s| s.to_string()).collect()),
        Some(requested) => {
            for s in requested {
                if !VALID_SECTIONS.contains(&s.as_str()) {
                    return Err(CliError::newf(
                        ErrorCode::InvalidInput,
                        format!(
                            "unknown section {:?}; valid sections: {}",
                            s,
                            VALID_SECTIONS.join(", ")
                        ),
                    ));
                }
            }
            Ok(requested.clone())
        }
    }
}

/// Formats an optional NaiveDate for display.
fn format_due(due: Option<NaiveDate>) -> String {
    match due {
        Some(d) => d.format("%Y-%m-%d").to_string(),
        None => "unknown".to_string(),
    }
}
