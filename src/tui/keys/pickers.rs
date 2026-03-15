use crossterm::event::{KeyCode, KeyEvent};

use crate::tui::app::{App, AppView, ContextPickerMode};

impl App {
    // ── Branch Picker View (#46) ────────────────────────────────────

    pub(crate) fn handle_branch_picker_key(&mut self, key: KeyEvent) {
        let filtered = self.filtered_branches();
        let count = filtered.len();

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.view = AppView::Board;
            }
            KeyCode::Down | KeyCode::Char(']') => {
                if count > 0 && self.picker.branch_cursor + 1 < count {
                    self.picker.branch_cursor += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('[') => {
                self.picker.branch_cursor = self.picker.branch_cursor.saturating_sub(1);
            }
            KeyCode::Home => {
                self.picker.branch_cursor = 0;
            }
            KeyCode::End => {
                if count > 0 {
                    self.picker.branch_cursor = count - 1;
                }
            }
            KeyCode::Enter => {
                let filtered = self.filtered_branches();
                if let Some(branch) = filtered.get(self.picker.branch_cursor) {
                    let branch = branch.clone();
                    self.assign_branch_to_task(&branch);
                }
                self.view = AppView::Board;
            }
            KeyCode::Backspace => {
                if self.picker.branch_filter.is_empty() {
                    // Clear branch assignment.
                    self.clear_branch_from_task();
                    self.view = AppView::Board;
                } else {
                    self.picker.branch_filter.pop();
                    self.picker.branch_cursor = 0;
                }
            }
            KeyCode::Char(c) if !c.is_control() => {
                self.picker.branch_filter.push(c);
                self.picker.branch_cursor = 0;
            }
            _ => {}
        }
    }

    // ── Context Picker ──────────────────────────────────────────────

    pub(crate) fn handle_context_picker_key(&mut self, key: KeyEvent) {
        let count = self.filtered_context_items().len();

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.view = AppView::Board;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if count > 0 && self.picker.context_cursor + 1 < count {
                    self.picker.context_cursor += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.picker.context_cursor = self.picker.context_cursor.saturating_sub(1);
            }
            KeyCode::Home => {
                self.picker.context_cursor = 0;
            }
            KeyCode::End => {
                if count > 0 {
                    self.picker.context_cursor = count - 1;
                }
            }
            KeyCode::Enter => {
                let filtered = self.filtered_context_items();
                if let Some(item) = filtered.get(self.picker.context_cursor) {
                    let item = (*item).clone();
                    match self.picker.context_picker_mode {
                        ContextPickerMode::SwitchContext => self.execute_context_select(&item),
                        ContextPickerMode::AssignBranch => self.execute_assign_context(&item),
                    }
                }
            }
            KeyCode::Backspace => {
                if self.picker.context_filter.is_empty() {
                    self.view = AppView::Board;
                } else {
                    self.picker.context_filter.pop();
                    self.picker.context_cursor = 0;
                }
            }
            KeyCode::Char(c) if !c.is_control() => {
                self.picker.context_filter.push(c);
                self.picker.context_cursor = 0;
                self.maybe_add_create_item();
            }
            _ => {}
        }
    }

    // ── Confirm Branch ──────────────────────────────────────────────

    pub(crate) fn handle_confirm_branch_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.create_branch_and_proceed();
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.view = AppView::Board;
                self.set_status("Branch creation cancelled".to_string());
            }
            _ => {}
        }
    }

}
