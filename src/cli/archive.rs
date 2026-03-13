//! `kanban-md archive` — archive one or more completed tasks.

use std::io::Write;
use chrono::Utc;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::config::ARCHIVED_STATUS;
use crate::model::task;
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct ArchiveArgs {
    /// Task ID(s)
    pub ids: Vec<String>,
}

/// Result of a single archive operation.
struct ArchiveResult {
    id: i32,
    title: String,
    old_status: String,
    task: task::Task,
}

pub fn run(cli: &Cli, args: ArchiveArgs) -> Result<(), CliError> {
    if args.ids.is_empty() {
        return Err(CliError::new(
            ErrorCode::InvalidInput,
            "at least one task ID is required",
        ));
    }

    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;

    let mut results: Vec<ArchiveResult> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for id_str in &args.ids {
        match archive_one(&cfg, id_str) {
            Ok(result) => results.push(result),
            Err(e) => errors.push(format!("#{}: {}", id_str.trim_start_matches('#'), e)),
        }
    }

    // Output results.
    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            let tasks: Vec<&task::Task> = results.iter().map(|r| &r.task).collect();
            crate::output::json::json(&mut stdout, &tasks)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        Format::Compact => {
            for r in &results {
                writeln!(stdout, "#{} {} -> {ARCHIVED_STATUS}", r.id, r.old_status).unwrap_or(());
            }
        }
        Format::Table => {
            for r in &results {
                writeln!(stdout, "Archived task #{}: {}", r.id, r.title).unwrap_or(());
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
                "{} of {} archive(s) failed",
                errors.len(),
                args.ids.len()
            ),
        ));
    }

    Ok(())
}

fn archive_one(
    cfg: &crate::model::config::Config,
    id_str: &str,
) -> Result<ArchiveResult, String> {
    let id: i32 = id_str
        .trim_start_matches('#')
        .parse()
        .map_err(|_| format!("invalid task ID: {id_str}"))?;

    let file_path =
        task::find_by_id(&cfg.tasks_path(), id).map_err(|e| format!("{e}"))?;
    let mut t =
        task::read(&file_path).map_err(|e| format!("{e}"))?;

    let old_status = t.status.clone();
    t.status = ARCHIVED_STATUS.to_string();
    t.updated = Utc::now();

    task::write(&file_path, &t)
        .map_err(|e| format!("failed to write: {e}"))?;

    Ok(ArchiveResult {
        id,
        title: t.title.clone(),
        old_status,
        task: t,
    })
}
