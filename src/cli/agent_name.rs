//! `kbmdx agent-name` — generate a random agent name.

use std::io::Write;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct AgentNameArgs {}

pub fn run(cli: &Cli, _args: AgentNameArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let name = crate::util::agentname::generate();

    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            let result = serde_json::json!({"name": name});
            crate::output::json::json(&mut stdout, &result)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        _ => {
            writeln!(stdout, "{name}")
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
    }
    Ok(())
}
