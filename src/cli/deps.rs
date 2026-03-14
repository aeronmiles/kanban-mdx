//! `kbmdx deps` — show task dependencies.

use std::io::Write;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::task;
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct DepsArgs {
    /// Task ID
    pub id: String,
    /// Show upstream (blocking) dependencies
    #[arg(long)]
    pub upstream: bool,
    /// Show downstream (dependent) tasks
    #[arg(long)]
    pub downstream: bool,
    /// Include transitive dependencies
    #[arg(long)]
    pub transitive: bool,
}

pub fn run(cli: &Cli, args: DepsArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;

    let id = super::helpers::parse_task_id(&args.id)?;

    let (all_tasks, _) = task::read_all_lenient(&cfg.tasks_path())
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;

    let deps = if args.downstream {
        crate::board::deps::downstream(&all_tasks, id, args.transitive)
    } else {
        // Default to upstream
        crate::board::deps::upstream(&all_tasks, id, args.transitive)
    };

    let mut stdout = std::io::stdout();
    let direction = if args.downstream { "downstream" } else { "upstream" };

    match format {
        Format::Json => {
            let result = serde_json::json!({
                "task_id": id,
                "direction": direction,
                "transitive": args.transitive,
                "dependencies": deps,
            });
            crate::output::json::json(&mut stdout, &result)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        Format::Compact => {
            let owned: Vec<task::Task> = deps.into_iter().cloned().collect();
            crate::output::compact::task_compact(&mut stdout, &owned);
        }
        Format::Table => {
            if deps.is_empty() {
                writeln!(stdout, "No {direction} dependencies for task #{id}").unwrap_or(());
            } else {
                writeln!(stdout, "{} dependencies for task #{id}:", direction).unwrap_or(());
                let owned: Vec<task::Task> = deps.into_iter().cloned().collect();
                crate::output::table::task_table(&mut stdout, &owned);
            }
        }
    }
    Ok(())
}
