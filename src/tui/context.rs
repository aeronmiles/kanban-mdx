//! Context picker — branch/context selection, assignment, and git operations.

use chrono::Utc;

use super::app::App;
use super::types::{AppView, ContextItem, ContextKind, ContextPickerMode};
use crate::model::task::Task;

impl App {
    pub(crate) fn assign_branch_to_task(&mut self, branch: &str) {
        let mut msg = None;
        if let Some(task) = self
            .columns
            .get_mut(self.active_col)
            .and_then(|c| c.tasks.get_mut(self.active_row))
        {
            task.branch = branch.to_string();
            task.updated = Utc::now();
            msg = Some(format!("#{} branch: {}", task.id, branch));
        }
        if let Some(m) = msg {
            self.set_status(m);
        }
        self.persist_task(self.active_col, self.active_row);
    }

    pub(crate) fn clear_branch_from_task(&mut self) {
        let mut msg = None;
        if let Some(task) = self
            .columns
            .get_mut(self.active_col)
            .and_then(|c| c.tasks.get_mut(self.active_row))
        {
            task.branch.clear();
            task.updated = Utc::now();
            msg = Some(format!("#{} branch cleared", task.id));
        }
        if let Some(m) = msg {
            self.set_status(m);
        }
        self.persist_task(self.active_col, self.active_row);
    }

    // ── Context Picker ──────────────────────────────────────────────

    pub(crate) fn open_context_picker(&mut self, worktree_only: bool) {
        self.picker.context_worktree_only = worktree_only;
        self.picker.context_picker_mode = ContextPickerMode::SwitchContext;
        self.build_context_items();
        self.picker.context_cursor = 0;
        self.picker.context_filter.clear();
        self.view = AppView::ContextPicker;
    }

    pub(crate) fn open_assign_context(&mut self, worktree_only: bool) {
        self.picker.context_worktree_only = worktree_only;
        self.picker.context_picker_mode = ContextPickerMode::AssignBranch;
        self.build_context_items();
        self.picker.context_cursor = 0;
        self.picker.context_filter.clear();

        // Pre-select current branch.
        if let Some(task) = self.active_task() {
            let branch = task.branch.clone();
            if !branch.is_empty() {
                let filtered = self.filtered_context_items();
                for (i, item) in filtered.iter().enumerate() {
                    if item.branch == branch {
                        self.picker.context_cursor = i;
                        break;
                    }
                }
            }
        }
        self.view = AppView::ContextPicker;
    }

    pub(crate) fn build_context_items(&mut self) {
        let mut items = Vec::new();
        let git_branches = crate::util::git::local_branches();

        // Meta-options depend on picker mode.
        if self.picker.context_picker_mode == ContextPickerMode::SwitchContext {
            items.push(ContextItem {
                kind: ContextKind::Auto,
                task_id: None,
                branch: String::new(),
                label: "Auto-detect (current branch)".to_string(),
                missing: false,
            });
            items.push(ContextItem {
                kind: ContextKind::Clear,
                task_id: None,
                branch: String::new(),
                label: "Clear context".to_string(),
                missing: false,
            });
        } else {
            items.push(ContextItem {
                kind: ContextKind::Clear,
                task_id: None,
                branch: String::new(),
                label: "Clear branch".to_string(),
                missing: false,
            });
        }

        // Collect worktree branches if restricting to worktrees.
        let worktree_branches: std::collections::HashSet<String> =
            if self.picker.context_worktree_only {
                crate::util::git::list_worktree_branches()
                    .into_iter()
                    .collect()
            } else {
                std::collections::HashSet::new()
            };

        // Add task branches (deduped).
        let mut seen = std::collections::HashSet::new();
        let all_tasks: Vec<&Task> = self
            .columns
            .iter()
            .flat_map(|c| c.tasks.iter())
            .collect();
        for task in &all_tasks {
            if task.branch.is_empty() {
                continue;
            }
            if self.picker.context_worktree_only && !worktree_branches.contains(&task.branch) {
                continue;
            }
            if !seen.insert(task.branch.clone()) {
                continue;
            }
            let missing = !git_branches.contains(&task.branch);
            items.push(ContextItem {
                kind: ContextKind::Task,
                task_id: Some(task.id),
                branch: task.branch.clone(),
                label: format!("#{} {}", task.id, task.branch),
                missing,
            });
        }

        // Add orphaned git branches (branches with no task).
        let mut branch_list: Vec<String> = if self.picker.context_worktree_only {
            worktree_branches.iter().cloned().collect()
        } else {
            git_branches.iter().cloned().collect()
        };
        branch_list.sort();
        for branch in &branch_list {
            if !seen.insert(branch.clone()) {
                continue;
            }
            items.push(ContextItem {
                kind: ContextKind::Branch,
                task_id: None,
                branch: branch.clone(),
                label: branch.clone(),
                missing: false,
            });
        }

        self.picker.context_items = items;
    }

    pub fn filtered_context_items(&self) -> Vec<&ContextItem> {
        if self.picker.context_filter.is_empty() {
            self.picker.context_items.iter().collect()
        } else {
            let q = self.picker.context_filter.to_lowercase();
            self.picker.context_items
                .iter()
                .filter(|item| {
                    // Always show Auto and Clear.
                    matches!(item.kind, ContextKind::Auto | ContextKind::Clear)
                        || item.label.to_lowercase().contains(&q)
                        || item.branch.to_lowercase().contains(&q)
                })
                .collect()
        }
    }

    pub(crate) fn maybe_add_create_item(&mut self) {
        // Remove any existing New items.
        self.picker.context_items
            .retain(|item| item.kind != ContextKind::New);

        let filter = self.picker.context_filter.trim().to_string();
        if filter.is_empty() {
            return;
        }

        // Check if filter matches any existing branch exactly.
        let has_exact = self.picker.context_items.iter().any(|item| item.branch == filter);
        if !has_exact {
            self.picker.context_items.push(ContextItem {
                kind: ContextKind::New,
                task_id: None,
                branch: filter.clone(),
                label: format!("Create: {}", filter),
                missing: false,
            });
        }
    }

    pub(crate) fn execute_context_select(&mut self, item: &ContextItem) {
        match item.kind {
            ContextKind::Clear => {
                self.picker.context_mode = false;
                self.picker.context_task_id = 0;
                self.picker.context_label.clear();
                self.set_status("Context cleared".to_string());
                self.view = AppView::Board;
            }
            ContextKind::Auto => {
                self.picker.context_mode = true;
                self.picker.context_task_id = 0;
                self.picker.context_label.clear();
                self.set_status("Context: auto-detect".to_string());
                self.view = AppView::Board;
            }
            ContextKind::New => {
                self.picker.confirm_branch_name = item.branch.clone();
                self.view = AppView::ConfirmBranch;
            }
            ContextKind::Task | ContextKind::Branch => {
                self.picker.context_mode = true;
                self.picker.context_task_id = item.task_id.unwrap_or(0);
                self.picker.context_label = item.branch.clone();
                self.set_status(format!("Context: {}", item.branch));
                self.view = AppView::Board;
            }
        }
    }

    pub(crate) fn execute_assign_context(&mut self, item: &ContextItem) {
        match item.kind {
            ContextKind::Clear => {
                self.record_branch_undo();
                self.clear_branch_from_task();
                self.complete_branch_undo();
                self.view = AppView::Board;
            }
            ContextKind::New => {
                self.picker.confirm_branch_name = item.branch.clone();
                self.view = AppView::ConfirmBranch;
            }
            _ => {
                self.record_branch_undo();
                let branch = item.branch.clone();
                self.assign_branch_to_task(&branch);
                self.complete_branch_undo();
                self.view = AppView::Board;
            }
        }
    }

    pub(crate) fn record_branch_undo(&mut self) {
        if let Some(task) = self.active_task() {
            let path = &task.file;
            if path.is_empty() {
                return;
            }
            let before = crate::board::undo::snapshot_file(std::path::Path::new(path));
            self.picker.pending_undo_before = Some((task.id, before));
        }
    }

    pub(crate) fn complete_branch_undo(&mut self) {
        if let Some((task_id, before)) = self.picker.pending_undo_before.take() {
            if let Some(task) = self
                .columns
                .get(self.active_col)
                .and_then(|c| c.tasks.get(self.active_row))
            {
                let after =
                    crate::board::undo::snapshot_file(std::path::Path::new(&task.file));
                let detail = format!("branch -> {}", task.branch);
                let entry = crate::board::undo::UndoEntry {
                    timestamp: Utc::now(),
                    action: "branch-assign".to_string(),
                    task_id,
                    detail,
                    files_before: vec![before],
                    files_after: vec![after],
                };
                let _ = crate::board::undo::record_undo(self.cfg.dir(), &entry);
            }
        }
    }

    pub(crate) fn create_branch_and_proceed(&mut self) {
        let name = self.picker.confirm_branch_name.clone();
        let git_branches = crate::util::git::local_branches();
        let branch_exists = git_branches.contains(&name);

        if self.picker.context_worktree_only {
            // Create worktree.
            let path = format!("../kb-{}", name);
            let result = if branch_exists {
                std::process::Command::new("git")
                    .args(["worktree", "add", &path, &name])
                    .output()
            } else {
                std::process::Command::new("git")
                    .args(["worktree", "add", &path, "-b", &name])
                    .output()
            };
            match result {
                Ok(out) if out.status.success() => {
                    self.set_status(format!("Created worktree: {}", path));
                }
                Ok(out) => {
                    let err = String::from_utf8_lossy(&out.stderr);
                    self.set_status(format!("Worktree error: {}", err.trim()));
                    self.view = AppView::Board;
                    return;
                }
                Err(e) => {
                    self.set_status(format!("Git error: {}", e));
                    self.view = AppView::Board;
                    return;
                }
            }
        } else if !branch_exists {
            // Create branch only.
            let result = std::process::Command::new("git")
                .args(["branch", &name])
                .output();
            match result {
                Ok(out) if out.status.success() => {
                    self.set_status(format!("Created branch: {}", name));
                }
                Ok(out) => {
                    let err = String::from_utf8_lossy(&out.stderr);
                    self.set_status(format!("Git error: {}", err.trim()));
                    self.view = AppView::Board;
                    return;
                }
                Err(e) => {
                    self.set_status(format!("Git error: {}", e));
                    self.view = AppView::Board;
                    return;
                }
            }
        }

        // Now proceed with the original action.
        let item = ContextItem {
            kind: ContextKind::Branch,
            task_id: None,
            branch: name,
            label: String::new(),
            missing: false,
        };

        match self.picker.context_picker_mode {
            ContextPickerMode::SwitchContext => self.execute_context_select(&item),
            ContextPickerMode::AssignBranch => {
                self.record_branch_undo();
                let branch = item.branch.clone();
                self.assign_branch_to_task(&branch);
                self.complete_branch_undo();
                self.view = AppView::Board;
            }
        }
    }
}
