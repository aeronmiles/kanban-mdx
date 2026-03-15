//! File picker — browse directories and open `.md` files in the reader view.

use super::app::App;
use super::types::{AppView, FilePickerEntry, FileView};

impl App {
    /// Open the file picker overlay, starting at the current directory.
    /// Remembers the current view so Esc returns to it.
    pub(crate) fn open_file_picker(&mut self) {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        self.file_picker.return_view = self.view;
        self.file_picker.cwd = cwd;
        self.file_picker.cursor = 0;
        self.file_picker.filter.clear();
        self.file_picker.path_input_active = false;
        self.file_picker.path_input.clear();
        self.file_picker.tab_completions.clear();
        self.file_picker.tab_idx = 0;
        self.file_picker.tab_prefix = None;
        self.scan_file_picker_dir();
        self.view = AppView::FilePicker;
    }

    /// Re-scan the current directory and populate entries.
    pub(crate) fn scan_file_picker_dir(&mut self) {
        self.file_picker.entries.clear();
        self.file_picker.cursor = 0;
        self.file_picker.filter.clear();

        let read_dir = match std::fs::read_dir(&self.file_picker.cwd) {
            Ok(rd) => rd,
            Err(_) => return,
        };

        let mut dirs: Vec<FilePickerEntry> = Vec::new();
        let mut files: Vec<FilePickerEntry> = Vec::new();

        for entry in read_dir.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files.
            if name.starts_with('.') {
                continue;
            }

            if path.is_dir() {
                dirs.push(FilePickerEntry {
                    name: format!("{}/", name),
                    path,
                    is_dir: true,
                });
            } else if name.ends_with(".md") || name.ends_with(".mdx") {
                files.push(FilePickerEntry {
                    name,
                    path,
                    is_dir: false,
                });
            }
        }

        dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        self.file_picker.entries.extend(dirs);
        self.file_picker.entries.extend(files);
    }

    /// Return entries filtered by the current type-to-filter string.
    pub fn filtered_file_entries(&self) -> Vec<&FilePickerEntry> {
        if self.file_picker.filter.is_empty() {
            self.file_picker.entries.iter().collect()
        } else {
            let q = self.file_picker.filter.to_lowercase();
            self.file_picker
                .entries
                .iter()
                .filter(|e| e.name.to_lowercase().contains(&q))
                .collect()
        }
    }

    /// Open a markdown file from the file picker into the detail view.
    pub(crate) fn open_file_entry(&mut self, entry_path: std::path::PathBuf) {
        let body = match std::fs::read_to_string(&entry_path) {
            Ok(b) => b,
            Err(e) => {
                self.set_status(format!("Read error: {}", e));
                return;
            }
        };

        let title = entry_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        self.file_view = Some(FileView {
            path: entry_path.display().to_string(),
            title,
            body,
            standalone: false,
        });
        self.detail.scroll = 0;
        self.detail.cache = None;
        self.detail.heading_cache = None;
        self.detail.find_query.clear();
        self.detail.find_matches.clear();
        self.detail.find_current = 0;
        self.view = AppView::Detail;
    }

    /// Compute tab-completion candidates for the path input.
    ///
    /// Candidates include directories (with trailing `/`) and `.md`/`.mdx` files.
    /// If the input ends with `/`, lists contents of that directory.
    /// Otherwise, completes the partial filename within its parent directory.
    pub(crate) fn compute_path_completions(&mut self) {
        self.file_picker.tab_completions.clear();

        let input = &self.file_picker.path_input;
        if input.is_empty() {
            return;
        }

        let path = std::path::Path::new(input);
        let (dir, prefix) = if input.ends_with('/') || input.ends_with(std::path::MAIN_SEPARATOR) {
            (std::path::PathBuf::from(input), String::new())
        } else {
            let parent = path.parent().unwrap_or(std::path::Path::new("."));
            let file_prefix = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            (parent.to_path_buf(), file_prefix)
        };

        let read_dir = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(_) => return,
        };

        let mut dirs: Vec<String> = Vec::new();
        let mut files: Vec<String> = Vec::new();

        for entry in read_dir.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if !prefix.is_empty() && !name.to_lowercase().starts_with(&prefix) {
                continue;
            }
            let full_path = dir.join(&name);
            if full_path.is_dir() {
                dirs.push(format!("{}/", full_path.display()));
            } else {
                let lower = name.to_lowercase();
                if lower.ends_with(".md") || lower.ends_with(".mdx") {
                    files.push(full_path.display().to_string());
                }
            }
        }

        dirs.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        files.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

        // Directories first, then markdown files.
        self.file_picker.tab_completions.extend(dirs);
        self.file_picker.tab_completions.extend(files);
    }
}
