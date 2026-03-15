//! Disk persistence — saving tasks, config, and TUI state.

use super::app::App;
use super::types::{SortMode, TimeMode, ViewMode};
use crate::model::task::Task;

impl App {
    // ── Disk persistence ────────────────────────────────────────────

    pub(crate) fn persist_task(&mut self, col_idx: usize, row_idx: usize) {
        if let Some(task) = self.columns.get(col_idx).and_then(|c| c.tasks.get(row_idx)) {
            if task.file.is_empty() {
                return;
            }
            let path = std::path::Path::new(&task.file);
            if let Err(e) = crate::io::task_file::write(path, task) {
                self.set_status(format!("Save error: {}", e));
            }
        }
    }

    /// Sync collapsed column state to config and save to disk.
    pub(crate) fn persist_collapsed(&mut self) {
        self.cfg.tui.collapsed_columns = self
            .columns
            .iter()
            .filter(|c| c.collapsed)
            .map(|c| c.name.clone())
            .collect();
        if let Err(e) = crate::io::config_file::save(&self.cfg) {
            self.set_status(format!("Config save error: {}", e));
        }
    }

    /// Sync sort_mode, time_mode, list_mode, theme, brightness, and saturation
    /// to config and save to disk.
    pub(crate) fn persist_tui_state(&mut self) {
        if self.is_file_reader() {
            return;
        }
        self.cfg.tui.sort_mode = match self.sort_mode {
            SortMode::ByPriority => 0,
            SortMode::Newest => 1,
            SortMode::Oldest => 2,
            SortMode::CreatedNew => 3,
            SortMode::CreatedOld => 4,
        };
        self.cfg.tui.time_mode = match self.time_mode {
            TimeMode::Created => 0,
            TimeMode::Updated => 1,
        };
        self.cfg.tui.list_mode = matches!(self.view_mode, ViewMode::List);
        self.cfg.tui.theme = self.theme_kind.as_config_str().to_string();
        self.cfg.tui.brightness = self.brightness;
        self.cfg.tui.saturation = self.saturation;
        if let Err(e) = crate::io::config_file::save(&self.cfg) {
            self.set_status(format!("Config save error: {}", e));
        }
    }

    pub(crate) fn adjust_reader_max_width(&mut self, delta: i16) {
        if self.is_file_reader() {
            // Adjust in memory only — no config file to persist.
            let current = self.reader_max_width as i16;
            let new_val = (current + delta).max(30).min(200);
            self.reader_max_width = new_val as u16;
            self.set_status(format!("Detail width: {}", self.reader_max_width));
            return;
        }
        let current = self.reader_max_width as i16;
        let new_val = (current + delta).max(30).min(200);
        self.reader_max_width = new_val as u16;
        self.cfg.tui.reader_max_width = self.reader_max_width as i32;
        if let Err(e) = crate::io::config_file::save(&self.cfg) {
            self.set_status(format!("Config save error: {}", e));
        } else {
            self.set_status(format!("Detail width: {}", self.reader_max_width));
        }
    }

    pub(crate) fn adjust_reader_width_pct(&mut self, delta: i16) {
        let current = self.reader_width_pct as i16;
        let new_val = (current + delta).clamp(10, 90);
        self.reader_width_pct = new_val as u16;
        self.cfg.tui.reader_width_pct = self.reader_width_pct as i32;
        if let Err(e) = crate::io::config_file::save(&self.cfg) {
            self.set_status(format!("Config save error: {}", e));
        } else {
            self.set_status(format!("Reader width: {}%", self.reader_width_pct));
        }
    }

    pub(crate) fn persist_deleted_task(&mut self, task: &Task) {
        if task.file.is_empty() {
            return;
        }
        let path = std::path::Path::new(&task.file);
        if let Err(e) = crate::io::task_file::write(path, task) {
            self.set_status(format!("Save error: {}", e));
        }
    }
}
