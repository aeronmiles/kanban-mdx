//! `kanban-md board` — show board summary.

use std::io::Write;
use std::time::Duration;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::config::Config;
use crate::model::task;
use crate::output::Format;

#[derive(clap::Args, Clone)]
#[command(visible_alias = "summary")]
pub struct BoardArgs {
    /// Watch for file changes and re-render the board on updates.
    #[arg(long, short = 'w')]
    pub watch: bool,
    /// Group by field
    #[arg(long)]
    pub group_by: Option<String>,
    /// Filter by parent task ID
    #[arg(long)]
    pub parent: Option<i32>,
}

/// Render the board to stdout once, reading tasks from disk.
fn render_board(
    cfg: &Config,
    format: &Format,
    parent: Option<i32>,
    group_by: Option<&str>,
) -> Result<(), CliError> {
    let (all_tasks, _) = task::read_all_lenient(&cfg.tasks_path())
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;

    // Filter by parent if requested
    let tasks: Vec<task::Task> = if let Some(parent_id) = parent {
        all_tasks.into_iter().filter(|t| t.parent == Some(parent_id)).collect()
    } else {
        all_tasks
    };

    // Grouped view
    if let Some(field) = group_by {
        let group_field: crate::board::GroupField = field.parse().map_err(|e: String| {
            CliError::newf(ErrorCode::InvalidInput, e)
        })?;
        let board_grouped = crate::board::group_by_summary(&tasks, group_field, cfg);
        // Convert board types to output types (they have slightly different StatusSummary).
        let grouped = crate::output::types::GroupedSummary {
            groups: board_grouped
                .groups
                .into_iter()
                .map(|g| crate::output::types::GroupSummary {
                    key: g.key,
                    total: g.total,
                    statuses: g
                        .statuses
                        .into_iter()
                        .map(|s| crate::output::types::StatusSummary {
                            status: s.status,
                            count: s.count,
                            wip_limit: s.wip_limit,
                            blocked: 0,
                            overdue: 0,
                        })
                        .collect(),
                })
                .collect(),
        };
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

    // Default: group by status for board view
    let board_statuses = cfg.board_statuses();
    let mut stdout = std::io::stdout();

    match format {
        Format::Json => {
            let mut columns = Vec::new();
            for status in &board_statuses {
                let status_tasks: Vec<&task::Task> = tasks.iter().filter(|t| t.status == *status).collect();
                columns.push(serde_json::json!({
                    "status": status,
                    "count": status_tasks.len(),
                    "wip_limit": cfg.wip_limit(status),
                    "tasks": status_tasks,
                }));
            }
            let result = serde_json::json!({
                "board": cfg.board.name,
                "columns": columns,
            });
            crate::output::json::json(&mut stdout, &result)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        Format::Compact => {
            for status in &board_statuses {
                let count = tasks.iter().filter(|t| t.status == *status).count();
                let wip = cfg.wip_limit(status);
                if wip > 0 {
                    writeln!(stdout, "{status} {count}/{wip}").unwrap_or(());
                } else {
                    writeln!(stdout, "{status} {count}").unwrap_or(());
                }
            }
        }
        Format::Table => {
            writeln!(stdout, "Board: {}", cfg.board.name).unwrap_or(());
            writeln!(stdout).unwrap_or(());
            for status in &board_statuses {
                let count = tasks.iter().filter(|t| t.status == *status).count();
                let wip = cfg.wip_limit(status);
                let wip_str = if wip > 0 {
                    format!(" [{count}/{wip}]")
                } else {
                    format!(" [{count}]")
                };
                writeln!(stdout, "  {status}{wip_str}").unwrap_or(());
                for t in tasks.iter().filter(|t| t.status == *status) {
                    let claimed = if t.claimed_by.is_empty() {
                        String::new()
                    } else {
                        format!(" @{}", t.claimed_by)
                    };
                    writeln!(stdout, "    #{} {} [{}]{}", t.id, t.title, t.priority, claimed).unwrap_or(());
                }
            }
        }
    }
    Ok(())
}

pub fn run(cli: &Cli, args: BoardArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;

    // Initial render
    render_board(&cfg, &format, args.parent, args.group_by.as_deref())?;

    if args.watch {
        let watcher = crate::watcher::Watcher::new(&cfg.tasks_path())
            .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("failed to start watcher: {e}")))?;

        loop {
            match watcher.events().recv_timeout(Duration::from_millis(500)) {
                Ok(crate::watcher::WatchEvent::Reload) => {
                    // Clear screen and move cursor to top-left
                    print!("\x1b[2J\x1b[H");
                    std::io::stdout().flush()
                        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;

                    // Reload config to pick up any config changes too
                    let cfg = crate::cli::root::load_config(cli)?;
                    render_board(&cfg, &format, args.parent, args.group_by.as_deref())?;
                }
                Err(_) => {
                    // Timeout or disconnect — just continue the loop.
                    // On disconnect (watcher dropped), this will spin harmlessly
                    // until the process is killed via ctrl+c.
                    continue;
                }
            }
        }
    }

    Ok(())
}
