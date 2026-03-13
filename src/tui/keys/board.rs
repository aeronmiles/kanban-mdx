use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use chrono::Utc;

use crate::tui::app::{priority_label, priority_lower, priority_raise, App, AppView, ViewMode};

impl App {
    // ── Board View ──────────────────────────────────────────────────

    pub(crate) fn handle_board_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('f') => {
                    self.view = AppView::Search;
                    return;
                }
                KeyCode::Char('j') => {
                    // Ctrl+J: full page down (or scroll reader).
                    if self.reader_open {
                        self.reader_scroll += self.reader_page_size();
                    } else {
                        self.move_row_down(self.visible_height());
                    }
                    return;
                }
                KeyCode::Char('k') => {
                    // Ctrl+K: full page up (or scroll reader).
                    if self.reader_open {
                        self.reader_scroll =
                            self.reader_scroll.saturating_sub(self.reader_page_size());
                    } else {
                        self.move_row_up(self.visible_height());
                    }
                    return;
                }
                KeyCode::Char('z') => {
                    // Ctrl+Z: undo last mutation.
                    match crate::board::undo::pop_undo(self.cfg.dir()) {
                        Ok(Some(entry)) => {
                            if let Err(e) =
                                crate::board::undo::restore_file_snapshots(&entry.files_before)
                            {
                                self.set_status(format!("Undo failed: {}", e));
                            } else {
                                let _ = crate::board::undo::push_redo(self.cfg.dir(), &entry);
                                self.set_status(format!("Undone: {}", entry.action));
                                self.reload_tasks();
                            }
                        }
                        Ok(None) => {
                            self.set_status("Nothing to undo");
                        }
                        Err(e) => {
                            self.set_status(format!("Undo error: {}", e));
                        }
                    }
                    return;
                }
                KeyCode::Char('r') => {
                    // Ctrl+R: redo last undone mutation.
                    match crate::board::undo::pop_redo(self.cfg.dir()) {
                        Ok(Some(entry)) => {
                            if let Err(e) =
                                crate::board::undo::restore_file_snapshots(&entry.files_after)
                            {
                                self.set_status(format!("Redo failed: {}", e));
                            } else {
                                let _ =
                                    crate::board::undo::append_undo_only(self.cfg.dir(), &entry);
                                self.set_status(format!("Redone: {}", entry.action));
                                self.reload_tasks();
                            }
                        }
                        Ok(None) => {
                            self.set_status("Nothing to redo");
                        }
                        Err(e) => {
                            self.set_status(format!("Redo error: {}", e));
                        }
                    }
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
                KeyCode::Char('d') => {
                    // Ctrl+D: debug info overlay.
                    self.debug.scroll = 0;
                    self.view = AppView::Debug;
                    return;
                }
                _ => return,
            }
        }

        // Alt modifiers: heading navigation in reader panel, saturation.
        // On macOS, Alt+[ and Alt+] may send curly quotes instead.
        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Char(']') => {
                    if self.reader_open {
                        self.reader_next_heading_level(2);
                    }
                    return;
                }
                KeyCode::Char('[') => {
                    if self.reader_open {
                        self.reader_prev_heading_level(2);
                    }
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

        // macOS curly quotes from Alt+[ / Alt+] (outside modifier check).
        match key.code {
            KeyCode::Char('\u{2018}') | KeyCode::Char('\u{2019}') => {
                // macOS Alt+] sends right single curly quote.
                if self.reader_open {
                    self.reader_next_heading_level(2);
                }
                return;
            }
            KeyCode::Char('\u{201c}') | KeyCode::Char('\u{201d}') => {
                // macOS Alt+[ sends left double curly quote.
                if self.reader_open {
                    self.reader_prev_heading_level(2);
                }
                return;
            }
            _ => {}
        }

        // Some terminals send Shift+digit as Char('3') + SHIFT modifier
        // instead of the symbol character (e.g. '#'). Handle both forms.
        if key.modifiers.contains(KeyModifiers::SHIFT) {
            if let KeyCode::Char(c @ '1'..='9') = key.code {
                let idx = (c as usize) - ('1' as usize);
                self.toggle_collapse_idx(idx);
                return;
            }
        }

        match key.code {
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Char('{') => {
                self.cycle_or_move_col_left();
            }
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Char('}') => {
                self.cycle_or_move_col_right();
            }
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Char(']') => {
                self.move_row_down(1);
            }
            KeyCode::Char('k') | KeyCode::Up | KeyCode::Char('[') => {
                self.move_row_up(1);
            }
            KeyCode::Char('g') | KeyCode::Home => {
                if let Some(indices) = self.active_col_filtered_indices() {
                    if let Some(&first) = indices.first() {
                        self.active_row = first;
                    }
                } else {
                    self.active_row = 0;
                }
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.active_row = self.last_row_index();
            }
            KeyCode::Char('J') => {
                // Shift+J: half-page down.
                self.move_row_down(self.visible_height() / 2);
            }
            KeyCode::Char('K') => {
                // Shift+K: half-page up.
                self.move_row_up(self.visible_height() / 2);
            }
            KeyCode::PageDown => {
                if self.reader_open {
                    self.reader_scroll += self.reader_page_size();
                } else {
                    self.move_row_down(self.visible_height());
                }
            }
            KeyCode::PageUp => {
                if self.reader_open {
                    self.reader_scroll = self.reader_scroll.saturating_sub(self.reader_page_size());
                } else {
                    self.move_row_up(self.visible_height());
                }
            }
            KeyCode::Char('v') => {
                // Toggle visual mode (native text selection) — vim convention.
                self.toggle_select_mode();
            }
            KeyCode::Char('V') => {
                self.view_mode = match self.view_mode {
                    ViewMode::Cards => ViewMode::List,
                    ViewMode::List => ViewMode::Cards,
                };
                self.persist_tui_state();
            }
            KeyCode::Char('s') => {
                self.sort_mode = self.sort_mode.next();
                self.sort_all_columns();
                self.set_status(format!("Sort: {}", self.sort_mode.label()));
                self.persist_tui_state();
            }
            KeyCode::Char('S') => {
                self.sort_mode = self.sort_mode.prev();
                self.sort_all_columns();
                self.set_status(format!("Sort: {}", self.sort_mode.label()));
                self.persist_tui_state();
            }
            KeyCode::Char('a') => {
                // Cycle time mode (matches Go's 'a' binding).
                self.time_mode = self.time_mode.next();
                self.set_status(format!("Time: {}", self.time_mode.label()));
                self.persist_tui_state();
            }
            KeyCode::Char('t') => {
                // Cycle theme.
                self.theme_kind = self.theme_kind.next();
                self.brightness = 0.0;
                self.saturation = -0.2;
                self.set_status(format!("Theme: {}", self.theme_kind.label()));
                self.persist_tui_state();
            }
            KeyCode::Char('T') => {
                // Reset brightness/saturation adjustments.
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
            KeyCode::Char('x') => {
                if let Some(col) = self.columns.get_mut(self.active_col) {
                    col.collapsed = !col.collapsed;
                    if col.collapsed {
                        self.move_col_right();
                    }
                }
                self.persist_collapsed();
            }
            KeyCode::Char('X') => {
                for col in &mut self.columns {
                    col.collapsed = false;
                }
                self.persist_collapsed();
            }
            KeyCode::Char(c @ '1'..='9') => {
                let target = (c as usize) - ('1' as usize);
                if target < self.columns.len() {
                    for (i, col) in self.columns.iter_mut().enumerate() {
                        col.collapsed = i != target;
                    }
                    self.active_col = target;
                    self.clamp_active_row();
                    self.persist_collapsed();
                }
            }
            // Shift+digit toggle-collapse (US + UK layouts).
            // US: ! @ # $ % ^ & *
            // UK: ! " £ $ % ^ & *
            KeyCode::Char('!') => self.toggle_collapse_idx(0),
            KeyCode::Char('@') => self.toggle_collapse_idx(1),
            KeyCode::Char('#' | '£') => self.toggle_collapse_idx(2),
            KeyCode::Char('$') => self.toggle_collapse_idx(3),
            KeyCode::Char('%') => self.toggle_collapse_idx(4),
            KeyCode::Char('^') => self.toggle_collapse_idx(5),
            KeyCode::Char('&') => self.toggle_collapse_idx(6),
            KeyCode::Char('*') => self.toggle_collapse_idx(7),
            KeyCode::Char('(') => {
                // Shift+9: reader prev heading or toggle column 9.
                if self.reader_open {
                    self.reader_prev_heading();
                } else {
                    self.toggle_collapse_idx(8);
                }
            }
            KeyCode::Char(')') => {
                // Shift+0: reader next heading.
                if self.reader_open {
                    self.reader_next_heading();
                }
            }
            KeyCode::Char('\'') => {
                // Next ## heading in reader (level 2).
                if self.reader_open {
                    self.reader_next_heading_level(2);
                }
            }
            KeyCode::Char('"') => {
                // UK Shift+2: toggle column 2; also reader ## heading nav.
                if self.reader_open {
                    self.reader_prev_heading_level(2);
                } else {
                    self.toggle_collapse_idx(1);
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
            KeyCode::Char('d') => {
                if self.active_task().is_some() {
                    self.picker.delete_cursor = 1;
                    self.view = AppView::ConfirmDelete;
                }
            }
            KeyCode::Char('c') => {
                self.start_create();
            }
            KeyCode::Char('e') => {
                self.start_edit();
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                let mut msg = None;
                if let Some(task) = self
                    .columns
                    .get_mut(self.active_col)
                    .and_then(|c| c.tasks.get_mut(self.active_row))
                {
                    task.priority = priority_raise(&task.priority);
                    task.updated = Utc::now();
                    msg = Some(format!(
                        "#{} priority: {}",
                        task.id,
                        priority_label(&task.priority)
                    ));
                }
                if let Some(m) = msg {
                    self.set_status(m);
                }
                self.persist_task(self.active_col, self.active_row);
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                let mut msg = None;
                if let Some(task) = self
                    .columns
                    .get_mut(self.active_col)
                    .and_then(|c| c.tasks.get_mut(self.active_row))
                {
                    task.priority = priority_lower(&task.priority);
                    task.updated = Utc::now();
                    msg = Some(format!(
                        "#{} priority: {}",
                        task.id,
                        priority_label(&task.priority)
                    ));
                }
                if let Some(m) = msg {
                    self.set_status(m);
                }
                self.persist_task(self.active_col, self.active_row);
            }
            KeyCode::Char(':') => {
                // Goto task by ID (matches Go TUI's ':' binding).
                self.goto_input.clear();
                self.search.query.clear();
                self.clear_sem_state();
                self.goto_active = true;
            }
            KeyCode::Char('/') => {
                self.view = AppView::Search;
            }
            KeyCode::Enter => {
                if self.active_task().is_some() {
                    // Record board position so back navigation can return here.
                    self.push_jump();
                    self.detail.scroll = 0;
                    self.view = AppView::Detail;
                }
            }
            KeyCode::Char('?') => {
                self.view = AppView::Help;
            }
            KeyCode::Char('u') => {
                // Undo (same as Ctrl+Z).
                match crate::board::undo::pop_undo(self.cfg.dir()) {
                    Ok(Some(entry)) => {
                        if let Err(e) =
                            crate::board::undo::restore_file_snapshots(&entry.files_before)
                        {
                            self.set_status(format!("Undo failed: {}", e));
                        } else {
                            let _ = crate::board::undo::push_redo(self.cfg.dir(), &entry);
                            self.set_status(format!("Undone: {}", entry.action));
                            self.reload_tasks();
                        }
                    }
                    Ok(None) => {
                        self.set_status("Nothing to undo");
                    }
                    Err(e) => {
                        self.set_status(format!("Undo error: {}", e));
                    }
                }
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
                // Open task in $EDITOR.
                self.open_in_editor();
            }
            KeyCode::Char('z') => {
                // Fold deeper (reader).
                if self.reader_open {
                    self.fold_deeper();
                }
            }
            KeyCode::Char('Z') => {
                // Unfold (reader).
                if self.reader_open {
                    self.fold_shallower();
                }
            }
            KeyCode::Char('>') => {
                // Widen reader panel.
                self.adjust_reader_width_pct(5);
            }
            KeyCode::Char('<') => {
                // Narrow reader panel.
                self.adjust_reader_width_pct(-5);
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
            KeyCode::Char('f') => {
                // Open search (same as /).
                self.view = AppView::Search;
            }
            KeyCode::Char('b') => {
                // Open branch assignment picker (assign branch to task).
                if self.active_task().is_some() {
                    self.open_assign_context(false);
                }
            }
            KeyCode::Char('w') => {
                // Instant toggle: filter board to tasks with worktrees (#57).
                self.worktree_filter_active = !self.worktree_filter_active;
                let label = if self.worktree_filter_active {
                    "Worktree filter: ON"
                } else {
                    "Worktree filter: OFF"
                };
                self.set_status(label.to_string());
            }
            KeyCode::Char('C') => {
                // Open context picker (all branches).
                self.open_context_picker(false);
            }
            KeyCode::Char('W') => {
                // Open context picker (worktree branches only).
                self.open_context_picker(true);
            }
            KeyCode::Char('r') => {
                self.reload_tasks();
            }
            KeyCode::Char('R') => {
                self.toggle_reader();
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                if self.select_mode && key.code == KeyCode::Esc {
                    self.toggle_select_mode();
                } else if !self.search.query.is_empty() {
                    self.search.query.clear();
                    self.clear_sem_state();
                    self.status_message.clear();
                    self.status_message_at = None;
                } else if self.worktree_filter_active {
                    self.worktree_filter_active = false;
                    self.set_status("Worktree filter: OFF".to_string());
                } else if self.picker.context_mode {
                    self.picker.context_mode = false;
                    self.picker.context_task_id = 0;
                    self.picker.context_label.clear();
                    self.set_status("Context cleared".to_string());
                } else {
                    self.should_quit = true;
                }
            }
            _ => {}
        }

        // Update reader scroll when active task changes.
        if self.reader_open {
            self.reader_scroll = 0;
        }
    }

    pub(crate) fn handle_board_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if self.reader_open && self.is_mouse_over_reader(mouse.column) {
                    self.reader_scroll = self.reader_scroll.saturating_sub(3);
                } else {
                    self.move_row_up(3);
                }
            }
            MouseEventKind::ScrollDown => {
                if self.reader_open && self.is_mouse_over_reader(mouse.column) {
                    self.reader_scroll += 3;
                } else {
                    self.move_row_down(3);
                }
            }
            MouseEventKind::ScrollLeft => {
                self.move_col_left();
            }
            MouseEventKind::ScrollRight => {
                self.move_col_right();
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // Try to select the task that was clicked.
                // This is a best-effort hit test.
                self.handle_board_click(mouse.column, mouse.row);
            }
            _ => {}
        }
    }

    pub(crate) fn handle_board_click(&mut self, _x: u16, _y: u16) {
        // Click handling is a best-effort approximation since we don't have
        // exact widget positions. For now, clicking opens the detail view
        // if a task is selected.
    }
}
