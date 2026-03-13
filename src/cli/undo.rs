//! `kanban-md undo` / `kanban-md redo` — undo/redo last mutation.

use std::io::Write;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct UndoArgs {
    /// Show what would be undone without doing it
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(clap::Args, Clone)]
pub struct RedoArgs {
    /// Show what would be redone without doing it
    #[arg(long)]
    pub dry_run: bool,
}

pub fn run_undo(cli: &Cli, args: UndoArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;

    let mut stack = crate::board::undo::load_stack(cfg.dir());

    if stack.undo.is_empty() {
        return Err(CliError::new(ErrorCode::NoChanges, "nothing to undo"));
    }

    let snapshot = stack.undo.last().unwrap().clone();

    if args.dry_run {
        let mut stdout = std::io::stdout();
        match format {
            Format::Json => {
                crate::output::json::json(&mut stdout, &snapshot)
                    .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
            }
            _ => {
                writeln!(stdout, "Would undo: {}", snapshot.action).unwrap_or(());
                for f in &snapshot.files {
                    writeln!(stdout, "  restore: {}", f.path).unwrap_or(());
                }
            }
        }
        return Ok(());
    }

    let snapshot = stack.undo.pop().unwrap();
    let reverse = crate::board::undo::restore_snapshot(&snapshot)
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
    stack.redo.push(reverse);
    crate::board::undo::save_stack(cfg.dir(), &stack)
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;

    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            let result = serde_json::json!({"action": "undo", "undone": snapshot.action});
            crate::output::json::json(&mut stdout, &result)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        _ => {
            writeln!(stdout, "Undone: {}", snapshot.action).unwrap_or(());
        }
    }
    Ok(())
}

pub fn run_redo(cli: &Cli, args: RedoArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;

    let mut stack = crate::board::undo::load_stack(cfg.dir());

    if stack.redo.is_empty() {
        return Err(CliError::new(ErrorCode::NoChanges, "nothing to redo"));
    }

    let snapshot = stack.redo.last().unwrap().clone();

    if args.dry_run {
        let mut stdout = std::io::stdout();
        match format {
            Format::Json => {
                crate::output::json::json(&mut stdout, &snapshot)
                    .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
            }
            _ => {
                writeln!(stdout, "Would redo: {}", snapshot.action).unwrap_or(());
            }
        }
        return Ok(());
    }

    let snapshot = stack.redo.pop().unwrap();
    let reverse = crate::board::undo::restore_snapshot(&snapshot)
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
    stack.undo.push(reverse);
    crate::board::undo::save_stack(cfg.dir(), &stack)
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;

    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            let result = serde_json::json!({"action": "redo", "redone": snapshot.action});
            crate::output::json::json(&mut stdout, &result)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        _ => {
            writeln!(stdout, "Redone: {}", snapshot.action).unwrap_or(());
        }
    }
    Ok(())
}
