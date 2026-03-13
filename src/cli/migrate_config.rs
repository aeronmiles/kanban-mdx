//! `kanban-md migrate-config` — convert config.yml (YAML) to config.toml (TOML).

use std::io::Write;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::config::{CONFIG_FILE_NAME, LEGACY_CONFIG_FILE_NAME};

#[derive(clap::Args, Clone)]
pub struct MigrateConfigArgs;

pub fn run(cli: &Cli, _args: MigrateConfigArgs) -> Result<(), CliError> {
    let dir = crate::cli::root::resolve_dir(cli)?;
    let abs_dir = dir.canonicalize().map_err(|e| {
        CliError::newf(ErrorCode::InternalError, format!("resolving path: {}", e))
    })?;

    let yaml_path = abs_dir.join(LEGACY_CONFIG_FILE_NAME);
    let toml_path = abs_dir.join(CONFIG_FILE_NAME);

    if toml_path.exists() {
        return Err(CliError::new(
            ErrorCode::InvalidInput,
            "config.toml already exists — migration not needed",
        ));
    }

    if !yaml_path.exists() {
        return Err(CliError::new(
            ErrorCode::BoardNotFound,
            "no config.yml found to migrate",
        ));
    }

    // Load triggers auto-migration: YAML → TOML conversion + removal of config.yml.
    let cfg = crate::io::config_file::load(&abs_dir)?;

    let mut stdout = std::io::stdout();
    writeln!(
        stdout,
        "Migrated {} to {} (version {})",
        LEGACY_CONFIG_FILE_NAME,
        CONFIG_FILE_NAME,
        cfg.version
    )
    .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;

    Ok(())
}
