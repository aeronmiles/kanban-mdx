//! Debounced file system watching for kanban board directories.
//!
//! Monitors task directories for file changes (create, modify, delete, rename)
//! and sends [`WatchEvent`] notifications through a channel. Rapid changes are
//! debounced so that batch operations (e.g. moving multiple tasks) produce a
//! single reload event.
//!
//! Ported from the Go `internal/watcher` package.

mod watcher;

pub use watcher::{WatchEvent, Watcher};
