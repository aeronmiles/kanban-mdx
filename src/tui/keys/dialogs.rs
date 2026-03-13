use crossterm::event::{KeyCode, KeyEvent};

use crate::tui::app::{App, AppView};

impl App {
    // ── MoveTask View ───────────────────────────────────────────────

    pub(crate) fn handle_move_task_key(&mut self, key: KeyEvent) {
        let col_count = self.columns.len();
        if col_count == 0 {
            self.view = AppView::Board;
            return;
        }

        if self.picker.move_filter_active {
            match key.code {
                KeyCode::Esc => {
                    self.picker.move_filter.clear();
                    self.picker.move_filter_active = false;
                    self.picker.move_cursor = 0;
                }
                KeyCode::Enter => {
                    let filtered = self.filtered_columns();
                    if let Some(&idx) = filtered.get(self.picker.move_cursor) {
                        self.execute_move(idx);
                    }
                    self.picker.move_filter.clear();
                    self.picker.move_filter_active = false;
                    self.view = AppView::Board;
                }
                KeyCode::Backspace => {
                    self.picker.move_filter.pop();
                    self.picker.move_cursor = 0;
                }
                KeyCode::Down | KeyCode::Char(']') => {
                    let filtered = self.filtered_columns();
                    if self.picker.move_cursor + 1 < filtered.len() {
                        self.picker.move_cursor += 1;
                    }
                }
                KeyCode::Up | KeyCode::Char('[') => {
                    self.picker.move_cursor = self.picker.move_cursor.saturating_sub(1);
                }
                KeyCode::Char(c) => {
                    self.picker.move_filter.push(c);
                    self.picker.move_cursor = 0;
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('/') => {
                self.picker.move_filter_active = true;
                self.picker.move_filter.clear();
                self.picker.move_cursor = 0;
            }
            KeyCode::Char('j') | KeyCode::Char(']') | KeyCode::Down => {
                if self.picker.move_cursor + 1 < col_count {
                    self.picker.move_cursor += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Char('[') | KeyCode::Up => {
                self.picker.move_cursor = self.picker.move_cursor.saturating_sub(1);
            }
            KeyCode::Enter => {
                self.execute_move(self.picker.move_cursor);
                self.view = AppView::Board;
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.picker.move_filter.clear();
                self.picker.move_filter_active = false;
                self.view = AppView::Board;
            }
            KeyCode::Char(c @ '1'..='9') => {
                let target = (c as usize) - ('1' as usize);
                if target < col_count {
                    self.execute_move(target);
                }
                self.view = AppView::Board;
            }
            KeyCode::Char(c) if c.is_alphabetic() => {
                let lower = c.to_lowercase().next().unwrap_or(c);
                if let Some(idx) = self
                    .columns
                    .iter()
                    .position(|col| col.name.to_lowercase().starts_with(lower))
                {
                    self.picker.move_cursor = idx;
                }
            }
            _ => {}
        }
    }

    // ── ConfirmDelete View ──────────────────────────────────────────

    pub(crate) fn handle_confirm_delete_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => {
                self.picker.delete_cursor = 0;
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.picker.delete_cursor = 1;
            }
            KeyCode::Char('y') => {
                self.execute_delete();
                self.view = AppView::Board;
            }
            KeyCode::Char('n') | KeyCode::Esc | KeyCode::Char('q') => {
                self.view = AppView::Board;
            }
            KeyCode::Enter => {
                if self.picker.delete_cursor == 0 {
                    self.execute_delete();
                }
                self.view = AppView::Board;
            }
            _ => {}
        }
    }

    // ── GoToTask overlay (#34) ─────────────────────────────────────

    pub(crate) fn handle_goto_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.goto_active = false;
            }
            KeyCode::Backspace => {
                self.goto_input.pop();
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                self.goto_input.push(c);
            }
            KeyCode::Enter => {
                self.goto_active = false;
                if let Ok(id) = self.goto_input.parse::<i32>() {
                    // Snapshot current position BEFORE moving the cursor
                    // so back navigation can return here.
                    self.push_jump();
                    let mut found = false;
                    for (col_idx, col) in self.columns.iter().enumerate() {
                        if let Some(row_idx) = col.tasks.iter().position(|t| t.id == id) {
                            self.active_col = col_idx;
                            self.active_row = row_idx;
                            found = true;
                            break;
                        }
                    }
                    if found {
                        // Preserve board layout: if exactly one column was
                        // solo'd, switch the solo to the target column;
                        // otherwise just ensure the target column is visible.
                        let expanded_count =
                            self.columns.iter().filter(|c| !c.collapsed).count();
                        if expanded_count <= 1 {
                            for (i, col) in self.columns.iter_mut().enumerate() {
                                col.collapsed = i != self.active_col;
                            }
                        } else {
                            self.columns[self.active_col].collapsed = false;
                        }
                        self.persist_collapsed();
                        self.search.query.clear();
                        self.clear_sem_state();
                        // Stay in current view context: board stays board,
                        // detail reloads the new task in the reader.
                        if self.view == AppView::Detail {
                            self.detail.scroll = 0;
                        }
                    } else {
                        self.set_status(format!("Task #{} not found", id));
                    }
                } else {
                    self.set_status("Invalid task ID");
                }
            }
            _ => {}
        }
    }
}
