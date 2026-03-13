use clap::Parser;

fn main() {
    let cli = kanban_mdx::cli::root::Cli::parse();
    if let Err(e) = kanban_mdx::cli::root::execute(cli) {
        // Check if it's a SilentError (batch results already written)
        if let Some(silent) = e.downcast_ref::<kanban_mdx::error::SilentError>() {
            std::process::exit(silent.code);
        }

        // Check if it's a CliError for structured error output
        if let Some(cli_err) = e.downcast_ref::<kanban_mdx::error::CliError>() {
            eprintln!("error: {}", cli_err.message);
            std::process::exit(cli_err.exit_code());
        }

        // Generic error fallback
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
