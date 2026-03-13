//! `kanban-md delete` — delete one or more tasks.

use std::io::Write;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::task;
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct DeleteArgs {
    /// Task ID(s)
    pub ids: Vec<String>,
    /// Skip confirmation
    #[arg(long, short = 'y')]
    pub yes: bool,
}

/// Result of a single delete operation (for JSON output).
#[derive(serde::Serialize)]
struct DeleteResult {
    action: &'static str,
    id: i32,
    title: String,
}

pub fn run(cli: &Cli, args: DeleteArgs) -> Result<(), CliError> {
    if args.ids.is_empty() {
        return Err(CliError::new(
            ErrorCode::InvalidInput,
            "at least one task ID is required",
        ));
    }

    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;

    let mut results: Vec<DeleteResult> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for id_str in &args.ids {
        match delete_one(&cfg, id_str) {
            Ok(result) => results.push(result),
            Err(e) => errors.push(format!("#{}: {}", id_str.trim_start_matches('#'), e)),
        }
    }

    // Output results.
    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            crate::output::json::json(&mut stdout, &results)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        Format::Compact => {
            for r in &results {
                writeln!(stdout, "#{} deleted", r.id).unwrap_or(());
            }
        }
        Format::Table => {
            for r in &results {
                writeln!(stdout, "Deleted task #{}: {}", r.id, r.title).unwrap_or(());
            }
        }
    }

    if !errors.is_empty() {
        for e in &errors {
            eprintln!("error: {e}");
        }
        return Err(CliError::newf(
            ErrorCode::InternalError,
            format!(
                "{} of {} delete(s) failed",
                errors.len(),
                args.ids.len()
            ),
        ));
    }

    Ok(())
}

fn delete_one(
    cfg: &crate::model::config::Config,
    id_str: &str,
) -> Result<DeleteResult, String> {
    let id: i32 = id_str
        .trim_start_matches('#')
        .parse()
        .map_err(|_| format!("invalid task ID: {id_str}"))?;

    let file_path =
        task::find_by_id(&cfg.tasks_path(), id).map_err(|e| format!("{e}"))?;
    let t =
        task::read(&file_path).map_err(|e| format!("{e}"))?;

    std::fs::remove_file(&file_path)
        .map_err(|e| format!("deleting task file: {e}"))?;

    Ok(DeleteResult {
        action: "delete",
        id,
        title: t.title,
    })
}
