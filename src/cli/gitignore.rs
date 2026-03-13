//! `kanban-md gitignore` — manage .gitignore entries for the kanban directory.
//!
//! Ensures the kanban directory is listed in the parent `.gitignore` file.
//! Can be used interactively (prompts for confirmation) or non-interactively
//! with `--yes`.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct GitignoreArgs {
    /// Skip confirmation prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
    /// Remove the kanban entry from .gitignore instead of adding
    #[arg(long)]
    pub remove: bool,
}

pub fn run(cli: &Cli, args: GitignoreArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;
    let kanban_dir = cfg.dir().to_path_buf();

    let (gitignore_path, entry) = gitignore_prompt_data(&kanban_dir)?;

    if args.remove {
        remove_gitignore_entry(&gitignore_path, &entry)?;
        let mut stdout = std::io::stdout();
        match format {
            Format::Json => {
                let result = serde_json::json!({
                    "action": "remove",
                    "gitignore": gitignore_path.to_string_lossy(),
                    "entry": entry,
                });
                crate::output::json::json(&mut stdout, &result)
                    .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
            }
            _ => {
                writeln!(stdout, "Removed {:?} from {}", entry, gitignore_path.display())
                    .unwrap_or(());
            }
        }
        return Ok(());
    }

    if !args.yes {
        eprint!("Add {:?} to .gitignore? [Y/n] ", entry);
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer).map_err(|e| {
            CliError::newf(ErrorCode::InternalError, format!("reading input: {e}"))
        })?;
        let answer = answer.trim().to_lowercase();
        if !answer.is_empty() && answer != "y" && answer != "yes" {
            return Ok(());
        }
    }

    ensure_gitignore_entry(&gitignore_path, &entry)?;

    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            let result = serde_json::json!({
                "action": "add",
                "gitignore": gitignore_path.to_string_lossy(),
                "entry": entry,
            });
            crate::output::json::json(&mut stdout, &result)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        _ => {
            writeln!(stdout, "Added {:?} to {}", entry, gitignore_path.display())
                .unwrap_or(());
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Gitignore helpers (ported from Go gitignore.go)
// ---------------------------------------------------------------------------

/// Computes the .gitignore path and the entry string for the given kanban directory.
fn gitignore_prompt_data(kanban_dir: &Path) -> Result<(PathBuf, String), CliError> {
    let abs_dir = kanban_dir
        .canonicalize()
        .or_else(|_| std::fs::canonicalize(kanban_dir))
        .unwrap_or_else(|_| kanban_dir.to_path_buf());

    let parent_dir = abs_dir.parent().ok_or_else(|| {
        CliError::newf(
            ErrorCode::InternalError,
            format!("invalid kanban directory {:?}", kanban_dir),
        )
    })?;

    let entry_base = abs_dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| {
            CliError::newf(
                ErrorCode::InternalError,
                format!("invalid kanban directory {:?}", kanban_dir),
            )
        })?;

    if entry_base.is_empty() || entry_base == "." {
        return Err(CliError::newf(
            ErrorCode::InternalError,
            format!("invalid kanban directory {:?}", kanban_dir),
        ));
    }

    let entry = sanitize_gitignore_entry(&format!("{entry_base}/"));
    let gitignore_path = parent_dir.join(".gitignore");
    Ok((gitignore_path, entry))
}

/// Ensures the given entry exists in the .gitignore file.
/// Creates the file if it doesn't exist.
pub fn ensure_gitignore_entry(gitignore_path: &Path, entry: &str) -> Result<(), CliError> {
    let entry = sanitize_gitignore_entry(entry);

    match std::fs::read(gitignore_path) {
        Ok(contents) => {
            if has_gitignore_entry(&contents, &entry) {
                return Ok(());
            }
            // Append entry.
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(gitignore_path)
                .map_err(|e| {
                    CliError::newf(
                        ErrorCode::InternalError,
                        format!("opening .gitignore: {e}"),
                    )
                })?;

            // Add newline before entry if file doesn't end with one.
            if !contents.is_empty() && contents.last() != Some(&b'\n') {
                file.write_all(b"\n").map_err(|e| {
                    CliError::newf(
                        ErrorCode::InternalError,
                        format!("updating .gitignore: {e}"),
                    )
                })?;
            }
            writeln!(file, "{entry}").map_err(|e| {
                CliError::newf(
                    ErrorCode::InternalError,
                    format!("updating .gitignore: {e}"),
                )
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Create new .gitignore with the entry.
            std::fs::write(gitignore_path, format!("{entry}\n")).map_err(|e| {
                CliError::newf(
                    ErrorCode::InternalError,
                    format!("creating .gitignore: {e}"),
                )
            })
        }
        Err(e) => Err(CliError::newf(
            ErrorCode::InternalError,
            format!("reading .gitignore: {e}"),
        )),
    }
}

/// Removes the given entry from the .gitignore file.
fn remove_gitignore_entry(gitignore_path: &Path, entry: &str) -> Result<(), CliError> {
    let entry = sanitize_gitignore_entry(entry);

    let contents = std::fs::read_to_string(gitignore_path).map_err(|e| {
        CliError::newf(
            ErrorCode::InternalError,
            format!("reading .gitignore: {e}"),
        )
    })?;

    let filtered: Vec<&str> = contents
        .lines()
        .filter(|line| line.trim() != entry)
        .collect();

    let mut output = filtered.join("\n");
    if !output.is_empty() {
        output.push('\n');
    }

    std::fs::write(gitignore_path, output).map_err(|e| {
        CliError::newf(
            ErrorCode::InternalError,
            format!("writing .gitignore: {e}"),
        )
    })
}

/// Returns true if the .gitignore contents already contain the entry.
fn has_gitignore_entry(contents: &[u8], entry: &str) -> bool {
    let text = String::from_utf8_lossy(contents);
    text.lines().any(|line| line.trim() == entry)
}

/// Sanitizes a gitignore entry: trims whitespace, normalizes path separators,
/// and ensures a trailing slash for directory entries.
fn sanitize_gitignore_entry(entry: &str) -> String {
    let clean = entry.trim().replace('\\', "/");
    let clean = clean.trim_end_matches('/');
    format!("{clean}/")
}

/// Offers to add the kanban directory to .gitignore (interactive helper).
/// Used by the `init` command.
#[allow(dead_code)]
pub fn offer_add_kanban_to_gitignore(kanban_dir: &Path) -> Result<(), CliError> {
    let (gitignore_path, entry) = gitignore_prompt_data(kanban_dir)?;

    eprint!("Add {:?} to .gitignore? [Y/n] ", entry);
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer).map_err(|e| {
        CliError::newf(ErrorCode::InternalError, format!("reading input: {e}"))
    })?;
    let answer = answer.trim().to_lowercase();

    if !answer.is_empty() && answer != "y" && answer != "yes" {
        return Ok(());
    }

    ensure_gitignore_entry(&gitignore_path, &entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_gitignore_entry() {
        assert_eq!(sanitize_gitignore_entry("kanban/"), "kanban/");
        assert_eq!(sanitize_gitignore_entry("kanban"), "kanban/");
        assert_eq!(sanitize_gitignore_entry("  kanban/  "), "kanban/");
        assert_eq!(sanitize_gitignore_entry("kanban//"), "kanban/");
    }

    #[test]
    fn test_has_gitignore_entry() {
        let contents = b"node_modules/\nkanban/\n.env\n";
        assert!(has_gitignore_entry(contents, "kanban/"));
        assert!(!has_gitignore_entry(contents, "build/"));
    }

    #[test]
    fn test_has_gitignore_entry_empty() {
        assert!(!has_gitignore_entry(b"", "kanban/"));
    }

    #[test]
    fn test_ensure_gitignore_entry_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let gitignore = dir.path().join(".gitignore");
        ensure_gitignore_entry(&gitignore, "kanban/").unwrap();
        let contents = std::fs::read_to_string(&gitignore).unwrap();
        assert!(contents.contains("kanban/"));
    }

    #[test]
    fn test_ensure_gitignore_entry_appends() {
        let dir = tempfile::tempdir().unwrap();
        let gitignore = dir.path().join(".gitignore");
        std::fs::write(&gitignore, "node_modules/\n").unwrap();
        ensure_gitignore_entry(&gitignore, "kanban/").unwrap();
        let contents = std::fs::read_to_string(&gitignore).unwrap();
        assert!(contents.contains("node_modules/"));
        assert!(contents.contains("kanban/"));
    }

    #[test]
    fn test_ensure_gitignore_entry_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let gitignore = dir.path().join(".gitignore");
        std::fs::write(&gitignore, "kanban/\n").unwrap();
        ensure_gitignore_entry(&gitignore, "kanban/").unwrap();
        let contents = std::fs::read_to_string(&gitignore).unwrap();
        // Should still have exactly one entry.
        assert_eq!(contents.matches("kanban/").count(), 1);
    }

    #[test]
    fn test_remove_gitignore_entry() {
        let dir = tempfile::tempdir().unwrap();
        let gitignore = dir.path().join(".gitignore");
        std::fs::write(&gitignore, "node_modules/\nkanban/\n.env\n").unwrap();
        remove_gitignore_entry(&gitignore, "kanban/").unwrap();
        let contents = std::fs::read_to_string(&gitignore).unwrap();
        assert!(!contents.contains("kanban/"));
        assert!(contents.contains("node_modules/"));
        assert!(contents.contains(".env"));
    }
}
