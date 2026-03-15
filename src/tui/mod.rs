pub mod app;
mod types;
mod jump;
mod board_nav;
mod context;
mod detail_nav;
mod semantic;
mod persistence;
mod actions;
mod file_picker;
mod keys;
pub mod guide;
pub mod render;
pub mod search;
pub mod theme;

use std::io;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use color_eyre::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::prelude::*;

use crate::model::config::Config;
use crate::model::task::Task;

/// How long to block on terminal event polling before checking for
/// file-system watcher notifications.
const EVENT_POLL_MS: u64 = 100;

/// Run the TUI application with the given config and tasks.
pub fn run_tui(cfg: Config, tasks: Vec<Task>) -> Result<()> {
    // Setup terminal.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    // Create app.
    let mut app = app::App::new(cfg, tasks);

    // Set initial terminal size.
    let size = terminal.size()?;
    app.terminal_width = size.width;
    app.terminal_height = size.height;
    if app.terminal_width >= 60 {
        app.reader_open = true;
    }

    // Setup file watcher for auto-reload.
    let (fs_tx, fs_rx) = mpsc::channel::<()>();
    let tasks_path = app.cfg.tasks_path();
    let _watcher = setup_file_watcher(tasks_path, fs_tx);

    // Event loop with polling to handle both terminal events and file changes.
    loop {
        // Auto-clear ephemeral status messages after 2 seconds.
        app.expire_status(Duration::from_secs(2));

        // In perf mode, only redraw when state has changed.
        // In non-perf mode, redraw unconditionally (for A/B comparison).
        if app.debug.needs_redraw || !app.debug.perf_mode {
            terminal.draw(|frame| render::render(&mut app, frame))?;
            app.update_fps();
            app.debug.needs_redraw = false;
        }

        // Tick semantic search debounce timer and check for async results.
        app.tick_semantic_debounce();

        // Check for file system changes (non-blocking).
        if fs_rx.try_recv().is_ok() {
            // Drain any additional pending events.
            while fs_rx.try_recv().is_ok() {}
            app.reload_tasks();
        }

        // Poll for terminal events with a short timeout so we can check fs events.
        if event::poll(Duration::from_millis(EVENT_POLL_MS))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        app.handle_key(key);
                    }
                }
                Event::Mouse(mouse) => {
                    app.handle_mouse(mouse);
                }
                Event::Resize(w, h) => {
                    app.terminal_width = w;
                    app.terminal_height = h;
                    app.debug.needs_redraw = true;
                }
                _ => {}
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal.
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

/// Run the TUI in standalone file-reader mode (no board, no config).
pub fn run_tui_reader(path: std::path::PathBuf, title: String, body: String) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let mut app = app::App::new_file_reader(
        path.display().to_string(),
        title,
        body,
    );

    let size = terminal.size()?;
    app.terminal_width = size.width;
    app.terminal_height = size.height;

    // Watch the markdown file for live reload.
    let (fs_tx, fs_rx) = mpsc::channel::<()>();
    let _watcher = setup_file_watcher_single(path.clone(), fs_tx);

    loop {
        app.expire_status(Duration::from_secs(2));

        if app.debug.needs_redraw || !app.debug.perf_mode {
            terminal.draw(|frame| render::render(&mut app, frame))?;
            app.update_fps();
            app.debug.needs_redraw = false;
        }

        // Check for file changes (live reload).
        if fs_rx.try_recv().is_ok() {
            while fs_rx.try_recv().is_ok() {}
            if let Ok(new_body) = std::fs::read_to_string(&path) {
                if let Some(ref mut fv) = app.file_view {
                    fv.body = new_body;
                }
                app.detail.cache = None;
                app.detail.heading_cache = None;
                app.debug.needs_redraw = true;
            }
        }

        if event::poll(Duration::from_millis(EVENT_POLL_MS))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        app.handle_key(key);
                    }
                }
                Event::Mouse(mouse) => {
                    app.handle_mouse(mouse);
                }
                Event::Resize(w, h) => {
                    app.terminal_width = w;
                    app.terminal_height = h;
                    app.debug.needs_redraw = true;
                }
                _ => {}
            }
        }

        if app.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

/// Setup a file system watcher on a single file.
fn setup_file_watcher_single(
    file_path: std::path::PathBuf,
    tx: mpsc::Sender<()>,
) -> Option<RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            use notify::EventKind;
            if matches!(event.kind, EventKind::Modify(_)) {
                let _ = tx.send(());
            }
        }
    })
    .ok()?;

    // Watch the parent directory (some editors do atomic rename).
    let parent = file_path.parent().unwrap_or(&file_path);
    watcher
        .watch(parent, RecursiveMode::NonRecursive)
        .ok()?;

    Some(watcher)
}

/// Setup a file system watcher on the tasks directory.
/// Returns the watcher (must be kept alive) or None if setup fails.
fn setup_file_watcher(
    tasks_path: PathBuf,
    tx: mpsc::Sender<()>,
) -> Option<RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            // Only trigger reload for file modifications and creations.
            use notify::EventKind;
            match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                    let _ = tx.send(());
                }
                _ => {}
            }
        }
    })
    .ok()?;

    watcher
        .watch(&tasks_path, RecursiveMode::NonRecursive)
        .ok()?;

    Some(watcher)
}
