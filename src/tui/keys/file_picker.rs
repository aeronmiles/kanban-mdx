//! Key handler for the file picker overlay.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{delete_word_back, App};

impl App {
    pub(crate) fn handle_file_picker_key(&mut self, key: KeyEvent) {
        if self.file_picker.path_input_active {
            self.handle_file_picker_path_key(key);
            return;
        }

        // Directory browser mode.
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.view = self.file_picker.return_view;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let filtered = self.filtered_file_entries();
                if !filtered.is_empty() {
                    self.file_picker.cursor = (self.file_picker.cursor + 1).min(filtered.len() - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.file_picker.cursor = self.file_picker.cursor.saturating_sub(1);
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.file_picker.cursor = 0;
            }
            KeyCode::Char('G') | KeyCode::End => {
                let filtered = self.filtered_file_entries();
                if !filtered.is_empty() {
                    self.file_picker.cursor = filtered.len() - 1;
                }
            }
            KeyCode::Enter => {
                let filtered = self.filtered_file_entries();
                if let Some(entry) = filtered.get(self.file_picker.cursor) {
                    let path = entry.path.clone();
                    let is_dir = entry.is_dir;
                    if is_dir {
                        self.file_picker.cwd = path;
                        self.scan_file_picker_dir();
                    } else {
                        self.open_file_entry(path);
                    }
                }
            }
            KeyCode::Backspace => {
                if !self.file_picker.filter.is_empty() {
                    self.file_picker.filter.pop();
                    self.file_picker.cursor = 0;
                } else {
                    // Go up one directory.
                    if let Some(parent) = self.file_picker.cwd.parent() {
                        self.file_picker.cwd = parent.to_path_buf();
                        self.scan_file_picker_dir();
                    }
                }
            }
            KeyCode::Char('h') | KeyCode::Left => {
                // Go up one directory.
                if let Some(parent) = self.file_picker.cwd.parent() {
                    self.file_picker.cwd = parent.to_path_buf();
                    self.scan_file_picker_dir();
                }
            }
            KeyCode::Char('/') => {
                self.file_picker.path_input_active = true;
                self.file_picker.path_input = self.file_picker.cwd.display().to_string();
                if !self.file_picker.path_input.ends_with('/') {
                    self.file_picker.path_input.push('/');
                }
                self.file_picker.tab_completions.clear();
                self.file_picker.tab_idx = 0;
                self.file_picker.tab_prefix = None;
            }
            KeyCode::Char(c) => {
                self.file_picker.filter.push(c);
                self.file_picker.cursor = 0;
            }
            _ => {}
        }
    }

    fn handle_file_picker_path_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('w') => {
                    delete_word_back(&mut self.file_picker.path_input);
                    self.file_picker.tab_prefix = None;
                    self.file_picker.tab_idx = 0;
                    return;
                }
                KeyCode::Char('u') => {
                    self.file_picker.path_input.clear();
                    self.file_picker.tab_prefix = None;
                    self.file_picker.tab_idx = 0;
                    return;
                }
                _ => return,
            }
        }

        match key.code {
            KeyCode::Esc => {
                self.file_picker.path_input_active = false;
                self.file_picker.path_input.clear();
            }
            KeyCode::Enter => {
                let input = self.file_picker.path_input.clone();
                self.file_picker.path_input_active = false;
                self.file_picker.path_input.clear();

                let path = std::path::PathBuf::from(&input);
                if path.is_dir() {
                    self.file_picker.cwd = path;
                    self.scan_file_picker_dir();
                } else if path.is_file() {
                    self.open_file_entry(path);
                } else {
                    self.set_status(format!("Not found: {}", input));
                }
            }
            KeyCode::Tab | KeyCode::BackTab => {
                // Save typed prefix on first Tab press; recompute if input
                // changed (e.g. user tabbed to a dir/ and presses Tab again).
                if self.file_picker.tab_prefix.is_none()
                    || self.file_picker.tab_prefix.as_deref() != Some(&self.file_picker.path_input)
                {
                    // If current input ends with '/' and isn't the prefix,
                    // re-scan inside that directory.
                    self.file_picker.tab_prefix = Some(self.file_picker.path_input.clone());
                    self.file_picker.tab_idx = 0;
                    self.compute_path_completions();

                    // If exactly one completion and it's a directory, apply it
                    // immediately so the next Tab descends into it.
                    if self.file_picker.tab_completions.len() == 1 {
                        self.file_picker.path_input =
                            self.file_picker.tab_completions[0].clone();
                        // Re-scan if it's a directory for continued tabbing.
                        if self.file_picker.path_input.ends_with('/') {
                            self.file_picker.tab_prefix = Some(self.file_picker.path_input.clone());
                            self.file_picker.tab_idx = 0;
                            self.compute_path_completions();
                        }
                        return;
                    }
                }

                if !self.file_picker.tab_completions.is_empty() {
                    let len = self.file_picker.tab_completions.len();
                    if key.code == KeyCode::BackTab {
                        // Reverse cycle.
                        self.file_picker.tab_idx =
                            (self.file_picker.tab_idx + len - 1) % len;
                    }
                    let idx = self.file_picker.tab_idx % len;
                    self.file_picker.path_input =
                        self.file_picker.tab_completions[idx].clone();
                    if key.code == KeyCode::Tab {
                        self.file_picker.tab_idx = (idx + 1) % len;
                    }
                }
            }
            KeyCode::Backspace => {
                if self.file_picker.path_input.is_empty() {
                    self.file_picker.path_input_active = false;
                } else {
                    self.file_picker.path_input.pop();
                    self.file_picker.tab_prefix = None;
                    self.file_picker.tab_idx = 0;
                }
            }
            KeyCode::Char(c) => {
                self.file_picker.path_input.push(c);
                self.file_picker.tab_prefix = None;
                self.file_picker.tab_idx = 0;
            }
            _ => {}
        }
    }
}
