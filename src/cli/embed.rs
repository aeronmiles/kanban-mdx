//! `kanban-md embed` — manage semantic search embeddings.
//!
//! Subcommands:
//! - `sync`   — generate/update embeddings for all tasks
//! - `status` — show embedding index status
//! - `clear`  — remove all stored embeddings

use crate::cli::root::Cli;
use crate::embed;
use crate::error::{CliError, ErrorCode};
use crate::model::task;
use crate::output::Format;

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

#[derive(clap::Args, Clone)]
pub struct EmbedArgs {
    #[command(subcommand)]
    pub command: EmbedCommands,
}

#[derive(clap::Subcommand, Clone)]
pub enum EmbedCommands {
    /// Sync embedding index with current tasks.
    Sync,
    /// Show embedding index status.
    Status,
    /// Delete the embedding index.
    Clear,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn run(cli: &Cli, args: EmbedArgs) -> Result<(), CliError> {
    match args.command {
        EmbedCommands::Sync => run_sync(cli),
        EmbedCommands::Status => run_status(cli),
        EmbedCommands::Clear => run_clear(cli),
    }
}

// ---------------------------------------------------------------------------
// Subcommand implementations
// ---------------------------------------------------------------------------

fn run_sync(cli: &Cli) -> Result<(), CliError> {
    let cfg = crate::cli::root::load_config(cli)?;

    if !cfg.semantic_search.enabled {
        return Err(CliError::new(
            ErrorCode::InvalidInput,
            "semantic search not configured; add semantic_search section to config.toml",
        ));
    }

    let mut mgr = embed::Manager::new(&cfg).map_err(|e| {
        CliError::newf(ErrorCode::InternalError, format!("{e}"))
    })?;

    let (tasks, _) = task::read_all_lenient(&cfg.tasks_path()).map_err(|e| {
        CliError::newf(ErrorCode::InternalError, format!("{e}"))
    })?;

    let stats = mgr.sync(&tasks).map_err(|e| {
        CliError::newf(ErrorCode::InternalError, format!("{e}"))
    })?;

    let format = crate::cli::root::output_format(cli);
    let mut stdout = std::io::stdout();

    match format {
        Format::Json => {
            let json_stats = serde_json::json!({
                "total_tasks": stats.total_tasks,
                "total_chunks": stats.total_chunks,
                "embedded": stats.embedded,
                "pruned": stats.pruned,
            });
            crate::output::json::json(&mut stdout, &json_stats)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        _ => {
            use std::io::Write;
            writeln!(
                stdout,
                "Synced embeddings for {} tasks ({} chunks, {} embedded, {} pruned)",
                stats.total_tasks, stats.total_chunks, stats.embedded, stats.pruned
            )
            .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
    }

    Ok(())
}

fn run_status(cli: &Cli) -> Result<(), CliError> {
    let cfg = crate::cli::root::load_config(cli)?;
    let status = embed::get_status(&cfg);

    let format = crate::cli::root::output_format(cli);
    let mut stdout = std::io::stdout();

    match format {
        Format::Json => {
            crate::output::json::json(&mut stdout, &status)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        _ => {
            use std::io::Write;
            writeln!(stdout, "Semantic search: {}", enabled_str(status.enabled))
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
            if !status.provider.is_empty() {
                writeln!(stdout, "Provider:        {}", status.provider)
                    .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
            }
            if !status.model.is_empty() {
                writeln!(stdout, "Model:           {}", status.model)
                    .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
            }
            if !status.index_file.is_empty() {
                writeln!(
                    stdout,
                    "Index:           {} ({} documents, {})",
                    status.index_file,
                    status.documents,
                    human_bytes(status.file_size_bytes)
                )
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
                writeln!(stdout, "Last sync:       {}", status.last_sync)
                    .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
            } else {
                writeln!(
                    stdout,
                    "Index:           not created (run 'kanban-md embed sync')"
                )
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
            }
        }
    }

    Ok(())
}

fn run_clear(cli: &Cli) -> Result<(), CliError> {
    let cfg = crate::cli::root::load_config(cli)?;
    let index_path = cfg.dir().join(embed::INDEX_FILE);

    let mut stdout = std::io::stdout();

    if !index_path.exists() {
        use std::io::Write;
        writeln!(stdout, "No embedding index to remove")
            .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        return Ok(());
    }

    std::fs::remove_file(&index_path).map_err(|e| {
        CliError::newf(
            ErrorCode::InternalError,
            format!("removing index: {e}"),
        )
    })?;

    use std::io::Write;
    writeln!(
        stdout,
        "Removed embedding index: {}",
        index_path.display()
    )
    .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn enabled_str(enabled: bool) -> &'static str {
    if enabled {
        "enabled"
    } else {
        "disabled"
    }
}

fn human_bytes(b: u64) -> String {
    const UNIT: u64 = 1024;
    if b < UNIT {
        return format!("{} B", b);
    }
    let mut div = UNIT;
    let mut exp = 0u32;
    let mut n = b / UNIT;
    while n >= UNIT {
        n /= UNIT;
        div *= UNIT;
        exp += 1;
    }
    let units = ['K', 'M', 'G', 'T', 'P', 'E'];
    format!("{:.1} {}B", b as f64 / div as f64, units[exp as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::config::Config;

    #[test]
    fn test_enabled_str() {
        assert_eq!(enabled_str(true), "enabled");
        assert_eq!(enabled_str(false), "disabled");
    }

    #[test]
    fn test_human_bytes() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(1048576), "1.0 MB");
    }

    #[test]
    fn test_human_bytes_gigabyte() {
        assert_eq!(human_bytes(1073741824), "1.0 GB");
    }

    #[test]
    fn test_human_bytes_boundary_just_under_1k() {
        assert_eq!(human_bytes(1023), "1023 B");
    }

    // -- get_status tests (via embed::get_status) --

    #[test]
    fn test_get_status_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::new_default("test");
        cfg.set_dir(dir.path().to_path_buf());
        // semantic_search is disabled by default
        assert!(!cfg.semantic_search.enabled);

        let status = embed::get_status(&cfg);
        assert!(!status.enabled);
        assert_eq!(status.documents, 0);
        assert!(status.index_file.is_empty());
        assert_eq!(status.provider, "voyage");
        assert!(status.model.is_empty());
    }

    #[test]
    fn test_get_status_enabled_no_index_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::new_default("test");
        cfg.set_dir(dir.path().to_path_buf());
        cfg.semantic_search.enabled = true;
        cfg.semantic_search.provider = "voyage".to_string();
        cfg.semantic_search.model = "voyage-3-lite".to_string();

        // No .embeddings.json exists in the temp dir
        let status = embed::get_status(&cfg);
        assert!(status.enabled);
        assert_eq!(status.provider, "voyage");
        assert_eq!(status.model, "voyage-3-lite");
        assert_eq!(status.documents, 0);
        assert!(status.index_file.is_empty());
        assert_eq!(status.file_size_bytes, 0);
    }

    #[test]
    fn test_get_status_enabled_with_index_file() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join(embed::INDEX_FILE);

        // Create a populated index and save it to disk.
        let idx = sembed_rs::Index::new();
        let doc = sembed_rs::Document {
            id: "1:0".to_string(),
            content: "hello world".to_string(),
            content_hash: "abc123".to_string(),
            vector: vec![1.0, 0.0, 0.0],
            metadata: std::collections::HashMap::new(),
        };
        idx.add(vec![doc]);
        let f = std::fs::File::create(&index_path).unwrap();
        idx.save(f).unwrap();

        let mut cfg = Config::new_default("test");
        cfg.set_dir(dir.path().to_path_buf());
        cfg.semantic_search.enabled = true;
        cfg.semantic_search.provider = "openai".to_string();
        cfg.semantic_search.model = "text-embedding-3-small".to_string();

        let status = embed::get_status(&cfg);
        assert!(status.enabled);
        assert_eq!(status.documents, 1);
        assert!(!status.index_file.is_empty());
        assert!(status.file_size_bytes > 0);
        assert!(!status.last_sync.is_empty());
    }

    #[test]
    fn test_get_status_enabled_with_multiple_documents() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join(embed::INDEX_FILE);

        let idx = sembed_rs::Index::new();
        for i in 0..5 {
            let doc = sembed_rs::Document {
                id: format!("{}:0", i),
                content: format!("task {}", i),
                content_hash: format!("hash_{}", i),
                vector: vec![i as f32, 0.0],
                metadata: std::collections::HashMap::new(),
            };
            idx.add(vec![doc]);
        }
        let f = std::fs::File::create(&index_path).unwrap();
        idx.save(f).unwrap();

        let mut cfg = Config::new_default("test");
        cfg.set_dir(dir.path().to_path_buf());
        cfg.semantic_search.enabled = true;
        cfg.semantic_search.provider = "voyage".to_string();
        cfg.semantic_search.model = "voyage-3".to_string();

        let status = embed::get_status(&cfg);
        assert_eq!(status.documents, 5);
    }

    #[test]
    fn test_get_status_disabled_skips_document_count() {
        // Even when an index file exists, if semantic_search is disabled,
        // get_status should not count documents.
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join(embed::INDEX_FILE);

        let idx = sembed_rs::Index::new();
        let doc = sembed_rs::Document {
            id: "1:0".to_string(),
            content: "hello".to_string(),
            content_hash: "abc".to_string(),
            vector: vec![1.0],
            metadata: std::collections::HashMap::new(),
        };
        idx.add(vec![doc]);
        let f = std::fs::File::create(&index_path).unwrap();
        idx.save(f).unwrap();

        let mut cfg = Config::new_default("test");
        cfg.set_dir(dir.path().to_path_buf());
        // semantic_search.enabled is false by default

        let status = embed::get_status(&cfg);
        assert!(!status.enabled);
        // The index file exists, so index_file/file_size_bytes are populated,
        // but documents is 0 because get_status skips loading when disabled.
        assert!(!status.index_file.is_empty());
        assert!(status.file_size_bytes > 0);
        assert_eq!(status.documents, 0);
    }

    // -- Status output rendering tests --

    #[test]
    fn test_status_json_output() {
        let status = embed::EmbedStatus {
            enabled: true,
            provider: "voyage".to_string(),
            model: "voyage-3".to_string(),
            index_file: "/tmp/test/.embeddings.json".to_string(),
            documents: 42,
            file_size_bytes: 12345,
            last_sync: "2026-01-15T10:30:00Z".to_string(),
        };

        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["enabled"], true);
        assert_eq!(json["provider"], "voyage");
        assert_eq!(json["model"], "voyage-3");
        assert_eq!(json["documents"], 42);
        assert_eq!(json["file_size_bytes"], 12345);
    }

    #[test]
    fn test_status_json_disabled() {
        let status = embed::EmbedStatus {
            enabled: false,
            provider: String::new(),
            model: String::new(),
            index_file: String::new(),
            documents: 0,
            file_size_bytes: 0,
            last_sync: String::new(),
        };

        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["enabled"], false);
        assert_eq!(json["documents"], 0);
    }
}
