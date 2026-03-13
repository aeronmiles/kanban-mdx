use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{App, AppView};

impl App {
    // ── Help View ───────────────────────────────────────────────────

    pub(crate) fn handle_help_key(&mut self, key: KeyEvent) {
        if self.help_filter_active {
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
                    self.help_filter.pop();
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
            KeyCode::Char(c @ '1'..='9') => {
                let target = (c as usize) - ('1' as usize);
                self.focus_column_and_return(target);
            }
            _ => {}
        }
    }
}
