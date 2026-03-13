use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{delete_word_back, App, AppView};

impl App {
    // ── Search View ─────────────────────────────────────────────────

    pub(crate) fn handle_search_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('r') => {
                    let current = self.search.query.clone();
                    if let Some(entry) = self.search.history.up(&current) {
                        self.search.query = entry.to_string();
                        self.on_search_query_changed();
                    }
                    return;
                }
                KeyCode::Char('w') => {
                    // Ctrl+W: delete last word (Unix convention).
                    delete_word_back(&mut self.search.query);
                    self.on_search_query_changed();
                    self.search.history.reset();
                    self.search.tab_prefix = None;
                    self.search.tab_idx = 0;
                    return;
                }
                KeyCode::Char('u') => {
                    // Ctrl+U: delete entire line (Unix convention).
                    self.search.query.clear();
                    self.on_search_query_changed();
                    self.search.history.reset();
                    self.search.tab_prefix = None;
                    self.search.tab_idx = 0;
                    return;
                }
                KeyCode::Char('n') => {
                    // Ctrl+N: jump to next matching task (stay in search).
                    self.search_next_match();
                    return;
                }
                KeyCode::Char('p') => {
                    // Ctrl+P: jump to previous matching task (stay in search).
                    self.search_prev_match();
                    return;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Backspace => {
                if key.modifiers.contains(KeyModifiers::SUPER) {
                    // Cmd+Backspace: delete entire query.
                    self.search.query.clear();
                } else if key.modifiers.contains(KeyModifiers::ALT) {
                    // Alt+Backspace: delete last word.
                    delete_word_back(&mut self.search.query);
                } else {
                    self.search.query.pop();
                }
                self.on_search_query_changed();
                self.search.history.reset();
                self.search.tab_prefix = None;
                self.search.tab_idx = 0;
            }
            KeyCode::Enter => {
                if !self.search.query.is_empty() {
                    self.search.history.push(&self.search.query.clone());
                }
                self.search.history.reset();
                self.search.tab_prefix = None;
                self.search.tab_idx = 0;
                self.view = AppView::Board;
            }
            KeyCode::Esc => {
                self.search.query.clear();
                self.clear_sem_state();
                self.search.history.reset();
                self.search.tab_prefix = None;
                self.search.tab_idx = 0;
                self.view = AppView::Board;
            }
            KeyCode::Up => {
                let current = self.search.query.clone();
                if let Some(entry) = self.search.history.up(&current) {
                    self.search.query = entry.to_string();
                    self.on_search_query_changed();
                }
                self.search.tab_prefix = None;
                self.search.tab_idx = 0;
            }
            KeyCode::Down => {
                let current = self.search.query.clone();
                if let Some(entry) = self.search.history.down(&current) {
                    self.search.query = entry.to_string();
                    self.on_search_query_changed();
                }
                self.search.tab_prefix = None;
                self.search.tab_idx = 0;
            }
            KeyCode::Tab => {
                let prefix = self
                    .search.tab_prefix
                    .get_or_insert_with(|| self.search.query.clone())
                    .clone();
                let completions = self.search.history.completions(&prefix);
                if !completions.is_empty() {
                    let rev_idx = self.search.tab_idx % completions.len();
                    let entry = completions[completions.len() - 1 - rev_idx];
                    self.search.query = entry.to_string();
                    self.on_search_query_changed();
                    self.search.tab_idx += 1;
                }
            }
            KeyCode::Char('?') if self.search.query.is_empty() => {
                self.search_help_return = AppView::Search;
                self.search_help_scroll = 0;
                self.view = AppView::SearchHelp;
            }
            KeyCode::Char(c) => {
                self.search.query.push(c);
                self.on_search_query_changed();
                self.search.history.reset();
                self.search.tab_prefix = None;
                self.search.tab_idx = 0;
            }
            _ => {}
        }
    }

    /// Jump to the next matching task on the board (used by Ctrl+N in search).
    /// Wraps around from last match back to first.
    fn search_next_match(&mut self) {
        if self.search.query.is_empty() {
            return;
        }
        let query = self.search.query.clone();
        let wt = self.worktree_filter_active;
        let sem = self.search.sem_scores.clone();
        let ctx = self.compute_context_ids();
        let tm = self.time_mode.label();

        // Collect (col_index, unfiltered_row_index) for every matching task,
        // ordered by column then row.
        let mut matches: Vec<(usize, usize)> = Vec::new();
        for (ci, col) in self.columns.iter().enumerate() {
            if col.collapsed {
                continue;
            }
            let filtered = App::filtered_tasks(col, &query, wt, &sem, &ctx, tm);
            for task in &filtered {
                // Find this task's index in the unfiltered column.
                if let Some(ri) = col.tasks.iter().position(|t| t.id == task.id) {
                    matches.push((ci, ri));
                }
            }
        }

        if matches.is_empty() {
            return;
        }

        // Find the first match strictly after the current cursor position.
        let current = (self.active_col, self.active_row);
        let next = matches
            .iter()
            .find(|&&(c, r)| (c, r) > current)
            .or_else(|| matches.first()); // wrap around
        if let Some(&(col, row)) = next {
            self.active_col = col;
            self.active_row = row;
        }
    }

    /// Jump to the previous matching task on the board (used by Ctrl+P in search).
    /// Wraps around from first match back to last.
    fn search_prev_match(&mut self) {
        if self.search.query.is_empty() {
            return;
        }
        let query = self.search.query.clone();
        let wt = self.worktree_filter_active;
        let sem = self.search.sem_scores.clone();
        let ctx = self.compute_context_ids();
        let tm = self.time_mode.label();

        // Collect (col_index, unfiltered_row_index) for every matching task,
        // ordered by column then row.
        let mut matches: Vec<(usize, usize)> = Vec::new();
        for (ci, col) in self.columns.iter().enumerate() {
            if col.collapsed {
                continue;
            }
            let filtered = App::filtered_tasks(col, &query, wt, &sem, &ctx, tm);
            for task in &filtered {
                if let Some(ri) = col.tasks.iter().position(|t| t.id == task.id) {
                    matches.push((ci, ri));
                }
            }
        }

        if matches.is_empty() {
            return;
        }

        // Find the last match strictly before the current cursor position.
        let current = (self.active_col, self.active_row);
        let prev = matches
            .iter()
            .rfind(|&&(c, r)| (c, r) < current)
            .or_else(|| matches.last()); // wrap around
        if let Some(&(col, row)) = prev {
            self.active_col = col;
            self.active_row = row;
        }
    }
}
