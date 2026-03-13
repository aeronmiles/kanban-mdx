//! `kanban-md completion` — generate shell completions.

use std::io;

use clap::CommandFactory;
use clap_complete::{Shell, generate};

use crate::cli::root::Cli;
use crate::error::CliError;

#[derive(clap::Args, Clone)]
pub struct CompletionArgs {
    /// Shell to generate completions for
    #[arg(value_enum)]
    pub shell: Shell,
}

pub fn run(_cli: &Cli, args: CompletionArgs) -> Result<(), CliError> {
    let mut cmd = super::root::Cli::command();
    generate(args.shell, &mut cmd, "kanban-md", &mut io::stdout());
    Ok(())
}
