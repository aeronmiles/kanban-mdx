use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

use crate::tui::app::{delete_word_back, App, AppView};

impl App {
    // ── Detail View ─────────────────────────────────────────────────

    pub(crate) fn handle_detail_key(&mut self, key: KeyEvent) {
        // Handle find input mode first.
        if self.detail.find_active {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('n') => {
                        self.find_next();
                        return;
                    }
                    KeyCode::Char('p') => {
                        self.find_prev();
                        return;
                    }
                    KeyCode::Char('f') => {
                        // Toggle find off.
                        self.detail.find_active = false;
                        return;
                    }
                    KeyCode::Char('r') => {
                        // Ctrl+R: reverse history search (same as Up arrow).
                        let current = self.detail.find_query.clone();
                        if let Some(entry) = self.detail.find_history.up(&current) {
                            self.detail.find_query = entry.to_string();
                            self.on_find_query_changed();
                        }
                        return;
                    }
                    KeyCode::Char('w') => {
                        // Ctrl+W: delete last word.
                        delete_word_back(&mut self.detail.find_query);
                        self.on_find_query_changed();
                        self.detail.find_history.reset();
                        self.detail.find_tab_prefix = None;
                        self.detail.find_tab_idx = 0;
                        return;
                    }
                    KeyCode::Char('u') => {
                        // Ctrl+U: delete entire line.
                        self.detail.find_query.clear();
                        self.on_find_query_changed();
                        self.detail.find_history.reset();
                        self.detail.find_tab_prefix = None;
                        self.detail.find_tab_idx = 0;
                        return;
                    }
                    _ => return,
                }
            }
            match key.code {
                KeyCode::Esc => {
                    self.detail.find_active = false;
                    self.detail.find_query.clear();
                    self.detail.find_matches.clear();
                    self.detail.find_current = 0;
                    self.detail.find_tab_prefix = None;
                    self.detail.find_tab_idx = 0;
                }
                KeyCode::Enter => {
                    if !self.detail.find_query.is_empty() {
                        self.detail.find_history.push(&self.detail.find_query.clone());
                    }
                    self.detail.find_active = false;
                    self.detail.find_tab_prefix = None;
                    self.detail.find_tab_idx = 0;
                    // Keep matches visible for n/N navigation.
                }
                KeyCode::Backspace => {
                    if key.modifiers.contains(KeyModifiers::SUPER) {
                        self.detail.find_query.clear();
                    } else if key.modifiers.contains(KeyModifiers::ALT) {
                        delete_word_back(&mut self.detail.find_query);
                    } else {
                        self.detail.find_query.pop();
                    }
                    self.on_find_query_changed();
                    self.detail.find_history.reset();
                    self.detail.find_tab_prefix = None;
                    self.detail.find_tab_idx = 0;
                }
                KeyCode::Up => {
                    let current = self.detail.find_query.clone();
                    if let Some(entry) = self.detail.find_history.up(&current) {
                        self.detail.find_query = entry.to_string();
                        self.on_find_query_changed();
                    }
                    self.detail.find_tab_prefix = None;
                    self.detail.find_tab_idx = 0;
                }
                KeyCode::Down => {
                    let current = self.detail.find_query.clone();
                    if let Some(entry) = self.detail.find_history.down(&current) {
                        self.detail.find_query = entry.to_string();
                        self.on_find_query_changed();
                    }
                    self.detail.find_tab_prefix = None;
                    self.detail.find_tab_idx = 0;
                }
                KeyCode::Tab => {
                    // Save the typed prefix on first Tab press.
                    let prefix = self
                        .detail.find_tab_prefix
                        .get_or_insert_with(|| self.detail.find_query.clone())
                        .clone();
                    let completions = self.detail.find_history.completions(&prefix);
                    if !completions.is_empty() {
                        let rev_idx = self.detail.find_tab_idx % completions.len();
                        let entry = completions[completions.len() - 1 - rev_idx];
                        self.detail.find_query = entry.to_string();
                        self.on_find_query_changed();
                        self.detail.find_tab_idx += 1;
                    }
                }
                KeyCode::Char('?') if self.detail.find_query.is_empty() => {
                    self.search_help_return = AppView::Detail;
                    self.search_help_scroll = 0;
                    self.view = AppView::SearchHelp;
                }
                KeyCode::Char(c) => {
                    self.detail.find_query.push(c);
                    self.on_find_query_changed();
                    self.detail.find_tab_prefix = None;
                    self.detail.find_tab_idx = 0;
                    self.detail.find_history.reset();
                }
                _ => {}
            }
            return;
        }

        // Ctrl modifiers.
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('j') => {
                    // Ctrl+J: full page down.
                    self.detail.scroll += self.detail_page_size();
                    return;
                }
                KeyCode::Char('k') => {
                    // Ctrl+K: full page up.
                    self.detail.scroll = self.detail.scroll.saturating_sub(self.detail_page_size());
                    return;
                }
                KeyCode::Char('f') => {
                    // Ctrl+F: open find in detail.
                    self.detail.find_active = true;
                    self.detail.find_query.clear();
                    self.detail.find_matches.clear();
                    self.detail.find_current = 0;
                    return;
                }
                KeyCode::Char('g') => {
                    // Ctrl+G: go-to-task dialog.
                    self.goto_input.clear();
                    self.search.query.clear();
                    self.clear_sem_state();
                    self.goto_active = true;
                    return;
                }
                _ => return,
            }
        }

        // Alt modifiers: ## heading navigation, saturation.
        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Char(']') => {
                    self.detail_next_heading_level(2);
                    return;
                }
                KeyCode::Char('[') => {
                    self.detail_prev_heading_level(2);
                    return;
                }
                KeyCode::Char('.') => {
                    // Alt+.: saturation up (non-macOS).
                    self.saturation = (self.saturation + 0.1).min(1.0);
                    self.set_status(format!("Saturation: {:+.0}%", self.saturation * 100.0));
                    self.persist_tui_state();
                    return;
                }
                KeyCode::Char(',') => {
                    // Alt+,: saturation down (non-macOS).
                    self.saturation = (self.saturation - 0.1).max(-1.0);
                    self.set_status(format!("Saturation: {:+.0}%", self.saturation * 100.0));
                    self.persist_tui_state();
                    return;
                }
                _ => return,
            }
        }

        // macOS curly quotes from Alt+[ / Alt+].
        match key.code {
            KeyCode::Char('\u{2018}') | KeyCode::Char('\u{2019}') => {
                self.detail_next_heading_level(2);
                return;
            }
            KeyCode::Char('\u{201c}') | KeyCode::Char('\u{201d}') => {
                self.detail_prev_heading_level(2);
                return;
            }
            _ => {}
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Backspace => {
                if self.select_mode && key.code == KeyCode::Esc {
                    self.toggle_select_mode();
                } else {
                    // Save final position without forking forward history.
                    self.exit_jump();
                    self.detail.find_query.clear();
                    self.detail.find_matches.clear();
                    self.detail.find_current = 0;
                    self.view = AppView::Board;
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.detail.scroll += 1;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.detail.scroll = self.detail.scroll.saturating_sub(1);
            }
            KeyCode::Char(']') => {
                // Scroll down by 3 lines.
                self.detail.scroll += 3;
            }
            KeyCode::Char('[') => {
                // Scroll up by 3 lines.
                self.detail.scroll = self.detail.scroll.saturating_sub(3);
            }
            KeyCode::Char('J') => {
                // Shift+J: half-page down.
                self.detail.scroll += self.detail_page_size() / 2;
            }
            KeyCode::Char('K') => {
                // Shift+K: half-page up.
                self.detail.scroll = self
                    .detail.scroll
                    .saturating_sub(self.detail_page_size() / 2);
            }
            KeyCode::Char('d') => {
                // Half-page down (vim-style).
                self.detail.scroll += self.detail_page_size() / 2;
            }
            KeyCode::Char('u') => {
                // Half-page up (vim-style).
                self.detail.scroll = self
                    .detail.scroll
                    .saturating_sub(self.detail_page_size() / 2);
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.detail.scroll = 0;
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.detail.scroll = usize::MAX / 2;
            }
            KeyCode::Char(')') | KeyCode::Char('}') => {
                // Next heading.
                self.detail_next_heading();
            }
            KeyCode::Char('(') | KeyCode::Char('{') => {
                // Previous heading.
                self.detail_prev_heading();
            }
            KeyCode::Char('\'') => {
                // Next ## heading (level 2).
                self.detail_next_heading_level(2);
            }
            KeyCode::Char('"') => {
                // Previous ## heading (level 2).
                self.detail_prev_heading_level(2);
            }
            // n/N/f/F for find navigation when matches exist.
            KeyCode::Char('n') | KeyCode::Char('f') => {
                if !self.detail.find_matches.is_empty() {
                    self.find_next();
                }
            }
            KeyCode::Char('N') | KeyCode::Char('F') => {
                if !self.detail.find_matches.is_empty() {
                    self.find_prev();
                }
            }
            KeyCode::Char('m') => {
                if self.active_task().is_some() {
                    self.picker.move_cursor = 0;
                    self.picker.move_filter.clear();
                    self.picker.move_filter_active = false;
                    self.view = AppView::MoveTask;
                }
            }
            KeyCode::Char('v') => {
                // Toggle visual mode (native text selection) — vim convention.
                self.toggle_select_mode();
            }
            KeyCode::Char('y') => {
                // Yank: copy task title + body to clipboard (vim yy).
                self.copy_task_content();
            }
            KeyCode::Char('Y') => {
                // Yank path: copy task file path to clipboard.
                self.copy_task_path();
            }
            KeyCode::Char('o') => {
                self.open_in_editor();
            }
            KeyCode::Char('>') => {
                self.adjust_reader_max_width(10);
            }
            KeyCode::Char('<') => {
                self.adjust_reader_max_width(-10);
            }
            // macOS Alt+, / Alt+. produce ≤ / ≥ instead of sending Alt modifier.
            KeyCode::Char('\u{2265}') => {
                // Alt+. (macOS): saturation up.
                self.saturation = (self.saturation + 0.1).min(1.0);
                self.set_status(format!("Saturation: {:+.0}%", self.saturation * 100.0));
                self.persist_tui_state();
            }
            KeyCode::Char('\u{2264}') => {
                // Alt+, (macOS): saturation down.
                self.saturation = (self.saturation - 0.1).max(-1.0);
                self.set_status(format!("Saturation: {:+.0}%", self.saturation * 100.0));
                self.persist_tui_state();
            }
            KeyCode::Char('z') => {
                self.fold_deeper();
            }
            KeyCode::Char('Z') => {
                self.fold_shallower();
            }
            KeyCode::Char('/') => {
                self.detail.find_active = true;
                self.detail.find_query.clear();
                self.detail.find_matches.clear();
                self.detail.find_current = 0;
            }
            KeyCode::Char(':') => {
                // Goto task by ID (matches Go TUI's ':' binding).
                self.goto_input.clear();
                self.search.query.clear();
                self.clear_sem_state();
                self.goto_active = true;
            }
            KeyCode::Char(c @ '1'..='9') => {
                let index = (c as usize) - ('1' as usize);
                self.detail_goto_heading_index(index);
            }
            KeyCode::Char('t') => {
                self.theme_kind = self.theme_kind.next();
                self.brightness = 0.0;
                self.saturation = -0.2;
                self.set_status(format!("Theme: {}", self.theme_kind.label()));
                self.persist_tui_state();
            }
            KeyCode::Char('T') => {
                self.brightness = 0.0;
                self.saturation = -0.2;
                self.set_status("Theme adjustments reset");
                self.persist_tui_state();
            }
            KeyCode::Char('.') => {
                self.brightness = (self.brightness + 0.05).min(1.0);
                self.set_status(format!("Brightness: {:+.0}%", self.brightness * 100.0));
                self.persist_tui_state();
            }
            KeyCode::Char(',') => {
                self.brightness = (self.brightness - 0.05).max(-1.0);
                self.set_status(format!("Brightness: {:+.0}%", self.brightness * 100.0));
                self.persist_tui_state();
            }
            _ => {}
        }
    }

    pub(crate) fn handle_detail_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.detail.scroll = self.detail.scroll.saturating_sub(3);
            }
            MouseEventKind::ScrollDown => {
                self.detail.scroll += 3;
            }
            _ => {}
        }
    }
}
