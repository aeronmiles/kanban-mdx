//! `kbmdx filepath` — print the file path for a task.

use std::io::Write;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct FilepathArgs {
    /// Task ID
    pub id: String,
}

pub fn run(cli: &Cli, args: FilepathArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;

    let id = super::helpers::parse_task_id(&args.id)?;

    let (file_path, _) = super::helpers::load_task(&cfg, id)?;

    let path_str = file_path.to_string_lossy().to_string();

    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            let result = serde_json::json!({"id": id, "path": path_str});
            crate::output::json::json(&mut stdout, &result)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        _ => {
            writeln!(stdout, "{path_str}")
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
    }
    Ok(())
}
