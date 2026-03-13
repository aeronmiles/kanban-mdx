//! Undo/redo support via file snapshot stacks.
//!
//! Provides two APIs:
//! - Simple: `UndoStack` stored as a single JSON file (`.undo.json`)
//! - JSONL: Individual entries in append-only JSONL files (`undo.jsonl`, `redo.jsonl`)
//!   with max 100 entries and automatic truncation.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Simple stack API (JSON file)
// ---------------------------------------------------------------------------

/// A snapshot of files before a mutation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub action: String,
    pub files: Vec<FileSnapshot>,
}

/// A snapshot of a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnapshot {
    pub path: String,
    pub content: Option<String>,
    pub existed: bool,
}

/// The undo/redo stack stored on disk.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UndoStack {
    pub undo: Vec<Snapshot>,
    pub redo: Vec<Snapshot>,
}

const UNDO_FILE: &str = ".undo.json";

/// Load the undo stack from the kanban directory.
pub fn load_stack(kanban_dir: &Path) -> UndoStack {
    let path = kanban_dir.join(UNDO_FILE);
    if !path.exists() {
        return UndoStack::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => UndoStack::default(),
    }
}

/// Save the undo stack to the kanban directory.
pub fn save_stack(kanban_dir: &Path, stack: &UndoStack) -> Result<(), Box<dyn std::error::Error>> {
    let path = kanban_dir.join(UNDO_FILE);
    let json = serde_json::to_string_pretty(stack)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Take a snapshot of the given files for undo.
pub fn snapshot_files(paths: &[PathBuf], action: &str) -> Snapshot {
    let files = paths
        .iter()
        .map(|p| {
            let existed = p.exists();
            let content = if existed {
                std::fs::read_to_string(p).ok()
            } else {
                None
            };
            FileSnapshot {
                path: p.to_string_lossy().to_string(),
                content,
                existed,
            }
        })
        .collect();
    Snapshot {
        action: action.to_string(),
        files,
    }
}

/// Restore files from a snapshot, returning a reverse snapshot for redo.
pub fn restore_snapshot(snapshot: &Snapshot) -> Result<Snapshot, Box<dyn std::error::Error>> {
    let mut reverse_files = Vec::new();
    for file_snap in &snapshot.files {
        let path = PathBuf::from(&file_snap.path);
        // Save current state for reverse snapshot
        let current_existed = path.exists();
        let current_content = if current_existed {
            std::fs::read_to_string(&path).ok()
        } else {
            None
        };
        reverse_files.push(FileSnapshot {
            path: file_snap.path.clone(),
            content: current_content,
            existed: current_existed,
        });

        // Restore from snapshot
        if file_snap.existed {
            if let Some(ref content) = file_snap.content {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&path, content)?;
            }
        } else if path.exists() {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(Snapshot {
        action: format!("undo: {}", snapshot.action),
        files: reverse_files,
    })
}

// ---------------------------------------------------------------------------
// JSONL journal API (matching Go implementation)
// ---------------------------------------------------------------------------

const UNDO_JSONL_FILE: &str = "undo.jsonl";
const REDO_JSONL_FILE: &str = "redo.jsonl";
const MAX_UNDO_ENTRIES: usize = 100;

/// Records a single undoable mutation with before/after file states (JSONL format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoEntry {
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub task_id: i32,
    pub detail: String,
    pub files_before: Vec<FileSnapshot>,
    pub files_after: Vec<FileSnapshot>,
}

/// Captures the current on-disk state of a single file.
pub fn snapshot_file(path: &Path) -> FileSnapshot {
    match std::fs::read_to_string(path) {
        Ok(content) => FileSnapshot {
            path: path.to_string_lossy().to_string(),
            content: Some(content),
            existed: true,
        },
        Err(_) => FileSnapshot {
            path: path.to_string_lossy().to_string(),
            content: None,
            existed: false,
        },
    }
}

/// Writes file snapshots back to disk.
///
/// Files with `existed=true` are written; files with `existed=false` are removed.
pub fn restore_file_snapshots(snapshots: &[FileSnapshot]) -> Result<(), Box<dyn std::error::Error>> {
    for snap in snapshots {
        let path = Path::new(&snap.path);
        if snap.existed {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if let Some(ref content) = snap.content {
                std::fs::write(path, content)?;
            }
        } else {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    Ok(())
}

/// Appends an entry to the undo journal and clears the redo journal.
pub fn record_undo(
    kanban_dir: &Path,
    entry: &UndoEntry,
) -> Result<(), Box<dyn std::error::Error>> {
    let undo_path = kanban_dir.join(UNDO_JSONL_FILE);
    append_journal(&undo_path, entry)?;
    let _ = truncate_journal_if_needed(&undo_path);
    clear_redo_journal(kanban_dir)?;
    Ok(())
}

/// Removes and returns the last entry from the undo journal.
pub fn pop_undo(kanban_dir: &Path) -> Result<Option<UndoEntry>, Box<dyn std::error::Error>> {
    pop_journal(&kanban_dir.join(UNDO_JSONL_FILE))
}

/// Returns the last entry from the undo journal without removing it.
pub fn peek_undo(kanban_dir: &Path) -> Result<Option<UndoEntry>, Box<dyn std::error::Error>> {
    peek_journal(&kanban_dir.join(UNDO_JSONL_FILE))
}

/// Appends an entry to the redo journal.
pub fn push_redo(
    kanban_dir: &Path,
    entry: &UndoEntry,
) -> Result<(), Box<dyn std::error::Error>> {
    append_journal(&kanban_dir.join(REDO_JSONL_FILE), entry)
}

/// Removes and returns the last entry from the redo journal.
pub fn pop_redo(kanban_dir: &Path) -> Result<Option<UndoEntry>, Box<dyn std::error::Error>> {
    pop_journal(&kanban_dir.join(REDO_JSONL_FILE))
}

/// Returns the last entry from the redo journal without removing it.
pub fn peek_redo(kanban_dir: &Path) -> Result<Option<UndoEntry>, Box<dyn std::error::Error>> {
    peek_journal(&kanban_dir.join(REDO_JSONL_FILE))
}

/// Removes all entries from the redo journal.
pub fn clear_redo_journal(kanban_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let path = kanban_dir.join(REDO_JSONL_FILE);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Returns the number of entries in the undo journal.
pub fn undo_depth(kanban_dir: &Path) -> usize {
    journal_depth(&kanban_dir.join(UNDO_JSONL_FILE))
}

/// Returns the number of entries in the redo journal.
pub fn redo_depth(kanban_dir: &Path) -> usize {
    journal_depth(&kanban_dir.join(REDO_JSONL_FILE))
}

/// Appends an entry to the undo journal without clearing redo.
/// Used by the redo command to restore the undo stack entry.
pub fn append_undo_only(
    kanban_dir: &Path,
    entry: &UndoEntry,
) -> Result<(), Box<dyn std::error::Error>> {
    append_journal(&kanban_dir.join(UNDO_JSONL_FILE), entry)
}

// ---------------------------------------------------------------------------
// JSONL journal helpers
// ---------------------------------------------------------------------------

fn append_journal(path: &Path, entry: &UndoEntry) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)?;

    let data = serde_json::to_string(entry)?;
    writeln!(f, "{}", data)?;
    Ok(())
}

fn read_journal_lines(path: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };

    let reader = BufReader::new(f);
    let mut lines = Vec::new();
    for line_result in reader.lines() {
        let line = line_result?;
        if !line.is_empty() {
            lines.push(line);
        }
    }
    Ok(lines)
}

fn write_journal_lines(path: &Path, lines: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = String::new();
    for line in lines {
        buf.push_str(line);
        buf.push('\n');
    }
    std::fs::write(path, buf)?;
    Ok(())
}

fn pop_journal(path: &Path) -> Result<Option<UndoEntry>, Box<dyn std::error::Error>> {
    let lines = read_journal_lines(path)?;
    if lines.is_empty() {
        return Ok(None);
    }

    let last = &lines[lines.len() - 1];
    let remaining = &lines[..lines.len() - 1];

    if remaining.is_empty() {
        let _ = std::fs::remove_file(path);
    } else {
        write_journal_lines(path, remaining)?;
    }

    let entry: UndoEntry = serde_json::from_str(last)?;
    Ok(Some(entry))
}

fn peek_journal(path: &Path) -> Result<Option<UndoEntry>, Box<dyn std::error::Error>> {
    let lines = read_journal_lines(path)?;
    if lines.is_empty() {
        return Ok(None);
    }

    let entry: UndoEntry = serde_json::from_str(&lines[lines.len() - 1])?;
    Ok(Some(entry))
}

fn journal_depth(path: &Path) -> usize {
    read_journal_lines(path).map(|l| l.len()).unwrap_or(0)
}

fn truncate_journal_if_needed(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let lines = read_journal_lines(path)?;
    if lines.len() <= MAX_UNDO_ENTRIES {
        return Ok(());
    }
    let keep = &lines[lines.len() - MAX_UNDO_ENTRIES..];
    write_journal_lines(path, keep)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // --- Simple stack tests ---

    #[test]
    fn test_snapshot_and_restore() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.md");
        std::fs::write(&file_path, "original").unwrap();

        let snap = snapshot_files(&[file_path.clone()], "edit");
        assert_eq!(snap.files.len(), 1);
        assert!(snap.files[0].existed);

        // Modify the file
        std::fs::write(&file_path, "modified").unwrap();

        // Restore
        let reverse = restore_snapshot(&snap).unwrap();
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "original");
        assert!(reverse.files[0].existed);
    }

    #[test]
    fn test_stack_save_load() {
        let dir = tempdir().unwrap();
        let mut stack = UndoStack::default();
        stack.undo.push(Snapshot {
            action: "test".to_string(),
            files: Vec::new(),
        });
        save_stack(dir.path(), &stack).unwrap();

        let loaded = load_stack(dir.path());
        assert_eq!(loaded.undo.len(), 1);
        assert_eq!(loaded.undo[0].action, "test");
    }

    // --- JSONL journal tests ---

    fn make_entry(action: &str, task_id: i32) -> UndoEntry {
        UndoEntry {
            timestamp: Utc::now(),
            action: action.to_string(),
            task_id,
            detail: format!("{} task {}", action, task_id),
            files_before: Vec::new(),
            files_after: Vec::new(),
        }
    }

    #[test]
    fn test_undo_redo_cycle() {
        let dir = tempdir().unwrap();
        let kanban_dir = dir.path();

        let entry = make_entry("move", 1);
        record_undo(kanban_dir, &entry).unwrap();
        assert_eq!(undo_depth(kanban_dir), 1);
        assert_eq!(redo_depth(kanban_dir), 0);

        let popped = pop_undo(kanban_dir).unwrap().unwrap();
        assert_eq!(popped.task_id, 1);
        assert_eq!(undo_depth(kanban_dir), 0);

        push_redo(kanban_dir, &popped).unwrap();
        assert_eq!(redo_depth(kanban_dir), 1);

        let redo_entry = pop_redo(kanban_dir).unwrap().unwrap();
        assert_eq!(redo_entry.task_id, 1);
    }

    #[test]
    fn test_record_undo_clears_redo() {
        let dir = tempdir().unwrap();
        let kanban_dir = dir.path();

        push_redo(kanban_dir, &make_entry("old", 1)).unwrap();
        assert_eq!(redo_depth(kanban_dir), 1);

        record_undo(kanban_dir, &make_entry("new", 2)).unwrap();
        assert_eq!(redo_depth(kanban_dir), 0);
    }

    #[test]
    fn test_peek_does_not_remove() {
        let dir = tempdir().unwrap();
        let kanban_dir = dir.path();

        record_undo(kanban_dir, &make_entry("move", 1)).unwrap();

        let peeked = peek_undo(kanban_dir).unwrap().unwrap();
        assert_eq!(peeked.task_id, 1);
        assert_eq!(undo_depth(kanban_dir), 1);
    }

    #[test]
    fn test_empty_journal() {
        let dir = tempdir().unwrap();
        assert!(pop_undo(dir.path()).unwrap().is_none());
        assert!(peek_undo(dir.path()).unwrap().is_none());
        assert_eq!(undo_depth(dir.path()), 0);
    }

    #[test]
    fn test_snapshot_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("task.md");
        std::fs::write(&file_path, "hello world").unwrap();

        let snap = snapshot_file(&file_path);
        assert!(snap.existed);
        assert_eq!(snap.content.as_deref(), Some("hello world"));
    }

    #[test]
    fn test_snapshot_missing_file() {
        let snap = snapshot_file(Path::new("/nonexistent/file.md"));
        assert!(!snap.existed);
        assert!(snap.content.is_none());
    }

    #[test]
    fn test_restore_file_snapshots() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("task.md");

        let snaps = vec![FileSnapshot {
            path: file_path.to_string_lossy().to_string(),
            content: Some("restored content".to_string()),
            existed: true,
        }];
        restore_file_snapshots(&snaps).unwrap();
        assert_eq!(
            std::fs::read_to_string(&file_path).unwrap(),
            "restored content"
        );

        let snaps_del = vec![FileSnapshot {
            path: file_path.to_string_lossy().to_string(),
            content: None,
            existed: false,
        }];
        restore_file_snapshots(&snaps_del).unwrap();
        assert!(!file_path.exists());
    }
}
