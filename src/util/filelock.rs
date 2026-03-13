//! File-based locking for concurrent access to the board.

use std::path::Path;

/// A file lock guard. The lock is released when this is dropped.
pub struct FileLock {
    _file: fslock::LockFile,
}

/// Acquire an exclusive lock on the kanban directory.
/// Creates a `.lock` file in the directory.
pub fn lock(kanban_dir: &Path) -> Result<FileLock, Box<dyn std::error::Error>> {
    let lock_path = kanban_dir.join(".lock");
    let mut file = fslock::LockFile::open(&lock_path)?;
    file.lock()?;
    Ok(FileLock { _file: file })
}

/// Try to acquire a lock without blocking. Returns None if already locked.
pub fn try_lock(kanban_dir: &Path) -> Result<Option<FileLock>, Box<dyn std::error::Error>> {
    let lock_path = kanban_dir.join(".lock");
    let mut file = fslock::LockFile::open(&lock_path)?;
    if file.try_lock()? {
        Ok(Some(FileLock { _file: file }))
    } else {
        Ok(None)
    }
}
