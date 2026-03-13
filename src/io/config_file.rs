//! Config file I/O: loading, saving, finding, initializing, and migrating config.toml.
//!
//! Supports auto-migration from legacy config.yml (YAML) to config.toml (TOML).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use crate::error::{CliError, ErrorCode};
use crate::model::config::{
    Config, StatusConfig,
    ARCHIVED_STATUS, CONFIG_FILE_NAME, CURRENT_VERSION, DEFAULT_CLASS, DEFAULT_CLAIM_TIMEOUT,
    DEFAULT_DIR, DEFAULT_READER_MAX_WIDTH, DEFAULT_READER_WIDTH_PCT, DEFAULT_TITLE_LINES,
    LEGACY_CONFIG_FILE_NAME, REFERENCES_STATUS,
    default_age_thresholds, default_classes, default_collapsed_columns,
};

const FILE_MODE: u32 = 0o600;
const DIR_MODE: u32 = 0o750;

// ── Public API ─────────────────────────────────────────────────────────

/// Load config from a kanban directory path.
///
/// 1. Read config.toml (or legacy config.yml with auto-migration)
/// 2. Deserialize TOML (or YAML for legacy files)
/// 3. Run migrations if version < CURRENT_VERSION
/// 4. Save migrated config as TOML
/// 5. Validate
/// 6. Set dir on config
pub fn load(dir: &Path) -> Result<Config, CliError> {
    let abs_dir = dir.canonicalize().map_err(|e| {
        CliError::newf(ErrorCode::InternalError, format!("resolving path: {}", e))
    })?;

    let toml_path = abs_dir.join(CONFIG_FILE_NAME);
    let yaml_path = abs_dir.join(LEGACY_CONFIG_FILE_NAME);

    let (data, from_yaml) = if toml_path.exists() {
        let d = fs::read_to_string(&toml_path).map_err(|e| {
            CliError::newf(ErrorCode::InternalError, format!("reading config: {}", e))
        })?;
        (d, false)
    } else if yaml_path.exists() {
        let d = fs::read_to_string(&yaml_path).map_err(|e| {
            CliError::newf(ErrorCode::InternalError, format!("reading config: {}", e))
        })?;
        (d, true)
    } else {
        return Err(CliError::new(
            ErrorCode::BoardNotFound,
            "no kanban board found (run 'kanban-md init' to create one)",
        ));
    };

    let mut cfg: Config = if from_yaml {
        serde_yml::from_str(&data).map_err(|e| {
            CliError::newf(ErrorCode::InvalidInput, format!("parsing config (yaml): {}", e))
        })?
    } else {
        toml::from_str(&data).map_err(|e| {
            CliError::newf(ErrorCode::InvalidInput, format!("parsing config: {}", e))
        })?
    };

    cfg.set_dir(abs_dir);

    // Migrate old config versions forward before validating.
    let old_version = cfg.version;
    migrate(&mut cfg)?;

    // Persist migrated config so future loads skip re-migration.
    // Also converts YAML → TOML on first load.
    if cfg.version != old_version || from_yaml {
        save(&cfg)?;
        if from_yaml {
            let _ = fs::remove_file(&yaml_path);
        }
    }

    cfg.validate().map_err(|e| CliError::new(ErrorCode::InvalidInput, e.to_string()))?;

    // Run task consistency checks (auto-repairs duplicates, filename mismatches, next_id drift).
    let old_next_id = cfg.next_id;
    if let Err(e) = crate::model::task::ensure_consistency(&mut cfg) {
        eprintln!("warning: consistency check failed: {}", e);
    }
    if cfg.next_id != old_next_id {
        save(&cfg)?;
    }

    Ok(cfg)
}

/// Save config to its config file (uses config's dir).
pub fn save(config: &Config) -> Result<(), CliError> {
    let data = toml::to_string_pretty(config).map_err(|e| {
        CliError::newf(
            ErrorCode::InternalError,
            format!("marshaling config: {}", e),
        )
    })?;
    write_with_mode(&config.config_path(), data.as_bytes())
}

/// Write config to a given kanban directory.
pub fn write(kanban_dir: &Path, cfg: &Config) -> Result<(), CliError> {
    let config_path = kanban_dir.join(CONFIG_FILE_NAME);
    let data = toml::to_string_pretty(cfg).map_err(|e| {
        CliError::newf(
            ErrorCode::InternalError,
            format!("marshaling config: {}", e),
        )
    })?;
    write_with_mode(&config_path, data.as_bytes())
}

/// Initialize a new board in a directory.
///
/// 1. Create directory and tasks subdirectory
/// 2. Create default config
/// 3. Save config
/// 4. Return config
pub fn init(dir: &Path, name: &str) -> Result<Config, CliError> {
    let abs_dir = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| {
                CliError::newf(
                    ErrorCode::InternalError,
                    format!("resolving path: {}", e),
                )
            })?
            .join(dir)
    };

    let mut cfg = Config::new_default(name);
    cfg.set_dir(abs_dir);

    // Create tasks directory (and parent kanban dir).
    create_dir_with_mode(&cfg.tasks_path(), DIR_MODE)?;

    save(&cfg)?;

    Ok(cfg)
}

/// Find the kanban directory by walking up from start_dir.
///
/// Walk upward looking for `kanban/config.toml` (or legacy `config.yml`).
/// If not found and in a git worktree, resolve main worktree and try there.
pub fn find_dir(start_dir: &Path) -> Result<PathBuf, CliError> {
    let abs_start = if start_dir.is_absolute() {
        start_dir.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| {
                CliError::newf(
                    ErrorCode::InternalError,
                    format!("resolving path: {}", e),
                )
            })?
            .join(start_dir)
    };

    if let Some(found) = find_dir_from(&abs_start) {
        return Ok(found);
    }

    // Normal walk exhausted. Try resolving through a git worktree.
    if let Some(main_root) = resolve_main_worktree(&abs_start) {
        if main_root != abs_start {
            if let Some(found) = find_dir_from(&main_root) {
                return Ok(found);
            }
        }
    }

    Err(CliError::new(
        ErrorCode::BoardNotFound,
        "no kanban board found (run 'kanban-md init' to create one)",
    ))
}

// ── Finding logic ──────────────────────────────────────────────────────

/// Walk up from a directory looking for config (TOML or legacy YAML).
fn find_dir_from(start_dir: &Path) -> Option<PathBuf> {
    let mut dir = start_dir.to_path_buf();
    loop {
        // Check dir/kanban/config.toml, then legacy config.yml
        let kanban_dir = dir.join(DEFAULT_DIR);
        if kanban_dir.join(CONFIG_FILE_NAME).exists()
            || kanban_dir.join(LEGACY_CONFIG_FILE_NAME).exists()
        {
            return Some(kanban_dir);
        }

        // Check dir/config.toml or config.yml (already inside kanban directory).
        if dir.join(CONFIG_FILE_NAME).exists()
            || dir.join(LEGACY_CONFIG_FILE_NAME).exists()
        {
            return Some(dir.clone());
        }

        // Walk up.
        let parent = dir.parent()?;
        if parent == dir {
            return None;
        }
        dir = parent.to_path_buf();
    }
}

/// Resolve main git worktree root from a linked worktree.
///
/// Finds a `.git` file (not directory) indicating a linked worktree,
/// parses `"gitdir: <path>"`, navigates up 2 levels to find the main
/// `.git` directory, and returns its parent (the main worktree root).
fn resolve_main_worktree(start_dir: &Path) -> Option<PathBuf> {
    let mut dir = start_dir.to_path_buf();
    loop {
        let git_path = dir.join(".git");
        if let Ok(metadata) = fs::symlink_metadata(&git_path) {
            if metadata.is_dir() {
                // Regular repo, not a linked worktree.
                return None;
            }
            // .git is a file -> linked worktree.
            let content = fs::read_to_string(&git_path).ok()?;
            let line = content.trim();
            let gitdir_str = line.strip_prefix("gitdir: ")?;

            let gitdir = if Path::new(gitdir_str).is_absolute() {
                PathBuf::from(gitdir_str)
            } else {
                dir.join(gitdir_str)
            };

            // gitdir points to <main>/.git/worktrees/<name>.
            // Go up two levels to reach <main>/.git, then take its parent.
            let main_git_dir = gitdir.join("..").join("..");
            let main_git_dir = main_git_dir.canonicalize().ok()?;
            let main_root = main_git_dir.parent()?;

            // Verify it's actually a git directory.
            if main_git_dir.is_dir() {
                return Some(main_root.to_path_buf());
            }
            return None;
        }

        let parent = dir.parent()?;
        if parent == dir {
            return None;
        }
        dir = parent.to_path_buf();
    }
}

// ── Migration system ───────────────────────────────────────────────────

/// Run migrations to bring config up to current version.
fn migrate(config: &mut Config) -> Result<(), CliError> {
    if config.version == CURRENT_VERSION {
        return Ok(());
    }
    if config.version > CURRENT_VERSION {
        return Err(CliError::newf(
            ErrorCode::InvalidInput,
            format!(
                "config version {} is newer than supported version {} (upgrade kanban-md)",
                config.version, CURRENT_VERSION
            ),
        ));
    }
    if config.version < 1 {
        return Err(CliError::newf(
            ErrorCode::InvalidInput,
            format!("config version {} is invalid", config.version),
        ));
    }

    // Apply migrations sequentially.
    while config.version < CURRENT_VERSION {
        let version = config.version;
        match version {
            1 => migrate_v1_to_v2(config),
            2 => migrate_v2_to_v3(config),
            3 => migrate_v3_to_v4(config),
            4 => migrate_v4_to_v5(config),
            5 => migrate_v5_to_v6(config),
            6 => migrate_v6_to_v7(config),
            7 => migrate_v7_to_v8(config),
            8 => migrate_v8_to_v9(config),
            9 => migrate_v9_to_v10(config),
            10 => migrate_v10_to_v11(config),
            11 => migrate_v11_to_v12(config),
            12 => migrate_v12_to_v13(config),
            13 => migrate_v13_to_v14(config),
            14 => migrate_v14_to_v15(config),
            _ => {
                return Err(CliError::newf(
                    ErrorCode::InvalidInput,
                    format!("no migration path from version {}", version),
                ));
            }
        }
    }

    Ok(())
}

/// v1 -> v2: adds wip_limits (empty map, which is the default).
fn migrate_v1_to_v2(cfg: &mut Config) {
    cfg.version = 2;
}

/// v2 -> v3: adds claim_timeout "1h", classes, defaults.class "standard".
fn migrate_v2_to_v3(cfg: &mut Config) {
    if cfg.claim_timeout.is_empty() {
        cfg.claim_timeout = DEFAULT_CLAIM_TIMEOUT.to_string();
    }
    if cfg.classes.is_empty() {
        cfg.classes = default_classes();
    }
    if cfg.defaults.class.is_empty() {
        cfg.defaults.class = DEFAULT_CLASS.to_string();
    }
    cfg.version = 3;
}

/// v3 -> v4: adds tui.title_lines = 1 (was the original default before v8->v9 bumped to 2).
fn migrate_v3_to_v4(cfg: &mut Config) {
    if cfg.tui.title_lines == 0 {
        cfg.tui.title_lines = DEFAULT_TITLE_LINES;
    }
    cfg.version = 4;
}

/// v4 -> v5: adds tui.age_thresholds defaults.
fn migrate_v4_to_v5(cfg: &mut Config) {
    if cfg.tui.age_thresholds.is_empty() {
        cfg.tui.age_thresholds = default_age_thresholds();
    }
    cfg.version = 5;
}

/// v5 -> v6: adds "archived" status.
fn migrate_v5_to_v6(cfg: &mut Config) {
    let names: Vec<String> = cfg.statuses.iter().map(|s| s.name.clone()).collect();
    if !names.contains(&ARCHIVED_STATUS.to_string()) {
        cfg.statuses.push(StatusConfig {
            name: ARCHIVED_STATUS.to_string(),
            ..Default::default()
        });
    }
    cfg.version = 6;
}

/// v6 -> v7: converts statuses to StatusConfig with require_claim.
/// The custom Deserialize on StatusConfig handles both string and mapping forms,
/// so this migration only bumps the version.
fn migrate_v6_to_v7(cfg: &mut Config) {
    cfg.version = 7;
}

/// v7 -> v8: adds show_duration to statuses.
/// Hide duration on the first status, the last non-archived status, and archived.
fn migrate_v7_to_v8(cfg: &mut Config) {
    if !cfg.statuses.is_empty() {
        let hide = Some(false);
        // Hide duration on first status.
        cfg.statuses[0].show_duration = hide;
        // Find last non-archived status and hide duration on it.
        let mut last_idx = cfg.statuses.len() - 1;
        if cfg.statuses[last_idx].name == ARCHIVED_STATUS {
            cfg.statuses[last_idx].show_duration = hide;
            if last_idx > 0 {
                last_idx -= 1;
            }
        }
        cfg.statuses[last_idx].show_duration = hide;
    }
    cfg.version = 8;
}

/// v8 -> v9: changes title_lines 1 -> 2.
fn migrate_v8_to_v9(cfg: &mut Config) {
    if cfg.tui.title_lines == 1 {
        cfg.tui.title_lines = DEFAULT_TITLE_LINES;
    }
    cfg.version = 9;
}

/// v9 -> v10: adds hide_empty_columns = false (zero-value default, no action needed).
fn migrate_v9_to_v10(cfg: &mut Config) {
    cfg.version = 10;
}

/// v10 -> v11: adds collapsed_columns [backlog, review, done, archived].
fn migrate_v10_to_v11(cfg: &mut Config) {
    if cfg.tui.collapsed_columns.is_empty() {
        cfg.tui.collapsed_columns = default_collapsed_columns();
    }
    cfg.version = 11;
}

/// v11 -> v12: adds reader_max_width = 120.
fn migrate_v11_to_v12(cfg: &mut Config) {
    if cfg.tui.reader_max_width == 0 {
        cfg.tui.reader_max_width = DEFAULT_READER_MAX_WIDTH;
    }
    cfg.version = 12;
}

/// v12 -> v13: adds "references" status before "done", adds to collapsed.
fn migrate_v12_to_v13(cfg: &mut Config) {
    let names: Vec<String> = cfg.statuses.iter().map(|s| s.name.clone()).collect();
    if !names.contains(&REFERENCES_STATUS.to_string()) {
        let ref_status = StatusConfig {
            name: REFERENCES_STATUS.to_string(),
            show_duration: Some(false),
            ..Default::default()
        };
        let mut inserted = false;
        // Try to insert before "done".
        if let Some(pos) = cfg.statuses.iter().position(|s| s.name == "done") {
            cfg.statuses.insert(pos, ref_status.clone());
            inserted = true;
        }
        // Fallback: insert before archived.
        if !inserted {
            if let Some(pos) = cfg.statuses.iter().position(|s| s.name == ARCHIVED_STATUS) {
                cfg.statuses.insert(pos, ref_status.clone());
                inserted = true;
            }
        }
        // Last fallback: append.
        if !inserted {
            cfg.statuses.push(ref_status);
        }
    }
    // Add references and archived to collapsed columns if not present.
    let to_add: Vec<String> = [REFERENCES_STATUS, ARCHIVED_STATUS]
        .iter()
        .filter(|s| !cfg.tui.collapsed_columns.iter().any(|c| c == **s))
        .map(|s| s.to_string())
        .collect();
    cfg.tui.collapsed_columns.extend(to_add);
    cfg.version = 13;
}

/// v13 -> v14: adds require_branch = false to all statuses (zero-value, no action needed).
fn migrate_v13_to_v14(cfg: &mut Config) {
    cfg.version = 14;
}

/// v14 -> v15: adds tui.reader_width_pct (default 40%).
fn migrate_v14_to_v15(cfg: &mut Config) {
    if cfg.tui.reader_width_pct == 0 {
        cfg.tui.reader_width_pct = DEFAULT_READER_WIDTH_PCT;
    }
    cfg.version = 15;
}

// ── File I/O helpers ───────────────────────────────────────────────────

/// Write bytes to a file with mode 0o600 on Unix.
fn write_with_mode(path: &Path, data: &[u8]) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                CliError::newf(
                    ErrorCode::InternalError,
                    format!("creating directory: {}", e),
                )
            })?;
        }
    }

    #[cfg(unix)]
    {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(FILE_MODE)
            .open(path)
            .map_err(|e| {
                CliError::newf(
                    ErrorCode::InternalError,
                    format!("writing config: {}", e),
                )
            })?;
        file.write_all(data).map_err(|e| {
            CliError::newf(
                ErrorCode::InternalError,
                format!("writing config: {}", e),
            )
        })?;
    }

    #[cfg(not(unix))]
    {
        fs::write(path, data).map_err(|e| {
            CliError::newf(
                ErrorCode::InternalError,
                format!("writing config: {}", e),
            )
        })?;
    }

    Ok(())
}

/// Create a directory with the specified mode on Unix.
fn create_dir_with_mode(path: &Path, _mode: u32) -> Result<(), CliError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(_mode)
            .create(path)
            .map_err(|e| {
                CliError::newf(
                    ErrorCode::InternalError,
                    format!("creating directory: {}", e),
                )
            })?;
    }

    #[cfg(not(unix))]
    {
        fs::create_dir_all(path).map_err(|e| {
            CliError::newf(
                ErrorCode::InternalError,
                format!("creating directory: {}", e),
            )
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_creates_directory_structure() {
        let tmp = tempfile::tempdir().unwrap();
        let kanban_dir = tmp.path().join("kanban");

        let cfg = init(&kanban_dir, "Test Board").unwrap();

        assert!(cfg.config_path().exists(), "config.toml should exist");
        assert!(cfg.tasks_path().exists(), "tasks directory should exist");
        assert_eq!(cfg.board.name, "Test Board");
        assert_eq!(cfg.version, CURRENT_VERSION);
        assert_eq!(cfg.next_id, 1);
    }

    #[test]
    fn test_init_and_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let kanban_dir = tmp.path().join("kanban");

        let original = init(&kanban_dir, "My Board").unwrap();

        let loaded = load(&kanban_dir).unwrap();
        assert_eq!(loaded.board.name, original.board.name);
        assert_eq!(loaded.version, CURRENT_VERSION);
        assert_eq!(loaded.tasks_dir, original.tasks_dir);
        assert_eq!(loaded.priorities, original.priorities);
        assert_eq!(loaded.next_id, original.next_id);
    }

    #[test]
    fn test_save_and_load() {
        let tmp = tempfile::tempdir().unwrap();
        let kanban_dir = tmp.path().join("kanban");

        let mut cfg = init(&kanban_dir, "Save Test").unwrap();
        cfg.next_id = 42;
        save(&cfg).unwrap();

        let loaded = load(&kanban_dir).unwrap();
        assert_eq!(loaded.next_id, 42);
    }

    #[test]
    fn test_find_dir_kanban_subdir() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("project");
        let kanban_dir = project_dir.join("kanban");
        fs::create_dir_all(&kanban_dir).unwrap();
        fs::write(kanban_dir.join(CONFIG_FILE_NAME), "version = 14\n").unwrap();

        let found = find_dir(&project_dir).unwrap();
        assert_eq!(
            found.canonicalize().unwrap(),
            kanban_dir.canonicalize().unwrap()
        );
    }

    #[test]
    fn test_find_dir_direct_config() {
        let tmp = tempfile::tempdir().unwrap();
        let kanban_dir = tmp.path().join("myboard");
        fs::create_dir_all(&kanban_dir).unwrap();
        fs::write(kanban_dir.join(CONFIG_FILE_NAME), "version = 14\n").unwrap();

        let found = find_dir(&kanban_dir).unwrap();
        assert_eq!(
            found.canonicalize().unwrap(),
            kanban_dir.canonicalize().unwrap()
        );
    }

    #[test]
    fn test_find_dir_walks_up() {
        let tmp = tempfile::tempdir().unwrap();
        let kanban_dir = tmp.path().join("kanban");
        fs::create_dir_all(&kanban_dir).unwrap();
        fs::write(kanban_dir.join(CONFIG_FILE_NAME), "version = 14\n").unwrap();

        let nested = tmp.path().join("a").join("b").join("c");
        fs::create_dir_all(&nested).unwrap();

        let found = find_dir(&nested).unwrap();
        assert_eq!(
            found.canonicalize().unwrap(),
            kanban_dir.canonicalize().unwrap()
        );
    }

    #[test]
    fn test_find_dir_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let result = find_dir(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_load_nonexistent_dir() {
        let path = Path::new("/tmp/nonexistent-kanban-dir-12345");
        let result = load(path);
        assert!(result.is_err());
    }

    #[test]
    fn test_migrate_v1_to_current() {
        let mut cfg = Config::new_default("Test");
        cfg.version = 1;
        // Reset fields that migrations should populate.
        cfg.claim_timeout = String::new();
        cfg.classes = Vec::new();
        cfg.defaults.class = String::new();
        cfg.tui.title_lines = 0;
        cfg.tui.age_thresholds = Vec::new();
        cfg.tui.collapsed_columns = Vec::new();
        cfg.tui.reader_max_width = 0;
        // Remove statuses that later migrations add.
        cfg.statuses = vec![
            StatusConfig { name: "backlog".into(), ..Default::default() },
            StatusConfig { name: "todo".into(), ..Default::default() },
            StatusConfig { name: "in-progress".into(), ..Default::default() },
            StatusConfig { name: "review".into(), ..Default::default() },
            StatusConfig { name: "done".into(), ..Default::default() },
        ];

        migrate(&mut cfg).unwrap();
        assert_eq!(cfg.version, CURRENT_VERSION);

        // Verify v2->v3 additions.
        assert_eq!(cfg.claim_timeout, DEFAULT_CLAIM_TIMEOUT);
        assert!(!cfg.classes.is_empty());
        assert_eq!(cfg.defaults.class, DEFAULT_CLASS);

        // Verify v3->v4 title_lines.
        assert_eq!(cfg.tui.title_lines, DEFAULT_TITLE_LINES);

        // Verify v4->v5 age_thresholds.
        assert!(!cfg.tui.age_thresholds.is_empty());

        // Verify v5->v6 archived status.
        let names = cfg.status_names();
        assert!(names.contains(&ARCHIVED_STATUS.to_string()));

        // Verify v12->v13 references status.
        assert!(names.contains(&REFERENCES_STATUS.to_string()));

        // Verify v10->v11 collapsed columns.
        assert!(!cfg.tui.collapsed_columns.is_empty());

        // Verify v11->v12 reader_max_width.
        assert_eq!(cfg.tui.reader_max_width, DEFAULT_READER_MAX_WIDTH);
    }

    #[test]
    fn test_migrate_already_current() {
        let mut cfg = Config::new_default("Test");
        assert_eq!(cfg.version, CURRENT_VERSION);
        migrate(&mut cfg).unwrap();
        assert_eq!(cfg.version, CURRENT_VERSION);
    }

    #[test]
    fn test_migrate_future_version_errors() {
        let mut cfg = Config::new_default("Test");
        cfg.version = CURRENT_VERSION + 1;
        let result = migrate(&mut cfg);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("newer"));
    }

    #[test]
    fn test_migrate_invalid_version_errors() {
        let mut cfg = Config::new_default("Test");
        cfg.version = 0;
        let result = migrate(&mut cfg);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid"));
    }

    #[test]
    fn test_migrate_v5_to_v6_archived() {
        let mut cfg = Config::new_default("Test");
        cfg.version = 5;
        cfg.statuses = vec![
            StatusConfig { name: "backlog".into(), ..Default::default() },
            StatusConfig { name: "done".into(), ..Default::default() },
        ];
        migrate_v5_to_v6(&mut cfg);
        assert_eq!(cfg.version, 6);
        let names: Vec<String> = cfg.statuses.iter().map(|s| s.name.clone()).collect();
        assert!(names.contains(&ARCHIVED_STATUS.to_string()));
    }

    #[test]
    fn test_migrate_v5_to_v6_already_has_archived() {
        let mut cfg = Config::new_default("Test");
        cfg.version = 5;
        cfg.statuses = vec![
            StatusConfig { name: "backlog".into(), ..Default::default() },
            StatusConfig { name: ARCHIVED_STATUS.into(), ..Default::default() },
        ];
        let count_before = cfg.statuses.len();
        migrate_v5_to_v6(&mut cfg);
        assert_eq!(
            cfg.statuses.len(),
            count_before,
            "should not duplicate archived"
        );
    }

    #[test]
    fn test_migrate_v7_to_v8_show_duration() {
        let mut cfg = Config::new_default("Test");
        cfg.version = 7;
        cfg.statuses = vec![
            StatusConfig { name: "backlog".into(), ..Default::default() },
            StatusConfig { name: "in-progress".into(), ..Default::default() },
            StatusConfig { name: "done".into(), ..Default::default() },
            StatusConfig { name: ARCHIVED_STATUS.into(), ..Default::default() },
        ];
        migrate_v7_to_v8(&mut cfg);
        assert_eq!(cfg.version, 8);
        // First status should have show_duration = false.
        assert_eq!(cfg.statuses[0].show_duration, Some(false));
        // Archived should have show_duration = false.
        assert_eq!(cfg.statuses[3].show_duration, Some(false));
        // "done" (last non-archived) should have show_duration = false.
        assert_eq!(cfg.statuses[2].show_duration, Some(false));
        // "in-progress" should be untouched (None).
        assert_eq!(cfg.statuses[1].show_duration, None);
    }

    #[test]
    fn test_migrate_v12_to_v13_references() {
        let mut cfg = Config::new_default("Test");
        cfg.version = 12;
        cfg.statuses = vec![
            StatusConfig { name: "backlog".into(), ..Default::default() },
            StatusConfig { name: "todo".into(), ..Default::default() },
            StatusConfig { name: "in-progress".into(), ..Default::default() },
            StatusConfig { name: "review".into(), ..Default::default() },
            StatusConfig { name: "done".into(), ..Default::default() },
            StatusConfig { name: ARCHIVED_STATUS.into(), ..Default::default() },
        ];
        cfg.tui.collapsed_columns = vec!["backlog".into(), "done".into()];

        migrate_v12_to_v13(&mut cfg);
        assert_eq!(cfg.version, 13);

        let names: Vec<String> = cfg.statuses.iter().map(|s| s.name.clone()).collect();
        // References should be inserted before "done".
        let ref_idx = names.iter().position(|n| n == REFERENCES_STATUS).unwrap();
        let done_idx = names.iter().position(|n| n == "done").unwrap();
        assert!(ref_idx < done_idx, "references should be before done");

        // Collapsed columns should include references and archived.
        assert!(cfg
            .tui
            .collapsed_columns
            .contains(&REFERENCES_STATUS.to_string()));
        assert!(cfg
            .tui
            .collapsed_columns
            .contains(&ARCHIVED_STATUS.to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn test_config_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let kanban_dir = tmp.path().join("kanban");

        init(&kanban_dir, "Perms Test").unwrap();

        let metadata = fs::metadata(kanban_dir.join(CONFIG_FILE_NAME)).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, FILE_MODE, "config file should have mode 0o600");
    }

    #[test]
    fn test_find_dir_from_inside_kanban_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let kanban_dir = tmp.path().join("kanban");
        let tasks_dir = kanban_dir.join("tasks");
        fs::create_dir_all(&tasks_dir).unwrap();
        fs::write(kanban_dir.join(CONFIG_FILE_NAME), "version = 14\n").unwrap();

        // Starting from inside the tasks directory should still find the kanban dir.
        let found = find_dir(&tasks_dir).unwrap();
        assert_eq!(
            found.canonicalize().unwrap(),
            kanban_dir.canonicalize().unwrap()
        );
    }

    #[test]
    fn test_status_config_deserialize_yaml_string() {
        let yaml = "backlog";
        let sc: StatusConfig = serde_yml::from_str(yaml).unwrap();
        assert_eq!(sc.name, "backlog");
        assert!(!sc.require_claim);
        assert!(!sc.require_branch);
        assert_eq!(sc.show_duration, None);
    }

    #[test]
    fn test_status_config_deserialize_yaml_mapping() {
        let yaml = "name: in-progress\nrequire_claim: true\nshow_duration: false\n";
        let sc: StatusConfig = serde_yml::from_str(yaml).unwrap();
        assert_eq!(sc.name, "in-progress");
        assert!(sc.require_claim);
        assert!(!sc.require_branch);
        assert_eq!(sc.show_duration, Some(false));
    }

    #[test]
    fn test_auto_migrate_yaml_to_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let kanban_dir = tmp.path().join("kanban");
        let tasks_dir = kanban_dir.join("tasks");
        fs::create_dir_all(&tasks_dir).unwrap();

        // Write a legacy YAML config.
        let yaml_config = Config::new_default("YAML Board");
        let yaml_data = serde_yml::to_string(&yaml_config).unwrap();
        fs::write(kanban_dir.join(LEGACY_CONFIG_FILE_NAME), &yaml_data).unwrap();

        // Verify YAML file exists and TOML does not.
        assert!(kanban_dir.join(LEGACY_CONFIG_FILE_NAME).exists());
        assert!(!kanban_dir.join(CONFIG_FILE_NAME).exists());

        // Load should auto-migrate.
        let loaded = load(&kanban_dir).unwrap();
        assert_eq!(loaded.board.name, "YAML Board");
        assert_eq!(loaded.version, CURRENT_VERSION);

        // After load, TOML file should exist and YAML should be removed.
        assert!(kanban_dir.join(CONFIG_FILE_NAME).exists(), "config.toml should exist after migration");
        assert!(!kanban_dir.join(LEGACY_CONFIG_FILE_NAME).exists(), "config.yml should be removed after migration");

        // Verify the TOML file is parseable.
        let toml_data = fs::read_to_string(kanban_dir.join(CONFIG_FILE_NAME)).unwrap();
        let reloaded: Config = toml::from_str(&toml_data).unwrap();
        assert_eq!(reloaded.board.name, "YAML Board");
    }

    #[test]
    fn test_find_dir_legacy_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        let kanban_dir = tmp.path().join("kanban");
        fs::create_dir_all(&kanban_dir).unwrap();
        // Write a legacy config.yml — find_dir should still locate it.
        fs::write(kanban_dir.join(LEGACY_CONFIG_FILE_NAME), "version = 15\n").unwrap();

        let found = find_dir(tmp.path()).unwrap();
        assert_eq!(
            found.canonicalize().unwrap(),
            kanban_dir.canonicalize().unwrap()
        );
    }

    #[test]
    fn test_config_validate_valid() {
        let cfg = Config::new_default("Valid Board");
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_config_validate_empty_board_name() {
        let mut cfg = Config::new_default("");
        cfg.board.name = String::new();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_config_validate_too_few_statuses() {
        let mut cfg = Config::new_default("Test");
        cfg.statuses = vec![StatusConfig {
            name: "only-one".into(),
            ..Default::default()
        }];
        cfg.defaults.status = "only-one".into();
        assert!(cfg.validate().is_err());
    }
}
