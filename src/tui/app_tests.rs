use super::*;
use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

// ── Test helpers ─────────────────────────────────────────────────

/// Build a minimal Config suitable for TUI tests (no file I/O).
fn test_config() -> Config {
    let mut cfg = Config::new_default("test-board");
    // Point dir at a temp path so InputHistory::with_path doesn't
    // interfere with the real board.
    cfg.set_dir(std::env::temp_dir().join("kbmdx-tui-test"));
    cfg
}

/// Build a sample task with the given id, title, and status.
fn make_task(id: i32, title: &str, status: &str, priority: &str) -> Task {
    let now = Utc::now();
    Task {
        id,
        title: title.to_string(),
        status: status.to_string(),
        priority: priority.to_string(),
        created: now,
        updated: now,
        started: None,
        completed: None,
        assignee: String::new(),
        tags: Vec::new(),
        due: None,
        estimate: String::new(),
        parent: None,
        depends_on: Vec::new(),
        blocked: false,
        block_reason: String::new(),
        claimed_by: String::new(),
        claimed_at: None,
        class: String::new(),
        branch: String::new(),
        worktree: String::new(),
        body: String::new(),
        file: String::new(),
    }
}

/// Build a test App with 4 tasks across 3 statuses (backlog has 0,
/// todo has 2, in-progress has 1, review has 0, done has 1).
fn test_app() -> App {
    let tasks = vec![
        make_task(1, "First task", "todo", "high"),
        make_task(2, "Second task", "todo", "medium"),
        make_task(3, "Third task", "in-progress", "low"),
        make_task(4, "Done task", "done", "medium"),
    ];
    let cfg = test_config();
    let mut app = App::new(cfg, tasks);
    app.terminal_width = 120;
    app.terminal_height = 40;
    app
}

/// Simulate a keypress.
fn send_key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent {
        code,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    });
}

/// Simulate a keypress with modifiers.
fn send_key_mod(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    app.handle_key(KeyEvent {
        code,
        modifiers: mods,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    });
}

// ── Construction tests ───────────────────────────────────────────

#[test]
fn app_new_distributes_tasks_to_columns() {
    let app = test_app();
    let names: Vec<&str> = app.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"todo"), "should have 'todo' column");
    assert!(names.contains(&"in-progress"), "should have 'in-progress' column");
    assert!(names.contains(&"done"), "should have 'done' column");

    let todo_col = app.columns.iter().find(|c| c.name == "todo").unwrap();
    assert_eq!(todo_col.tasks.len(), 2);

    let ip_col = app.columns.iter().find(|c| c.name == "in-progress").unwrap();
    assert_eq!(ip_col.tasks.len(), 1);

    let done_col = app.columns.iter().find(|c| c.name == "done").unwrap();
    assert_eq!(done_col.tasks.len(), 1);
}

#[test]
fn app_starts_on_board_view() {
    let app = test_app();
    assert_eq!(app.view, AppView::Board);
    assert!(!app.should_quit);
}

// ── Navigation tests ─────────────────────────────────────────────

#[test]
fn j_k_navigate_rows() {
    let mut app = test_app();
    // Navigate to todo column (which has 2 tasks).
    let todo_idx = app.columns.iter().position(|c| c.name == "todo").unwrap();
    app.active_col = todo_idx;
    app.active_row = 0;

    send_key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.active_row, 1, "j should move down");

    // j clamps at the bottom (no wrapping).
    send_key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.active_row, 1, "j at bottom should clamp");

    send_key(&mut app, KeyCode::Char('k'));
    assert_eq!(app.active_row, 0, "k should move up");
}

#[test]
fn h_l_navigate_columns() {
    let mut app = test_app();
    let start_col = app.active_col;

    send_key(&mut app, KeyCode::Char('l'));
    assert!(app.active_col > start_col || app.columns.len() == 1,
            "l should move right");

    send_key(&mut app, KeyCode::Char('h'));
    assert_eq!(app.active_col, start_col, "h should move back left");
}

#[test]
fn arrow_keys_navigate() {
    let mut app = test_app();
    let todo_idx = app.columns.iter().position(|c| c.name == "todo").unwrap();
    app.active_col = todo_idx;
    app.active_row = 0;

    send_key(&mut app, KeyCode::Down);
    assert_eq!(app.active_row, 1, "Down arrow should move down");

    send_key(&mut app, KeyCode::Up);
    assert_eq!(app.active_row, 0, "Up arrow should move up");
}

// ── View transition tests ────────────────────────────────────────

#[test]
fn enter_opens_detail_view() {
    let mut app = test_app();
    let todo_idx = app.columns.iter().position(|c| c.name == "todo").unwrap();
    app.active_col = todo_idx;
    app.active_row = 0;

    send_key(&mut app, KeyCode::Enter);
    assert_eq!(app.view, AppView::Detail, "Enter should open detail view");
}

#[test]
fn esc_returns_from_detail_to_board() {
    let mut app = test_app();
    let todo_idx = app.columns.iter().position(|c| c.name == "todo").unwrap();
    app.active_col = todo_idx;

    send_key(&mut app, KeyCode::Enter);
    assert_eq!(app.view, AppView::Detail);

    send_key(&mut app, KeyCode::Esc);
    assert_eq!(app.view, AppView::Board, "Esc should return to board");
}

#[test]
fn question_mark_opens_help() {
    let mut app = test_app();
    send_key(&mut app, KeyCode::Char('?'));
    assert_eq!(app.view, AppView::Help, "? should open help overlay");
}

#[test]
fn esc_closes_help() {
    let mut app = test_app();
    send_key(&mut app, KeyCode::Char('?'));
    assert_eq!(app.view, AppView::Help);

    send_key(&mut app, KeyCode::Esc);
    assert_eq!(app.view, AppView::Board, "Esc should close help");
}

#[test]
fn m_opens_move_dialog() {
    let mut app = test_app();
    let todo_idx = app.columns.iter().position(|c| c.name == "todo").unwrap();
    app.active_col = todo_idx;
    app.active_row = 0;

    send_key(&mut app, KeyCode::Char('m'));
    assert_eq!(app.view, AppView::MoveTask, "m should open move dialog");
}

#[test]
fn esc_closes_move_dialog() {
    let mut app = test_app();
    let todo_idx = app.columns.iter().position(|c| c.name == "todo").unwrap();
    app.active_col = todo_idx;

    send_key(&mut app, KeyCode::Char('m'));
    assert_eq!(app.view, AppView::MoveTask);

    send_key(&mut app, KeyCode::Esc);
    assert_eq!(app.view, AppView::Board, "Esc should close move dialog");
}

#[test]
fn d_opens_delete_confirm() {
    let mut app = test_app();
    let todo_idx = app.columns.iter().position(|c| c.name == "todo").unwrap();
    app.active_col = todo_idx;

    send_key(&mut app, KeyCode::Char('d'));
    assert_eq!(app.view, AppView::ConfirmDelete, "d should open delete confirm");
}

// ── Search tests ─────────────────────────────────────────────────

#[test]
fn slash_opens_search() {
    let mut app = test_app();
    send_key(&mut app, KeyCode::Char('/'));
    assert_eq!(app.view, AppView::Search, "/ should open search");
}

#[test]
fn esc_closes_search() {
    let mut app = test_app();
    send_key(&mut app, KeyCode::Char('/'));
    assert_eq!(app.view, AppView::Search);

    send_key(&mut app, KeyCode::Esc);
    assert_eq!(app.view, AppView::Board);
}

// ── Ctrl+C quit test ─────────────────────────────────────────────

#[test]
fn ctrl_c_quits() {
    let mut app = test_app();
    send_key_mod(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(app.should_quit, "Ctrl+C should set should_quit");
}

#[test]
fn q_quits_from_board() {
    let mut app = test_app();
    send_key(&mut app, KeyCode::Char('q'));
    assert!(app.should_quit, "q should quit from board view");
}

// ── View mode toggle ─────────────────────────────────────────────

#[test]
fn shift_v_toggles_view_mode() {
    let mut app = test_app();
    let original = app.view_mode;
    send_key(&mut app, KeyCode::Char('V'));
    assert_ne!(app.view_mode, original, "V should toggle view mode");
}

// ── Create wizard ────────────────────────────────────────────────

#[test]
fn c_opens_create_wizard() {
    let mut app = test_app();
    send_key(&mut app, KeyCode::Char('c'));
    assert_eq!(app.view, AppView::CreateTask, "c should open create wizard");
}

#[test]
fn esc_closes_create_wizard() {
    let mut app = test_app();
    send_key(&mut app, KeyCode::Char('c'));
    assert_eq!(app.view, AppView::CreateTask);

    send_key(&mut app, KeyCode::Esc);
    assert_eq!(app.view, AppView::Board, "Esc should close create wizard");
}

// ── Goto dialog ──────────────────────────────────────────────────

#[test]
fn colon_opens_goto_dialog() {
    let mut app = test_app();
    send_key(&mut app, KeyCode::Char(':'));
    assert!(app.goto_active, ": should open goto dialog");
}

#[test]
fn esc_closes_goto() {
    let mut app = test_app();
    send_key(&mut app, KeyCode::Char(':'));
    assert!(app.goto_active);

    send_key(&mut app, KeyCode::Esc);
    assert!(!app.goto_active, "Esc should close goto dialog");
}

// ── InputHistory tests ───────────────────────────────────────────

#[test]
fn input_history_push_deduplicates() {
    let dir = std::env::temp_dir().join("kbmdx-test-history");
    let _ = std::fs::create_dir_all(&dir);
    let mut h = InputHistory::with_path(dir.join("test_hist"));
    h.push("first");
    h.push("second");
    h.push("first"); // moves to end
    assert_eq!(h.entries(), &["second", "first"]);
}

#[test]
fn input_history_up_down_navigation() {
    let dir = std::env::temp_dir().join("kbmdx-test-history");
    let _ = std::fs::create_dir_all(&dir);
    let mut h = InputHistory::with_path(dir.join("test_hist2"));
    h.push("alpha");
    h.push("beta");
    h.push("gamma");

    // Up navigates to most recent.
    let val = h.up("current");
    assert_eq!(val, Some("gamma"));

    let val = h.up("current");
    assert_eq!(val, Some("beta"));

    let val = h.up("current");
    assert_eq!(val, Some("alpha"));

    // At the top, stays at first entry.
    let val = h.up("current");
    assert_eq!(val, Some("alpha"));

    // Down moves toward recent.
    let val = h.down("current");
    assert_eq!(val, Some("beta"));

    let val = h.down("current");
    assert_eq!(val, Some("gamma"));

    // Past the end, returns the draft.
    let val = h.down("current");
    assert_eq!(val, Some("current"));

    // Now we're in "not browsing" state, down returns None.
    let val = h.down("current");
    assert_eq!(val, None);
}

#[test]
fn input_history_reset() {
    let dir = std::env::temp_dir().join("kbmdx-test-history");
    let _ = std::fs::create_dir_all(&dir);
    let mut h = InputHistory::with_path(dir.join("test_hist3"));
    h.push("a");
    h.push("b");
    h.up("x");
    h.reset();
    // After reset, down should return None (not browsing).
    assert_eq!(h.down("x"), None);
}

#[test]
fn input_history_empty_entries_ignored() {
    let dir = std::env::temp_dir().join("kbmdx-test-history");
    let _ = std::fs::create_dir_all(&dir);
    let mut h = InputHistory::with_path(dir.join("test_hist4"));
    h.push("");
    h.push("   ");
    assert!(h.entries().is_empty());
}

#[test]
fn input_history_completions() {
    let dir = std::env::temp_dir().join("kbmdx-test-history");
    let _ = std::fs::create_dir_all(&dir);
    let mut h = InputHistory::with_path(dir.join("test_hist5"));
    h.push("priority:high");
    h.push("priority:low");
    h.push("status:todo");

    let completions = h.completions("pri");
    assert_eq!(completions, vec!["priority:high", "priority:low"]);

    let completions = h.completions("stat");
    assert_eq!(completions, vec!["status:todo"]);

    let completions = h.completions("");
    assert!(completions.is_empty());
}

// ── Sort mode cycling ────────────────────────────────────────────

#[test]
fn s_cycles_sort_mode() {
    let mut app = test_app();
    let initial = app.sort_mode;
    send_key(&mut app, KeyCode::Char('s'));
    assert_ne!(app.sort_mode, initial, "s should cycle sort mode");
}

// ── All views reachable and dismissible ──────────────────────────

#[test]
fn all_dialog_views_dismissible_with_esc() {
    let mut app = test_app();
    let todo_idx = app.columns.iter().position(|c| c.name == "todo").unwrap();
    app.active_col = todo_idx;

    // Detail
    send_key(&mut app, KeyCode::Enter);
    assert_eq!(app.view, AppView::Detail);
    send_key(&mut app, KeyCode::Esc);
    assert_eq!(app.view, AppView::Board);

    // Help
    send_key(&mut app, KeyCode::Char('?'));
    assert_eq!(app.view, AppView::Help);
    send_key(&mut app, KeyCode::Esc);
    assert_eq!(app.view, AppView::Board);

    // Search
    send_key(&mut app, KeyCode::Char('/'));
    assert_eq!(app.view, AppView::Search);
    send_key(&mut app, KeyCode::Esc);
    assert_eq!(app.view, AppView::Board);

    // Move
    send_key(&mut app, KeyCode::Char('m'));
    assert_eq!(app.view, AppView::MoveTask);
    send_key(&mut app, KeyCode::Esc);
    assert_eq!(app.view, AppView::Board);

    // Delete
    send_key(&mut app, KeyCode::Char('d'));
    assert_eq!(app.view, AppView::ConfirmDelete);
    send_key(&mut app, KeyCode::Esc);
    assert_eq!(app.view, AppView::Board);

    // Create
    send_key(&mut app, KeyCode::Char('c'));
    assert_eq!(app.view, AppView::CreateTask);
    send_key(&mut app, KeyCode::Esc);
    assert_eq!(app.view, AppView::Board);

    // Goto
    send_key(&mut app, KeyCode::Char(':'));
    assert!(app.goto_active);
    send_key(&mut app, KeyCode::Esc);
    assert!(!app.goto_active);
}

// ── Debug/perf mode ─────────────────────────────────────────────

#[test]
fn f12_toggles_perf_mode() {
    let mut app = test_app();
    let initial = app.debug.perf_mode;
    send_key(&mut app, KeyCode::F(12));
    assert_ne!(app.debug.perf_mode, initial, "F12 should toggle perf mode");
    send_key(&mut app, KeyCode::F(12));
    assert_eq!(app.debug.perf_mode, initial, "F12 twice should restore state");
}
