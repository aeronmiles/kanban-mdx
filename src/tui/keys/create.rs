use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use chrono::Utc;

use crate::model::task::Task;
use crate::tui::app::{App, AppView, CreateState, CreateStep};

impl App {
    // ── Create wizard ───────────────────────────────────────────────

    pub(crate) fn start_create(&mut self) {
        let status = self
            .columns
            .get(self.active_col)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| self.cfg.defaults.status.clone());

        let default_priority_idx = self
            .cfg
            .priorities
            .iter()
            .position(|p| p == &self.cfg.defaults.priority)
            .unwrap_or(0);

        self.create_state = CreateState {
            step: CreateStep::Title,
            title: String::new(),
            body: String::new(),
            priority_index: default_priority_idx,
            tags: String::new(),
            status,
            is_edit: false,
            edit_id: 0,
        };
        self.view = AppView::CreateTask;
    }

    pub(crate) fn start_edit(&mut self) {
        let task = match self.active_task() {
            Some(t) => t.clone(),
            None => return,
        };

        let priority_idx = self
            .cfg
            .priorities
            .iter()
            .position(|p| p == &task.priority)
            .unwrap_or(0);

        self.create_state = CreateState {
            step: CreateStep::Title,
            title: task.title.clone(),
            body: task.body.clone(),
            priority_index: priority_idx,
            tags: task.tags.join(","),
            status: task.status.clone(),
            is_edit: true,
            edit_id: task.id,
        };
        self.view = AppView::CreateTask;
    }

    pub(crate) fn handle_create_key(&mut self, key: KeyEvent) {
        // Esc always cancels.
        if key.code == KeyCode::Esc {
            self.view = AppView::Board;
            return;
        }

        // Ctrl+Enter submits from any step.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('j') {
            self.execute_create_or_edit();
            return;
        }

        // Enter submits from non-body fields (body needs Enter for newlines).
        if key.code == KeyCode::Enter && self.create_state.step != CreateStep::Body {
            if self.create_state.step == CreateStep::Title
                || self.create_state.step == CreateStep::Tags
            {
                // Enter submits the wizard.
                self.execute_create_or_edit();
                return;
            }
            // For priority, Enter also submits.
            if self.create_state.step == CreateStep::Priority {
                self.execute_create_or_edit();
                return;
            }
        }

        // Tab advances to next step.
        if key.code == KeyCode::Tab {
            self.create_state.step = self.create_state.step.next();
            return;
        }

        // Shift+Tab goes back.
        if key.code == KeyCode::BackTab {
            self.create_state.step = self.create_state.step.prev();
            return;
        }

        // Delegate to step-specific handler.
        match self.create_state.step {
            CreateStep::Title => self.handle_create_title(key),
            CreateStep::Body => self.handle_create_body(key),
            CreateStep::Priority => self.handle_create_priority(key),
            CreateStep::Tags => self.handle_create_tags(key),
        }
    }

    fn handle_create_title(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) => self.create_state.title.push(c),
            KeyCode::Backspace => {
                self.create_state.title.pop();
            }
            _ => {}
        }
    }

    fn handle_create_body(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) => self.create_state.body.push(c),
            KeyCode::Enter => self.create_state.body.push('\n'),
            KeyCode::Backspace => {
                self.create_state.body.pop();
            }
            _ => {}
        }
    }

    fn handle_create_priority(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Char(']') | KeyCode::Down => {
                if self.create_state.priority_index + 1 < self.cfg.priorities.len() {
                    self.create_state.priority_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Char('[') | KeyCode::Up => {
                self.create_state.priority_index =
                    self.create_state.priority_index.saturating_sub(1);
            }
            _ => {}
        }
    }

    fn handle_create_tags(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) => self.create_state.tags.push(c),
            KeyCode::Backspace => {
                self.create_state.tags.pop();
            }
            _ => {}
        }
    }

    fn execute_create_or_edit(&mut self) {
        if self.create_state.is_edit {
            self.execute_edit_task();
        } else {
            self.execute_create_task();
        }
    }

    fn execute_create_task(&mut self) {
        let title = self.create_state.title.trim().to_string();
        if title.is_empty() {
            self.view = AppView::Board;
            return;
        }

        let body = self.create_state.body.trim().to_string();
        let priority = self
            .cfg
            .priorities
            .get(self.create_state.priority_index)
            .cloned()
            .unwrap_or_else(|| self.cfg.defaults.priority.clone());

        let tags: Vec<String> = self
            .create_state
            .tags
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let status = self.create_state.status.clone();

        // Determine the next ID by scanning existing files.
        let tasks_path = self.cfg.tasks_path();
        let max_id = crate::model::task::max_id_from_files(&tasks_path).unwrap_or(0);
        let id = std::cmp::max(self.cfg.next_id, max_id + 1);

        let now = Utc::now();
        let task = Task {
            id,
            title: title.clone(),
            status: status.clone(),
            priority,
            created: now,
            updated: now,
            tags,
            body,
            ..Default::default()
        };

        // Generate filename and write.
        let slug = crate::model::task::generate_slug(&title);
        let filename = crate::model::task::generate_filename(id, &slug);
        let path = tasks_path.join(&filename);

        if let Err(e) = crate::io::task_file::write(&path, &task) {
            self.set_status(format!("Create error: {}", e));
            self.view = AppView::Board;
            return;
        }

        // Update config next_id.
        self.cfg.next_id = id + 1;
        // Best-effort save config (ignore error since we have in-memory update).
        let _ = crate::io::config_file::save(&self.cfg);

        self.set_status(format!("Created #{} in {}", id, status));
        self.view = AppView::Board;
        self.reload_tasks();
        self.select_task_by_id(id);
    }

    fn execute_edit_task(&mut self) {
        let title = self.create_state.title.trim().to_string();
        if title.is_empty() {
            self.view = AppView::Board;
            return;
        }

        let body = self.create_state.body.trim().to_string();
        let priority = self
            .cfg
            .priorities
            .get(self.create_state.priority_index)
            .cloned()
            .unwrap_or_else(|| self.cfg.defaults.priority.clone());

        let tags: Vec<String> = self
            .create_state
            .tags
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let edit_id = self.create_state.edit_id;

        // Find and update the task.
        for col in &mut self.columns {
            if let Some(task) = col.tasks.iter_mut().find(|t| t.id == edit_id) {
                task.title = title;
                task.priority = priority;
                task.tags = tags;
                task.body = body;
                task.updated = Utc::now();

                // Persist to disk.
                if !task.file.is_empty() {
                    let path = std::path::Path::new(&task.file);
                    if let Err(e) = crate::io::task_file::write(path, task) {
                        self.set_status(format!("Save error: {}", e));
                    } else {
                        self.set_status(format!("Updated #{}", edit_id));
                    }
                }
                break;
            }
        }

        self.view = AppView::Board;
    }
}
