//! `kanban-md skill` — manage agent skills.
//!
//! Subcommands: check, install, show, update.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{CliError, ErrorCode};
use crate::skill;

/// Top-level args for `kanban-md skill`.
#[derive(clap::Args, Clone)]
pub struct SkillArgs {
    #[command(subcommand)]
    pub command: SkillCommands,
}

/// Skill subcommands.
#[derive(clap::Subcommand, Clone)]
pub enum SkillCommands {
    /// Check if installed skills are up to date.
    Check(SkillCheckArgs),
    /// Install agent skills.
    Install(SkillInstallArgs),
    /// Print embedded skill content to stdout.
    Show(SkillShowArgs),
    /// Update installed skills to current version.
    Update(SkillUpdateArgs),
}

#[derive(clap::Args, Clone)]
pub struct SkillCheckArgs {
    /// Agent(s) to check (claude, codex, cursor, openclaw).
    #[arg(long, value_delimiter = ',')]
    pub agent: Option<Vec<String>>,
    /// Check user-level (global) skills.
    #[arg(long)]
    pub global: bool,
}

#[derive(clap::Args, Clone)]
pub struct SkillInstallArgs {
    /// Agent(s) to install for (claude, codex, cursor, openclaw).
    #[arg(long, value_delimiter = ',')]
    pub agent: Option<Vec<String>>,
    /// Skill(s) to install (kanban-md, kanban-based-development).
    #[arg(long, value_delimiter = ',')]
    pub skill: Option<Vec<String>>,
    /// Install to user-level (global) skill directory.
    #[arg(long)]
    pub global: bool,
    /// Overwrite existing skills without checking version.
    #[arg(long)]
    pub force: bool,
    /// Install skills to a specific directory (skips agent selection).
    #[arg(long)]
    pub path: Option<String>,
}

#[derive(clap::Args, Clone)]
pub struct SkillShowArgs {
    /// Skill to show (kanban-md or kanban-based-development).
    #[arg(long)]
    pub skill: Option<String>,
}

#[derive(clap::Args, Clone)]
pub struct SkillUpdateArgs {
    /// Agent(s) to update.
    #[arg(long, value_delimiter = ',')]
    pub agent: Option<Vec<String>>,
    /// Update user-level (global) skills.
    #[arg(long)]
    pub global: bool,
}

/// Execute the skill command (dispatches to subcommands).
pub fn run(_cli: &crate::cli::root::Cli, args: SkillArgs) -> Result<(), CliError> {
    match args.command {
        SkillCommands::Check(a) => run_check(a),
        SkillCommands::Install(a) => run_install(a),
        SkillCommands::Show(a) => run_show(a),
        SkillCommands::Update(a) => run_update(a),
    }
}

// ---------------------------------------------------------------------------
// skill show
// ---------------------------------------------------------------------------

fn run_show(args: SkillShowArgs) -> Result<(), CliError> {
    let mut stdout = std::io::stdout();

    for s in skill::AVAILABLE_SKILLS {
        if let Some(ref filter) = args.skill {
            if s.name != filter.as_str() {
                continue;
            }
        }

        let content = skill::read_embedded_skill(s.name).ok_or_else(|| {
            CliError::newf(
                ErrorCode::InternalError,
                format!("embedded skill {} not found", s.name),
            )
        })?;

        if args.skill.is_none() {
            write!(stdout, "=== {} ===\n\n", s.name).unwrap_or(());
        }
        write!(stdout, "{content}").unwrap_or(());
        if args.skill.is_none() {
            writeln!(stdout).unwrap_or(());
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// skill install
// ---------------------------------------------------------------------------

fn run_install(args: SkillInstallArgs) -> Result<(), CliError> {
    let version = skill::cli_version();

    // Determine which skills to install.
    let selected_skills = resolve_skills(args.skill.as_deref())?;
    if selected_skills.is_empty() {
        println!("No skills selected.");
        return Ok(());
    }

    // --path mode: install directly to the given directory, skip agent selection.
    if let Some(ref path) = args.path {
        let abs_path = std::fs::canonicalize(path)
            .or_else(|_| {
                // If path doesn't exist yet, resolve relative to cwd.
                let p = PathBuf::from(path);
                if p.is_absolute() {
                    Ok(p)
                } else {
                    std::env::current_dir().map(|cwd| cwd.join(path))
                }
            })
            .map_err(|e| {
                CliError::newf(ErrorCode::InternalError, format!("resolving path: {e}"))
            })?;

        return install_to_path(&abs_path, &selected_skills, args.force, version);
    }

    let project_root = skill::find_project_root().map_err(|e| {
        CliError::newf(
            ErrorCode::InternalError,
            format!("finding project root: {e}"),
        )
    })?;

    // Determine which agents to install for.
    let selected_agents = resolve_agents(args.agent.as_deref(), &project_root, args.global);
    if selected_agents.is_empty() {
        println!("No agents selected.");
        return Ok(());
    }

    // Install.
    let mut installed = 0;
    for agent in &selected_agents {
        let base_dir = match agent.skill_path(&project_root, args.global) {
            Some(p) => p,
            None => continue,
        };

        for s in &selected_skills {
            let dest_path = base_dir.join(s.name).join("SKILL.md");
            let display_path = relative_path(&project_root, &dest_path);

            if !args.force {
                if let Some(ref v) = skill::installed_version(&dest_path) {
                    if v == version {
                        println!("  {} -- already at {} (skipped)", display_path, version);
                        continue;
                    }
                }
            }

            skill::install::install(s.name, &base_dir, version).map_err(|e| {
                CliError::newf(
                    ErrorCode::InternalError,
                    format!("installing {} for {}: {e}", s.name, agent.display_name),
                )
            })?;

            println!("  {} ({})", display_path, version);
            installed += 1;
        }
    }

    if installed > 0 {
        println!("Installed {} skill(s).", installed);
    } else {
        println!("All skills are already up to date.");
    }
    Ok(())
}

fn install_to_path(
    dir: &Path,
    skills: &[&skill::SkillInfo],
    force: bool,
    version: &str,
) -> Result<(), CliError> {
    let mut installed = 0;

    for s in skills {
        let dest_path = dir.join(s.name).join("SKILL.md");

        if !force {
            if let Some(ref v) = skill::installed_version(&dest_path) {
                if v == version {
                    println!(
                        "  {} -- already at {} (skipped)",
                        dest_path.display(),
                        version
                    );
                    continue;
                }
            }
        }

        skill::install::install(s.name, dir, version).map_err(|e| {
            CliError::newf(
                ErrorCode::InternalError,
                format!("installing {} to {}: {e}", s.name, dir.display()),
            )
        })?;

        println!("  {} ({})", dest_path.display(), version);
        installed += 1;
    }

    if installed > 0 {
        println!("Installed {} skill(s).", installed);
    } else {
        println!("All skills are already up to date.");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// skill update
// ---------------------------------------------------------------------------

fn run_update(args: SkillUpdateArgs) -> Result<(), CliError> {
    let version = skill::cli_version();

    let project_root = skill::find_project_root().map_err(|e| {
        CliError::newf(
            ErrorCode::InternalError,
            format!("finding project root: {e}"),
        )
    })?;

    let agents = resolve_agent_list(args.agent.as_deref());
    let mut updated = 0;

    for agent in &agents {
        let base_dir = match agent.skill_path(&project_root, args.global) {
            Some(p) => p,
            None => continue,
        };

        let installed = skill::find_installed_skills(&base_dir);
        for (skill_name, skill_path) in &installed {
            let display_path = relative_path(&project_root, skill_path);

            if !skill::is_outdated(skill_path, version) {
                println!(
                    "  {} -- already at {} (skipped)",
                    display_path, version
                );
                continue;
            }

            let old_ver = skill::installed_version(skill_path)
                .unwrap_or_else(|| "unknown".to_string());

            skill::install::install(skill_name, &base_dir, version).map_err(|e| {
                CliError::newf(
                    ErrorCode::InternalError,
                    format!("updating {} for {}: {e}", skill_name, agent.display_name),
                )
            })?;

            println!("  {} ({} -> {})", display_path, old_ver, version);
            updated += 1;
        }
    }

    if updated > 0 {
        println!("Updated {} skill(s).", updated);
    } else {
        println!("All skills are already up to date.");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// skill check
// ---------------------------------------------------------------------------

fn run_check(args: SkillCheckArgs) -> Result<(), CliError> {
    let version = skill::cli_version();

    let project_root = skill::find_project_root().map_err(|e| {
        CliError::newf(
            ErrorCode::InternalError,
            format!("finding project root: {e}"),
        )
    })?;

    let agents = resolve_agent_list(args.agent.as_deref());
    let mut any_outdated = false;
    let mut any_found = false;

    for agent in &agents {
        let base_dir = match agent.skill_path(&project_root, args.global) {
            Some(p) => p,
            None => continue,
        };

        let installed = skill::find_installed_skills(&base_dir);
        for (skill_name, skill_path) in &installed {
            any_found = true;
            let installed_ver = skill::installed_version(skill_path)
                .unwrap_or_else(|| "unknown".to_string());

            if skill::is_outdated(skill_path, version) {
                any_outdated = true;
                println!(
                    "  x {}/{} ({} -> {})",
                    agent.display_name, skill_name, installed_ver, version
                );
            } else {
                println!(
                    "  ok {}/{} ({})",
                    agent.display_name, skill_name, installed_ver
                );
            }
        }
    }

    if !any_found {
        println!("No kanban-md skills installed. Run: kanban-md skill install");
        return Ok(());
    }

    if any_outdated {
        println!("Run: kanban-md skill update");
        return Err(CliError::new(
            ErrorCode::InternalError,
            "outdated skills found",
        ));
    }

    println!("All skills are up to date.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Resolve which skills to install based on filter flags.
/// In non-interactive mode (no filter), returns all skills.
fn resolve_skills(filter: Option<&[String]>) -> Result<Vec<&'static skill::SkillInfo>, CliError> {
    let all = skill::AVAILABLE_SKILLS;

    if let Some(names) = filter {
        let mut result = Vec::new();
        for name in names {
            let found = all.iter().find(|s| s.name == name.as_str());
            match found {
                Some(s) => result.push(s),
                None => {
                    return Err(CliError::newf(
                        ErrorCode::InvalidInput,
                        format!(
                            "unknown skill: {:?} (available: {})",
                            name,
                            skill::skill_names().join(", ")
                        ),
                    ));
                }
            }
        }
        return Ok(result);
    }

    // Non-interactive: use all skills.
    Ok(all.iter().collect())
}

/// Resolve which agents to install for. If no filter, detect or use all.
fn resolve_agents(
    filter: Option<&[String]>,
    project_root: &Path,
    global: bool,
) -> Vec<&'static skill::Agent> {
    if let Some(names) = filter {
        return resolve_agent_list(Some(names));
    }

    // Non-interactive: use all detected agents.
    if global {
        skill::agents().iter().collect()
    } else {
        skill::detect_agents(project_root)
    }
}

/// Convert agent name strings to Agent references. Unknown names are ignored.
/// If no filter, returns all agents.
fn resolve_agent_list(names: Option<&[String]>) -> Vec<&'static skill::Agent> {
    match names {
        Some(names) => names
            .iter()
            .filter_map(|n| skill::agent_by_name(n))
            .collect(),
        None => skill::agents().iter().collect(),
    }
}

/// Returns a relative path from root, or the absolute path as a string.
fn relative_path(root: &Path, abs: &Path) -> String {
    abs.strip_prefix(root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| abs.display().to_string())
}
