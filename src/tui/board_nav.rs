//! Board navigation — column/row movement, filtering, sorting.

use super::app::App;
use super::types::{Column, SortMode, priority_sort_key, task_age_hours, task_created_age_hours};

impl App {
    // ── Navigation helpers ──────────────────────────────────────────

    /// Save `active_row` into the current column, switch `active_col` to
    /// `new_col`, and restore that column's saved cursor.  Every column
    /// switch must go through this so per-column cursors stay in sync.
    pub(crate) fn switch_col(&mut self, new_col: usize) {
        // Save current cursor into the column we're leaving.
        if let Some(col) = self.columns.get_mut(self.active_col) {
            col.active_row = self.active_row;
        }
        self.active_col = new_col;
        // Restore saved cursor from the column we're entering.
        self.active_row = self
            .columns
            .get(self.active_col)
            .map(|c| c.active_row)
            .unwrap_or(0);
        self.clamp_active_row();
    }

    pub(crate) fn move_col_left(&mut self) {
        if self.columns.is_empty() {
            return;
        }
        let len = self.columns.len();
        let mut col = self.active_col;
        for _ in 0..len {
            col = if col == 0 { len - 1 } else { col - 1 };
            if !self.columns[col].collapsed {
                self.switch_col(col);
                return;
            }
        }
    }

    pub(crate) fn move_col_right(&mut self) {
        if self.columns.is_empty() {
            return;
        }
        let len = self.columns.len();
        let mut col = self.active_col;
        for _ in 0..len {
            col = if col + 1 >= len { 0 } else { col + 1 };
            if !self.columns[col].collapsed {
                self.switch_col(col);
                return;
            }
        }
    }

    /// When exactly one column is expanded (solo mode), cycle the solo to
    /// the previous column, preserving the layout. Otherwise normal nav.
    pub(crate) fn cycle_or_move_col_left(&mut self) {
        let expanded = self.columns.iter().filter(|c| !c.collapsed).count();
        if expanded == 1 {
            let len = self.columns.len();
            let prev = if self.active_col == 0 { len - 1 } else { self.active_col - 1 };
            for (i, col) in self.columns.iter_mut().enumerate() {
                col.collapsed = i != prev;
            }
            self.switch_col(prev);
            self.persist_collapsed();
        } else {
            self.move_col_left();
        }
    }

    /// When exactly one column is expanded (solo mode), cycle the solo to
    /// the next column, preserving the layout. Otherwise normal nav.
    pub(crate) fn cycle_or_move_col_right(&mut self) {
        let expanded = self.columns.iter().filter(|c| !c.collapsed).count();
        if expanded == 1 {
            let len = self.columns.len();
            let next = if self.active_col + 1 >= len { 0 } else { self.active_col + 1 };
            for (i, col) in self.columns.iter_mut().enumerate() {
                col.collapsed = i != next;
            }
            self.switch_col(next);
            self.persist_collapsed();
        } else {
            self.move_col_right();
        }
    }

    /// Returns indices (into `col.tasks`) of tasks visible under current
    /// filters, or `None` when no filter is active (all tasks visible).
    pub(crate) fn active_col_filtered_indices(&self) -> Option<Vec<usize>> {
        if self.search.query.is_empty() && !self.worktree_filter_active && !self.picker.context_mode {
            return None;
        }
        let col = self.columns.get(self.active_col)?;
        let context_ids = self.compute_context_ids();
        let filtered = Self::filtered_tasks(
            col,
            &self.search.query,
            self.worktree_filter_active,
            &self.search.sem_scores,
            &context_ids,
            self.time_mode.label(),
        );
        // Map filtered tasks back to col.tasks indices, preserving the
        // filtered order (which may differ from original — e.g. semantic
        // ranking).  Navigation must follow this order so that j/k moves
        // through tasks in the same sequence the user sees on screen.
        let indices: Vec<usize> = filtered
            .iter()
            .filter_map(|ft| col.tasks.iter().position(|t| t.id == ft.id))
            .collect();
        Some(indices)
    }

    pub(crate) fn move_row_down(&mut self, amount: usize) {
        if let Some(indices) = self.active_col_filtered_indices() {
            if indices.is_empty() {
                return;
            }
            let cur_pos = indices.iter().position(|&i| i == self.active_row);
            match cur_pos {
                Some(p) => {
                    self.active_row = indices[(p + amount).min(indices.len() - 1)];
                }
                None => {
                    // Not on a visible task — snap to next visible one downward.
                    let next = indices
                        .iter()
                        .position(|&i| i > self.active_row)
                        .unwrap_or(0);
                    self.active_row = indices[next];
                }
            }
            return;
        }
        let last = self.last_row_index();
        self.active_row = (self.active_row + amount).min(last);
    }

    pub(crate) fn move_row_up(&mut self, amount: usize) {
        if let Some(indices) = self.active_col_filtered_indices() {
            if indices.is_empty() {
                return;
            }
            let cur_pos = indices.iter().position(|&i| i == self.active_row);
            match cur_pos {
                Some(p) => {
                    self.active_row = indices[p.saturating_sub(amount)];
                }
                None => {
                    // Not on a visible task — snap to previous visible one.
                    let prev = indices
                        .iter()
                        .rposition(|&i| i < self.active_row)
                        .unwrap_or(indices.len() - 1);
                    self.active_row = indices[prev];
                }
            }
            return;
        }
        self.active_row = self.active_row.saturating_sub(amount);
    }

    pub(crate) fn last_row_index(&self) -> usize {
        if let Some(indices) = self.active_col_filtered_indices() {
            return indices.last().copied().unwrap_or(0);
        }
        self.columns
            .get(self.active_col)
            .map(|c| c.tasks.len().saturating_sub(1))
            .unwrap_or(0)
    }

    pub(crate) fn clamp_active_row(&mut self) {
        if let Some(indices) = self.active_col_filtered_indices() {
            if indices.is_empty() {
                self.active_row = 0;
                return;
            }
            if !indices.contains(&self.active_row) {
                // Snap to nearest visible task.
                self.active_row = *indices
                    .iter()
                    .min_by_key(|&&i| (i as isize - self.active_row as isize).unsigned_abs())
                    .unwrap();
            }
            return;
        }
        let last = self.last_row_index();
        if self.active_row > last {
            self.active_row = last;
        }
    }

    pub(crate) fn visible_height(&self) -> usize {
        if let Some(indices) = self.active_col_filtered_indices() {
            return indices.len().max(2);
        }
        self.columns
            .get(self.active_col)
            .map(|c| c.tasks.len())
            .unwrap_or(10)
            .max(2)
    }

    // ── Sort ────────────────────────────────────────────────────────

    pub(crate) fn sort_all_columns(&mut self) {
        for col in &mut self.columns {
            Self::sort_column(col, self.sort_mode);
        }
    }

    pub(crate) fn sort_column(col: &mut Column, mode: SortMode) {
        match mode {
            SortMode::ByPriority => {
                col.tasks.sort_by_key(|t| priority_sort_key(&t.priority));
            }
            SortMode::Newest => {
                col.tasks.sort_by_key(|t| task_age_hours(t));
            }
            SortMode::Oldest => {
                col.tasks
                    .sort_by(|a, b| task_age_hours(b).cmp(&task_age_hours(a)));
            }
            SortMode::CreatedNew => {
                col.tasks.sort_by_key(|t| task_created_age_hours(t));
            }
            SortMode::CreatedOld => {
                col.tasks
                    .sort_by(|a, b| task_created_age_hours(b).cmp(&task_created_age_hours(a)));
            }
        }
    }
}
