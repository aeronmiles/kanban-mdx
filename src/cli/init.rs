//! `kanban-md init` — initialize a new kanban board.

use std::io::Write;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::config::{StatusConfig, ARCHIVED_STATUS};
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct InitArgs {
    /// Board name
    #[arg(default_value = "My Board")]
    pub name: String,
    /// Directory to initialize in (defaults to ./kanban)
    #[arg(long)]
    pub path: Option<String>,
    /// Comma-separated custom status list (e.g. "backlog,todo,in-progress,done")
    #[arg(long, value_delimiter = ',')]
    pub statuses: Option<Vec<String>>,
    /// Per-status WIP limit as STATUS:N (repeatable, e.g. --wip-limit in-progress:3)
    #[arg(long = "wip-limit")]
    pub wip_limits: Option<Vec<String>>,
}

pub fn run(cli: &Cli, args: InitArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);

    let dir = if let Some(ref p) = args.path {
        std::path::PathBuf::from(p)
    } else if let Some(ref d) = cli.dir {
        std::path::PathBuf::from(d)
    } else {
        std::path::PathBuf::from(crate::model::config::DEFAULT_DIR)
    };

    let mut cfg = crate::io::config_file::init(&dir, &args.name)?;

    let mut modified = false;

    // Apply custom statuses if provided.
    if let Some(ref status_list) = args.statuses {
        let mut statuses: Vec<StatusConfig> = status_list
            .iter()
            .map(|s| StatusConfig {
                name: s.trim().to_string(),
                ..Default::default()
            })
            .collect();

        // Ensure "archived" is always present.
        if !statuses.iter().any(|s| s.name == ARCHIVED_STATUS) {
            statuses.push(StatusConfig {
                name: ARCHIVED_STATUS.to_string(),
                ..Default::default()
            });
        }

        // Update default status if the current default isn't in the new list.
        let status_names: Vec<&str> = statuses.iter().map(|s| s.name.as_str()).collect();
        if !status_names.contains(&cfg.defaults.status.as_str()) {
            cfg.defaults.status = statuses
                .first()
                .map(|s| s.name.clone())
                .unwrap_or_default();
        }

        cfg.statuses = statuses;
        modified = true;
    }

    // Apply WIP limits if provided.
    if let Some(ref wip_list) = args.wip_limits {
        let status_names: Vec<String> = cfg.statuses.iter().map(|s| s.name.clone()).collect();

        for wip in wip_list {
            let parts: Vec<&str> = wip.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Err(CliError::newf(
                    ErrorCode::InvalidInput,
                    format!(
                        "invalid --wip-limit format {:?}: expected STATUS:N",
                        wip
                    ),
                ));
            }
            let status = parts[0];
            let limit: i32 = parts[1].parse().map_err(|_| {
                CliError::newf(
                    ErrorCode::InvalidInput,
                    format!(
                        "invalid --wip-limit value {:?}: N must be an integer",
                        wip
                    ),
                )
            })?;

            if !status_names.contains(&status.to_string()) {
                return Err(CliError::newf(
                    ErrorCode::InvalidStatus,
                    format!(
                        "--wip-limit references unknown status {:?}; known statuses: {:?}",
                        status, status_names
                    ),
                ));
            }

            cfg.wip_limits.insert(status.to_string(), limit);
        }
        modified = true;
    }

    // Re-save if we modified the config after init.
    if modified {
        crate::io::config_file::save(&cfg)?;
    }

    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            crate::output::json::json(&mut stdout, &cfg)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        Format::Compact => {
            writeln!(stdout, "init {}", dir.display()).unwrap_or(());
        }
        Format::Table => {
            writeln!(
                stdout,
                "Initialized board {:?} in {}",
                args.name,
                dir.display()
            )
            .unwrap_or(());
        }
    }
    Ok(())
}
