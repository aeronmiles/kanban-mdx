//! TUI actions — move, delete, collapse, clipboard, editor, toggle operations.

use chrono::Utc;

use super::app::App;
use super::types::AppView;

impl App {
    pub(crate) fn toggle_reader(&mut self) {
        if self.reader_open {
            self.reader_open = false;
            self.reader_scroll = 0;
        } else if self.terminal_width >= 60 {
            self.reader_open = true;
            self.reader_scroll = 0;
        } else {
            self.set_status("Terminal too narrow for reader panel");
        }
    }

    pub(crate) fn execute_move(&mut self, target_col: usize) {
        if target_col >= self.columns.len() || target_col == self.active_col {
            return;
        }
        let src = self.active_col;
        if self.active_row >= self.columns[src].tasks.len() {
            return;
        }

        let mut task = self.columns[src].tasks.remove(self.active_row);
        let id = task.id;
        let new_status = self.columns[target_col].name.clone();

        task.status.clone_from(&new_status);
        task.updated = Utc::now();

        self.columns[target_col].tasks.push(task);
        Self::sort_column(&mut self.columns[target_col], self.sort_mode);

        let new_row = self.columns[target_col]
            .tasks
            .iter()
            .position(|t| t.id == id)
            .unwrap_or(0);

        self.set_status(format!("Moved #{} to {}", id, new_status));
        self.active_col = target_col;
        self.active_row = new_row;

        self.persist_task(target_col, new_row);
    }

    pub(crate) fn execute_delete(&mut self) {
        let col_idx = self.active_col;
        let row_idx = self.active_row;
        let can_delete = self
            .columns
            .get(col_idx)
            .map_or(false, |col| row_idx < col.tasks.len());

        if !can_delete {
            return;
        }

        let mut task = self.columns[col_idx].tasks.remove(row_idx);
        let id = task.id;

        // Soft-delete: move to archived status.
        task.status = crate::model::config::ARCHIVED_STATUS.to_string();
        task.updated = Utc::now();
        self.persist_deleted_task(&task);

        self.set_status(format!("Deleted #{}", id));

        let col = &self.columns[col_idx];
        if !col.tasks.is_empty() {
            self.active_row = self.active_row.min(col.tasks.len() - 1);
        } else {
            self.active_row = 0;
        }
    }

    pub(crate) fn toggle_collapse_idx(&mut self, idx: usize) {
        if idx < self.columns.len() {
            // Don't collapse the last visible column.
            if !self.columns[idx].collapsed {
                let expanded = self.columns.iter().filter(|c| !c.collapsed).count();
                if expanded <= 1 {
                    return;
                }
            }
            let new_state = !self.columns[idx].collapsed;
            self.columns[idx].collapsed = new_state;
            if new_state {
                if self.active_col == idx {
                    self.move_col_right();
                }
            } else {
                self.switch_col(idx);
            }
            self.persist_collapsed();
        }
    }

    /// Switch to board view and solo-focus a column by 0-based index.
    /// Used by digit shortcuts (`1`-`9`) from non-board views.
    pub(crate) fn focus_column_and_return(&mut self, idx: usize) {
        if idx < self.columns.len() {
            for (i, col) in self.columns.iter_mut().enumerate() {
                col.collapsed = i != idx;
            }
            self.switch_col(idx);
            self.view = AppView::Board;
        }
    }

    /// Toggle select mode: disables/enables mouse capture so the terminal
    /// handles native text selection.
    pub(crate) fn toggle_select_mode(&mut self) {
        self.select_mode = !self.select_mode;
        let mut stdout = std::io::stdout();
        if self.select_mode {
            let _ = crossterm::execute!(stdout, crossterm::event::DisableMouseCapture);
            self.set_status("Visual mode ON — native text selection enabled");
        } else {
            let _ = crossterm::execute!(stdout, crossterm::event::EnableMouseCapture);
            self.set_status("Visual mode OFF");
        }
    }

    /// Best-effort clipboard copy via platform tools (pbcopy / xclip).
    fn copy_to_clipboard(text: &str) -> bool {
        #[cfg(target_os = "macos")]
        {
            use std::process::{Command, Stdio};
            if let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = std::io::Write::write_all(stdin, text.as_bytes());
                }
                let _ = child.wait();
                return true;
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            use std::process::{Command, Stdio};
            let result = Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(Stdio::piped())
                .spawn();
            if let Ok(mut child) = result {
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = std::io::Write::write_all(stdin, text.as_bytes());
                }
                let _ = child.wait();
                return true;
            }
        }
        false
    }

    pub(crate) fn copy_task_path(&mut self) {
        if let Some(task) = self.active_task() {
            if task.file.is_empty() {
                self.set_status("No file path");
                return;
            }
            let path = task.file.clone();
            Self::copy_to_clipboard(&path);
            self.set_status(format!("Copied: {}", path));
        }
    }

    /// Copy the active task's title and body to the clipboard.
    pub(crate) fn copy_task_content(&mut self) {
        if let Some(task) = self.active_task() {
            let mut text = task.title.clone();
            if !task.body.is_empty() {
                text.push_str("\n\n");
                text.push_str(&task.body);
            }
            Self::copy_to_clipboard(&text);
            self.set_status(format!("Copied content: {}", task.title));
        }
    }

    pub(crate) fn open_in_editor(&mut self) {
        if let Some(task) = self.active_task() {
            if task.file.is_empty() {
                self.set_status("No file path");
                return;
            }
            let file = task.file.clone();

            // Detect the current editor context and open the file appropriately.
            // For GUI editors (VS Code, Cursor, Zed, etc.) we can spawn the editor
            // without suspending the TUI since they open in a separate window/tab.
            if let Some(cmd) = Self::detect_gui_editor() {
                use std::process::{Command, Stdio};
                let result = Command::new(&cmd)
                    .arg(&file)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn();
                match result {
                    Ok(_) => self.set_status(format!("Opened in {cmd}")),
                    Err(e) => self.set_status(format!("Failed to open in {cmd}: {e}")),
                }
            } else {
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                self.set_status(format!("Run: {} {}", editor, file));
            }
        }
    }

    // ── File-reader clipboard / editor actions ────────────────────

    pub(crate) fn copy_file_content(&mut self) {
        if let Some(ref fv) = self.file_view {
            let text = fv.body.clone();
            let title = fv.title.clone();
            Self::copy_to_clipboard(&text);
            self.set_status(format!("Copied content: {}", title));
        }
    }

    pub(crate) fn copy_file_path(&mut self) {
        if let Some(ref fv) = self.file_view {
            let path = fv.path.clone();
            Self::copy_to_clipboard(&path);
            self.set_status(format!("Copied: {}", path));
        }
    }

    pub(crate) fn open_file_in_editor(&mut self) {
        if let Some(ref fv) = self.file_view {
            let file = fv.path.clone();
            if let Some(cmd) = Self::detect_gui_editor() {
                use std::process::{Command, Stdio};
                let result = Command::new(&cmd)
                    .arg(&file)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn();
                match result {
                    Ok(_) => self.set_status(format!("Opened in {cmd}")),
                    Err(e) => self.set_status(format!("Failed to open in {cmd}: {e}")),
                }
            } else {
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                self.set_status(format!("Run: {} {}", editor, file));
            }
        }
    }

    /// Detect a GUI editor from the current terminal environment.
    /// Returns the CLI command to use, or `None` if no GUI editor is detected.
    fn detect_gui_editor() -> Option<String> {
        // Check TERM_PROGRAM first — set by the terminal emulator / integrated terminal.
        if let Ok(term) = std::env::var("TERM_PROGRAM") {
            match term.to_lowercase().as_str() {
                "vscode" => return Some("code".to_string()),
                "cursor" => return Some("cursor".to_string()),
                _ => {}
            }
        }

        // Check VISUAL — convention for GUI-capable editors.
        if let Ok(visual) = std::env::var("VISUAL") {
            let cmd = visual.split_whitespace().next().unwrap_or("");
            // Only use VISUAL if it's a known GUI editor, not a terminal one.
            let base = std::path::Path::new(cmd)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(cmd);
            match base {
                "code" | "cursor" | "zed" | "subl" | "atom" | "mate" | "idea" | "webstorm"
                | "goland" | "rustrover" | "fleet" => return Some(visual),
                _ => {}
            }
        }

        None
    }
}
