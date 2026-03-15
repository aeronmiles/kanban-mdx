use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use chrono::Utc;

use crate::tui::app::{delete_word_back, App, AppView};

impl App {
    // ── Help View ───────────────────────────────────────────────────

    pub(crate) fn handle_help_key(&mut self, key: KeyEvent) {
        if self.help_filter_active {
            // Ctrl modifiers first (prevents Ctrl+W from typing 'w').
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('w') => {
                        delete_word_back(&mut self.help_filter);
                        self.help_scroll = 0;
                    }
                    KeyCode::Char('u') => {
                        self.help_filter.clear();
                        self.help_scroll = 0;
                    }
                    _ => {}
                }
                return;
            }
            // Filter input mode.
            match key.code {
                KeyCode::Esc => {
                    self.help_filter.clear();
                    self.help_filter_active = false;
                    self.help_scroll = 0;
                }
                KeyCode::Enter => {
                    self.help_filter_active = false;
                }
                KeyCode::Backspace => {
                    if key.modifiers.contains(KeyModifiers::SUPER) {
                        self.help_filter.clear();
                    } else if key.modifiers.contains(KeyModifiers::ALT) {
                        delete_word_back(&mut self.help_filter);
                    } else {
                        self.help_filter.pop();
                    }
                    self.help_scroll = 0;
                }
                KeyCode::Char(c) => {
                    self.help_filter.push(c);
                    self.help_scroll = 0;
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => {
                self.help_filter.clear();
                self.help_filter_active = false;
                self.help_scroll = 0;
                self.view = AppView::Board;
            }
            KeyCode::Char('/') => {
                self.help_filter_active = true;
            }
            KeyCode::Char('j') | KeyCode::Char(']') | KeyCode::Down => {
                self.help_scroll += 1;
            }
            KeyCode::Char('k') | KeyCode::Char('[') | KeyCode::Up => {
                self.help_scroll = self.help_scroll.saturating_sub(1);
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.help_scroll = 0;
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.help_scroll = usize::MAX / 2;
            }
            KeyCode::Char('O') => {
                self.help_filter.clear();
                self.help_filter_active = false;
                self.help_scroll = 0;
                self.open_file_picker();
            }
            KeyCode::Char(c @ '1'..='9') => {
                let target = (c as usize) - ('1' as usize);
                self.help_filter.clear();
                self.help_filter_active = false;
                self.help_scroll = 0;
                self.focus_column_and_return(target);
            }
            _ => {}
        }
    }

    // ── Search DSL Help ──────────────────────────────────────────────

    pub(crate) fn handle_search_help_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                self.search_help_scroll = 0;
                self.view = self.search_help_return;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.search_help_scroll += 1;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.search_help_scroll = self.search_help_scroll.saturating_sub(1);
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.search_help_scroll = 0;
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.search_help_scroll = usize::MAX / 2;
            }
            KeyCode::Char('O') => {
                self.search_help_scroll = 0;
                self.open_file_picker();
            }
            _ => {}
        }
    }

    // ── Debug View (#52) ────────────────────────────────────────────

    pub(crate) fn handle_debug_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('d') {
            self.view = AppView::Board;
            return;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.view = AppView::Board;
            }
            KeyCode::Char('j') | KeyCode::Char(']') | KeyCode::Down => {
                self.debug.scroll += 1;
            }
            KeyCode::Char('k') | KeyCode::Char('[') | KeyCode::Up => {
                self.debug.scroll = self.debug.scroll.saturating_sub(1);
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.debug.scroll = 0;
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.debug.scroll = usize::MAX / 2;
            }
            KeyCode::Char('O') => {
                self.open_file_picker();
            }
            KeyCode::Char(c @ '1'..='9') => {
                let target = (c as usize) - ('1' as usize);
                self.focus_column_and_return(target);
            }
            _ => {}
        }
    }

    // ── Block Reason overlay ────────────────────────────────────────

    pub(crate) fn handle_block_reason_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('w') => {
                    delete_word_back(&mut self.block_reason_input);
                }
                KeyCode::Char('u') => {
                    self.block_reason_input.clear();
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Enter => {
                let reason = self.block_reason_input.trim().to_string();
                let col = self.active_col;
                let row = self.active_row;
                let mut id = 0;
                if let Some(t) = self.columns.get_mut(col).and_then(|c| c.tasks.get_mut(row)) {
                    t.blocked = true;
                    t.block_reason = reason.clone();
                    t.updated = Utc::now();
                    id = t.id;
                }
                self.persist_task(col, row);
                if self.block_return_view == AppView::Detail {
                    self.detail.cache = None;
                }
                let msg = if reason.is_empty() {
                    format!("Blocked #{}", id)
                } else {
                    format!("Blocked #{}: {}", id, reason)
                };
                self.set_status(msg);
                self.view = self.block_return_view;
            }
            KeyCode::Esc => {
                self.view = self.block_return_view;
            }
            KeyCode::Backspace => {
                if key.modifiers.contains(KeyModifiers::SUPER) {
                    self.block_reason_input.clear();
                } else if key.modifiers.contains(KeyModifiers::ALT) {
                    delete_word_back(&mut self.block_reason_input);
                } else {
                    self.block_reason_input.pop();
                }
            }
            KeyCode::Char(c) => {
                self.block_reason_input.push(c);
            }
            _ => {}
        }
    }
}
