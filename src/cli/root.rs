//! Root CLI definition: `Cli` struct, `Commands` enum, `execute()`, and helpers.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::error::{CliError, ErrorCode};
use crate::model::config::Config;
use crate::output::Format;

/// A file-based Kanban tool powered by Markdown.
#[derive(Parser)]
#[command(name = "kbmdx", version, about, long_about = None)]
pub struct Cli {
    /// Path to the kanban directory (overrides auto-detection).
    #[arg(long, short = 'd', global = true, env = "KANBAN_DIR")]
    pub dir: Option<String>,

    /// Output as JSON.
    #[arg(long, global = true)]
    pub json: bool,

    /// Output as compact one-line format.
    #[arg(long, global = true)]
    pub compact: bool,

    /// Output as table (default).
    #[arg(long, global = true)]
    pub table: bool,

    /// Disable colored output.
    #[arg(long, global = true, env = "NO_COLOR")]
    pub no_color: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new kanban board.
    Init(super::init::InitArgs),
    /// Create a new task.
    Create(super::create::CreateArgs),
    /// Show task details.
    Show(super::show::ShowArgs),
    /// Edit an existing task.
    Edit(super::edit::EditArgs),
    /// Delete a task.
    Delete(super::delete::DeleteArgs),
    /// Move a task to a different status.
    #[command(name = "move")]
    Move(super::move_cmd::MoveArgs),
    /// List tasks with filters.
    #[command(visible_alias = "ls")]
    List(super::list::ListArgs),
    /// Search for tasks.
    Find(super::find::FindArgs),
    /// Pick the highest-priority unclaimed task.
    Pick(super::pick::PickArgs),
    /// Archive a completed task.
    Archive(super::archive::ArchiveArgs),
    /// Hand off a task to another agent.
    Handoff(super::handoff::HandoffArgs),
    /// Show task dependencies.
    Deps(super::deps::DepsArgs),
    /// Show board metrics.
    Metrics(super::metrics::MetricsArgs),
    /// Show board summary.
    Board(super::board::BoardArgs),
    /// Show mutation log.
    Log(super::log::LogArgs),
    /// Undo last mutation.
    Undo(super::undo::UndoArgs),
    /// Redo last undone mutation.
    Redo(super::undo::RedoArgs),
    /// Get or set config values.
    Config(super::config::ConfigArgs),
    /// Migrate config.yml (YAML) to config.toml (TOML).
    #[command(name = "migrate-config")]
    MigrateConfig(super::migrate_config::MigrateConfigArgs),
    /// Launch the terminal UI.
    Tui(super::tui::TuiArgs),
    /// Generate board context for agents.
    Context(super::context::ContextArgs),
    /// Generate a random agent name.
    AgentName(super::agent_name::AgentNameArgs),
    /// Generate shell completions.
    Completion(super::completion::CompletionArgs),
    /// Print the file path for a task.
    Filepath(super::filepath::FilepathArgs),
    /// List git worktrees.
    Worktrees(super::worktrees::WorktreesArgs),
    /// Bulk-create tasks from a JSON or YAML spec.
    Import(super::import::ImportArgs),
    /// Validate branch setup for the current worktree/branch.
    #[command(name = "branch-check")]
    BranchCheck(super::branch_check::BranchCheckArgs),
    /// Manage .gitignore entries for the kanban directory.
    Gitignore(super::gitignore::GitignoreArgs),
    /// Manage agent skills.
    Skill(super::skill::SkillArgs),
    /// Manage semantic search embeddings.
    Embed(super::embed::EmbedArgs),
}

/// Execute the parsed CLI command.
pub fn execute(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    // Disable colored output globally if --no-color or NO_COLOR env is set.
    if cli.no_color {
        colored::control::set_override(false);
    }

    let result = match &cli.command {
        Commands::Init(args) => super::init::run(&cli, args.clone()),
        Commands::Create(args) => super::create::run(&cli, args.clone()),
        Commands::Show(args) => super::show::run(&cli, args.clone()),
        Commands::Edit(args) => super::edit::run(&cli, args.clone()),
        Commands::Delete(args) => super::delete::run(&cli, args.clone()),
        Commands::Move(args) => super::move_cmd::run(&cli, args.clone()),
        Commands::List(args) => super::list::run(&cli, args.clone()),
        Commands::Find(args) => super::find::run(&cli, args.clone()),
        Commands::Pick(args) => super::pick::run(&cli, args.clone()),
        Commands::Archive(args) => super::archive::run(&cli, args.clone()),
        Commands::Handoff(args) => super::handoff::run(&cli, args.clone()),
        Commands::Deps(args) => super::deps::run(&cli, args.clone()),
        Commands::Metrics(args) => super::metrics::run(&cli, args.clone()),
        Commands::Board(args) => super::board::run(&cli, args.clone()),
        Commands::Log(args) => super::log::run(&cli, args.clone()),
        Commands::Undo(args) => super::undo::run_undo(&cli, args.clone()),
        Commands::Redo(args) => super::undo::run_redo(&cli, args.clone()),
        Commands::Config(args) => super::config::run(&cli, args.clone()),
        Commands::MigrateConfig(args) => super::migrate_config::run(&cli, args.clone()),
        Commands::Tui(args) => super::tui::run(&cli, args.clone()),
        Commands::Context(args) => super::context::run(&cli, args.clone()),
        Commands::AgentName(args) => super::agent_name::run(&cli, args.clone()),
        Commands::Completion(args) => super::completion::run(&cli, args.clone()),
        Commands::Filepath(args) => super::filepath::run(&cli, args.clone()),
        Commands::Worktrees(args) => super::worktrees::run(&cli, args.clone()),
        Commands::Import(args) => super::import::run(&cli, args.clone()),
        Commands::BranchCheck(args) => super::branch_check::run(&cli, args.clone()),
        Commands::Gitignore(args) => super::gitignore::run(&cli, args.clone()),
        Commands::Skill(args) => super::skill::run(&cli, args.clone()),
        Commands::Embed(args) => super::embed::run(&cli, args.clone()),
    };

    result.map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
}

/// Resolve the kanban directory from CLI args or auto-detection.
pub fn resolve_dir(cli: &Cli) -> Result<PathBuf, CliError> {
    if let Some(ref dir) = cli.dir {
        let path = PathBuf::from(dir);
        if path.exists() {
            return Ok(path);
        }
        return Err(CliError::newf(
            ErrorCode::BoardNotFound,
            format!("directory not found: {dir}"),
        ));
    }

    let cwd = std::env::current_dir().map_err(|e| {
        CliError::newf(ErrorCode::InternalError, format!("getting cwd: {e}"))
    })?;

    crate::io::config_file::find_dir(&cwd)
}

/// Load config from the resolved kanban directory.
pub fn load_config(cli: &Cli) -> Result<Config, CliError> {
    let dir = resolve_dir(cli)?;
    crate::io::config_file::load(&dir)
}

/// Determine the output format from CLI flags.
pub fn output_format(cli: &Cli) -> Format {
    crate::output::detect(cli.json, cli.table, cli.compact)
}

/// Returns true if color output should be disabled.
pub fn no_color(cli: &Cli) -> bool {
    cli.no_color
}
