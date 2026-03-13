//! `kanban-md tui` — launch the terminal UI.

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::task;

#[derive(clap::Args, Clone)]
pub struct TuiArgs {
    /// Hide empty columns in TUI (overrides config).
    #[arg(long, conflicts_with = "show_empty_columns")]
    pub hide_empty_columns: bool,

    /// Show empty columns in TUI (overrides config).
    #[arg(long, conflicts_with = "hide_empty_columns")]
    pub show_empty_columns: bool,
}

pub fn run(cli: &Cli, args: TuiArgs) -> Result<(), CliError> {
    let mut cfg = crate::cli::root::load_config(cli)?;

    // Apply CLI flag overrides for hide_empty_columns.
    if args.hide_empty_columns {
        cfg.tui.hide_empty_columns = true;
    } else if args.show_empty_columns {
        cfg.tui.hide_empty_columns = false;
    }

    let (tasks, _warnings) = task::read_all_lenient(&cfg.tasks_path())
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;

    crate::tui::run_tui(cfg, tasks)
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("TUI error: {e}")))?;

    Ok(())
}
