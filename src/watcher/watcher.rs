//! File watcher implementation using the `notify` crate.
//!
//! Watches one or more directories for meaningful file system events
//! (create, modify, remove, rename) and delivers debounced [`WatchEvent`]
//! notifications through a [`std::sync::mpsc::Receiver`].
//!
//! Temporary files, swap files, and backup files are silently ignored so that
//! editors saving via atomic-rename do not produce spurious reloads.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher as NotifyWatcher};

/// Debounce window -- events arriving within this window after the first event
/// are coalesced into a single [`WatchEvent::Reload`].
const DEBOUNCE_DELAY: Duration = Duration::from_millis(100);

// ── Events ───────────────────────────────────────────────────────────

/// Events emitted by the [`Watcher`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    /// One or more files in the watched directories changed.
    /// The watcher does not distinguish between create/modify/delete --
    /// consumers should simply reload the board.
    Reload,
}

// ── Watcher ──────────────────────────────────────────────────────────

/// Watches directories for file changes and sends debounced reload events.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use kanban_mdx::watcher::Watcher;
///
/// let watcher = Watcher::new(Path::new(".kbmdx/tasks")).unwrap();
/// // In your event loop:
/// if let Ok(event) = watcher.events().try_recv() {
///     println!("Board changed: {:?}", event);
/// }
/// // Stop watching:
/// watcher.stop();
/// ```
pub struct Watcher {
    /// The underlying notify watcher. Kept alive so watches remain registered.
    /// Wrapped in Option so we can drop it in `stop()`.
    _watcher: Option<RecommendedWatcher>,

    /// Receiver end of the debounced event channel.
    rx: Receiver<WatchEvent>,

    /// Shared debounce state -- the timer thread checks this to know whether
    /// to actually send the event or if a newer timer superseded it.
    debounce_state: Arc<Mutex<DebounceState>>,
}

/// Internal state for debouncing.
struct DebounceState {
    /// Monotonically increasing generation counter. Each raw event bumps this;
    /// the timer thread only fires if the generation has not changed since it
    /// was spawned.
    generation: u64,

    /// Sender for debounced events.
    tx: Sender<WatchEvent>,
}

impl Watcher {
    /// Creates a new watcher monitoring the given directory (non-recursively).
    ///
    /// Returns an error if the directory does not exist or cannot be watched.
    pub fn new(tasks_dir: &Path) -> color_eyre::Result<Self> {
        Self::new_multi(&[tasks_dir.to_path_buf()])
    }

    /// Creates a new watcher monitoring multiple directories.
    ///
    /// Returns an error if any directory does not exist or cannot be watched.
    pub fn new_multi(paths: &[PathBuf]) -> color_eyre::Result<Self> {
        let (tx, rx) = mpsc::channel();

        let debounce_state = Arc::new(Mutex::new(DebounceState {
            generation: 0,
            tx,
        }));

        let state_for_handler = Arc::clone(&debounce_state);

        let mut notify_watcher = notify::recommended_watcher(
            move |res: NotifyResult<Event>| {
                if let Ok(event) = res {
                    if is_meaningful(&event) && !is_ignored_path(&event) {
                        schedule_debounce(&state_for_handler);
                    }
                }
            },
        )?;

        for path in paths {
            notify_watcher.watch(path, RecursiveMode::NonRecursive)?;
        }

        Ok(Watcher {
            _watcher: Some(notify_watcher),
            rx,
            debounce_state,
        })
    }

    /// Returns a reference to the event receiver.
    ///
    /// Use `recv()` (blocking) or `try_recv()` (non-blocking) to consume
    /// events in your event loop.
    pub fn events(&self) -> &Receiver<WatchEvent> {
        &self.rx
    }

    /// Stops the watcher and releases all OS resources.
    ///
    /// After calling `stop()`, the event receiver will eventually return
    /// `RecvError` / `TryRecvError::Disconnected`.
    pub fn stop(&self) {
        // Drop the sender so the receiver sees a disconnect.
        // We can't move out of &self, so we just drop the generation's tx
        // by replacing it. The notify watcher will be dropped when the
        // Watcher struct itself is dropped.
        if let Ok(mut state) = self.debounce_state.lock() {
            // Create a dead-end channel -- the new tx is immediately dropped.
            let (dead_tx, _dead_rx) = mpsc::channel();
            state.tx = dead_tx;
        }
    }
}

// ── Helper functions ─────────────────────────────────────────────────

/// Returns `true` if the event represents a meaningful file change
/// (create, modify, remove, or rename). Ignores access and metadata-only
/// events (e.g. chmod).
fn is_meaningful(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_)
            | EventKind::Modify(_)
            | EventKind::Remove(_)
    )
}

/// Returns `true` if any path in the event looks like a temporary, swap,
/// or backup file that should be ignored.
///
/// Patterns filtered:
/// - Vim swap files: `.file.swp`, `.file.swo`, `.file.swx`, `file~`
/// - Emacs backup/lock files: `#file#`, `.#file`
/// - macOS metadata: `.DS_Store`, `._*`
/// - Generic temp files: `*.tmp`, `*.bak`
fn is_ignored_path(event: &Event) -> bool {
    event.paths.iter().any(|p| {
        let name = match p.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => return false,
        };

        // Vim swap files
        if name.ends_with(".swp") || name.ends_with(".swo") || name.ends_with(".swx") {
            return true;
        }
        // Vim/nano backup
        if name.ends_with('~') {
            return true;
        }
        // Emacs lock files
        if name.starts_with(".#") {
            return true;
        }
        // Emacs backup files
        if name.starts_with('#') && name.ends_with('#') {
            return true;
        }
        // macOS metadata
        if name == ".DS_Store" || name.starts_with("._") {
            return true;
        }
        // Generic temp/backup
        if name.ends_with(".tmp") || name.ends_with(".bak") {
            return true;
        }

        false
    })
}

/// Bumps the generation counter and spawns a timer thread. When the timer
/// fires, it only sends a `Reload` event if no newer generation has been
/// scheduled (i.e. no more events arrived during the debounce window).
fn schedule_debounce(state: &Arc<Mutex<DebounceState>>) {
    let gen = {
        let mut s = state.lock().expect("debounce lock poisoned");
        s.generation += 1;
        s.generation
    };

    let state = Arc::clone(state);
    std::thread::spawn(move || {
        std::thread::sleep(DEBOUNCE_DELAY);
        let s = state.lock().expect("debounce lock poisoned");
        if s.generation == gen {
            // No newer events arrived -- fire the reload.
            let _ = s.tx.send(WatchEvent::Reload);
        }
    });
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;

    /// Helper: wait for a reload event with a timeout.
    fn wait_for_reload(watcher: &Watcher, timeout: Duration) -> bool {
        watcher
            .events()
            .recv_timeout(timeout)
            .map(|e| e == WatchEvent::Reload)
            .unwrap_or(false)
    }

    #[test]
    fn detects_file_create() {
        let dir = TempDir::new().unwrap();
        let w = Watcher::new(dir.path()).unwrap();

        // Give the OS watcher time to register.
        thread::sleep(Duration::from_millis(50));

        fs::write(dir.path().join("test.md"), "hello").unwrap();

        assert!(
            wait_for_reload(&w, Duration::from_millis(500)),
            "expected Reload event after file create"
        );
    }

    #[test]
    fn detects_file_modify() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.md");
        fs::write(&path, "v1").unwrap();

        let w = Watcher::new(dir.path()).unwrap();
        thread::sleep(Duration::from_millis(50));

        fs::write(&path, "v2").unwrap();

        assert!(
            wait_for_reload(&w, Duration::from_millis(500)),
            "expected Reload event after file modify"
        );
    }

    #[test]
    fn detects_file_delete() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.md");
        fs::write(&path, "data").unwrap();

        let w = Watcher::new(dir.path()).unwrap();
        thread::sleep(Duration::from_millis(50));

        fs::remove_file(&path).unwrap();

        assert!(
            wait_for_reload(&w, Duration::from_millis(500)),
            "expected Reload event after file delete"
        );
    }

    #[test]
    fn debounces_batch_changes() {
        let dir = TempDir::new().unwrap();
        let w = Watcher::new(dir.path()).unwrap();
        thread::sleep(Duration::from_millis(50));

        // Create 5 files in rapid succession.
        for i in 0..5 {
            fs::write(dir.path().join(format!("task{i}.md")), "data").unwrap();
        }

        // Wait for debounce to settle.
        thread::sleep(Duration::from_millis(400));

        // Drain events -- should be a small number (ideally 1).
        let mut count = 0;
        while w.events().try_recv().is_ok() {
            count += 1;
        }

        // At least 1 event, but debouncing should keep it low.
        assert!(count >= 1, "expected at least 1 event, got {count}");
        assert!(count <= 3, "expected debouncing to reduce events, got {count}");
    }

    #[test]
    fn error_on_invalid_path() {
        let result = Watcher::new(Path::new("/nonexistent/path/abc123"));
        assert!(result.is_err(), "expected error for invalid path");
    }

    #[test]
    fn stop_disconnects_receiver() {
        let dir = TempDir::new().unwrap();
        let w = Watcher::new(dir.path()).unwrap();

        w.stop();

        // After stop, further receives should eventually fail.
        // The existing sender is replaced, so try_recv should return
        // Disconnected once the old sender is dropped.
        thread::sleep(Duration::from_millis(50));

        // Create a file -- should NOT produce events after stop.
        fs::write(dir.path().join("after_stop.md"), "data").unwrap();
        thread::sleep(Duration::from_millis(200));

        // No events should arrive.
        assert!(
            w.events().try_recv().is_err(),
            "expected no events after stop"
        );
    }

    #[test]
    fn multiple_directories() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();
        let w = Watcher::new_multi(&[dir1.path().to_path_buf(), dir2.path().to_path_buf()]).unwrap();
        thread::sleep(Duration::from_millis(50));

        // Change in first dir.
        fs::write(dir1.path().join("a.md"), "a").unwrap();
        assert!(
            wait_for_reload(&w, Duration::from_millis(500)),
            "expected Reload from dir1 change"
        );

        // Wait for debounce to fully settle before next test.
        thread::sleep(Duration::from_millis(200));

        // Change in second dir.
        fs::write(dir2.path().join("b.md"), "b").unwrap();
        assert!(
            wait_for_reload(&w, Duration::from_millis(500)),
            "expected Reload from dir2 change"
        );
    }

    #[test]
    fn ignores_swap_files() {
        let dir = TempDir::new().unwrap();
        let w = Watcher::new(dir.path()).unwrap();
        thread::sleep(Duration::from_millis(50));

        // Create files that should be ignored.
        fs::write(dir.path().join(".test.swp"), "swap").unwrap();
        fs::write(dir.path().join("test.md~"), "backup").unwrap();
        fs::write(dir.path().join(".DS_Store"), "meta").unwrap();
        fs::write(dir.path().join("test.tmp"), "temp").unwrap();

        thread::sleep(Duration::from_millis(300));

        // Should have no events.
        assert!(
            w.events().try_recv().is_err(),
            "expected swap/temp files to be ignored"
        );
    }

    #[test]
    fn ignores_emacs_files() {
        let dir = TempDir::new().unwrap();
        let w = Watcher::new(dir.path()).unwrap();
        thread::sleep(Duration::from_millis(50));

        fs::write(dir.path().join(".#lockfile"), "lock").unwrap();
        fs::write(dir.path().join("#autosave#"), "auto").unwrap();

        thread::sleep(Duration::from_millis(300));

        assert!(
            w.events().try_recv().is_err(),
            "expected emacs lock/backup files to be ignored"
        );
    }

    // ── Unit tests for helper functions ──────────────────────────────

    #[test]
    fn is_meaningful_create() {
        let event = Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![],
            attrs: Default::default(),
        };
        assert!(is_meaningful(&event));
    }

    #[test]
    fn is_meaningful_modify() {
        let event = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![],
            attrs: Default::default(),
        };
        assert!(is_meaningful(&event));
    }

    #[test]
    fn is_meaningful_remove() {
        let event = Event {
            kind: EventKind::Remove(notify::event::RemoveKind::File),
            paths: vec![],
            attrs: Default::default(),
        };
        assert!(is_meaningful(&event));
    }

    #[test]
    fn is_meaningful_access_ignored() {
        let event = Event {
            kind: EventKind::Access(notify::event::AccessKind::Read),
            paths: vec![],
            attrs: Default::default(),
        };
        assert!(!is_meaningful(&event));
    }

    #[test]
    fn is_ignored_vim_swap() {
        let event = Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/tmp/.task.swp")],
            attrs: Default::default(),
        };
        assert!(is_ignored_path(&event));
    }

    #[test]
    fn is_ignored_backup_tilde() {
        let event = Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/tmp/task.md~")],
            attrs: Default::default(),
        };
        assert!(is_ignored_path(&event));
    }

    #[test]
    fn is_not_ignored_normal_md() {
        let event = Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/tmp/task.md")],
            attrs: Default::default(),
        };
        assert!(!is_ignored_path(&event));
    }

    #[test]
    fn is_ignored_ds_store() {
        let event = Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/tmp/.DS_Store")],
            attrs: Default::default(),
        };
        assert!(is_ignored_path(&event));
    }

    #[test]
    fn is_ignored_dot_underscore() {
        let event = Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/tmp/._resource")],
            attrs: Default::default(),
        };
        assert!(is_ignored_path(&event));
    }

    #[test]
    fn is_ignored_emacs_lock() {
        let event = Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/tmp/.#lockfile")],
            attrs: Default::default(),
        };
        assert!(is_ignored_path(&event));
    }

    #[test]
    fn is_ignored_emacs_autosave() {
        let event = Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/tmp/#autosave#")],
            attrs: Default::default(),
        };
        assert!(is_ignored_path(&event));
    }

    #[test]
    fn is_ignored_tmp_file() {
        let event = Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/tmp/tempfile.tmp")],
            attrs: Default::default(),
        };
        assert!(is_ignored_path(&event));
    }

    #[test]
    fn is_ignored_bak_file() {
        let event = Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/tmp/config.bak")],
            attrs: Default::default(),
        };
        assert!(is_ignored_path(&event));
    }

    #[test]
    fn watch_event_debug() {
        // Ensure WatchEvent derives are functional.
        let e = WatchEvent::Reload;
        assert_eq!(e, WatchEvent::Reload);
        assert_eq!(format!("{e:?}"), "Reload");
        let e2 = e.clone();
        assert_eq!(e2, WatchEvent::Reload);
    }
}
