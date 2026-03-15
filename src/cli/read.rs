//! `kbmdx read <path>` — standalone markdown reader (no board required).

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};

#[derive(clap::Args, Clone)]
pub struct ReadArgs {
    /// Path to a markdown file.
    pub path: String,
}

pub fn run(_cli: &Cli, args: ReadArgs) -> Result<(), CliError> {
    let raw = std::path::PathBuf::from(&args.path);
    let path = if raw.is_absolute() {
        raw
    } else {
        std::env::current_dir()
            .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("getting cwd: {e}")))?
            .join(raw)
    };

    if !path.exists() {
        return Err(CliError::newf(
            ErrorCode::TaskNotFound,
            format!("file not found: {}", path.display()),
        ));
    }

    let body = std::fs::read_to_string(&path)
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("reading file: {e}")))?;

    let title = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    crate::tui::run_tui_reader(path, title, body)
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("TUI error: {e}")))?;

    Ok(())
}
