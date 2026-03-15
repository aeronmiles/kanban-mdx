//! Guide view key handling: index (topic list) and page (markdown reader).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{delete_word_back, App, AppView, GuideMode};
use crate::tui::guide::content;

impl App {
    pub(crate) fn handle_guide_key(&mut self, key: KeyEvent) {
        match self.guide.mode {
            GuideMode::Index => self.handle_guide_index_key(key),
            GuideMode::Page => self.handle_guide_page_key(key),
        }
    }

    // ── Index mode ───────────────────────────────────────────────────

    fn handle_guide_index_key(&mut self, key: KeyEvent) {
        if self.guide.topic_filter_active {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('w') => {
                        delete_word_back(&mut self.guide.topic_filter);
                        self.guide.topic_cursor = 0;
                    }
                    KeyCode::Char('u') => {
                        self.guide.topic_filter.clear();
                        self.guide.topic_cursor = 0;
                    }
                    _ => {}
                }
                return;
            }
            match key.code {
                KeyCode::Esc => {
                    self.guide.topic_filter.clear();
                    self.guide.topic_filter_active = false;
                    self.guide.topic_cursor = 0;
                }
                KeyCode::Enter => {
                    self.guide.topic_filter_active = false;
                }
                KeyCode::Backspace => {
                    if key.modifiers.contains(KeyModifiers::SUPER) {
                        self.guide.topic_filter.clear();
                    } else if key.modifiers.contains(KeyModifiers::ALT) {
                        delete_word_back(&mut self.guide.topic_filter);
                    } else {
                        self.guide.topic_filter.pop();
                    }
                    self.guide.topic_cursor = 0;
                }
                KeyCode::Char(c) => {
                    self.guide.topic_filter.push(c);
                    self.guide.topic_cursor = 0;
                }
                _ => {}
            }
            return;
        }

        let filtered = content::filtered_indices(&self.guide.topic_filter);
        let count = filtered.len();

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.guide.topic_filter.clear();
                self.guide.topic_filter_active = false;
                self.guide.topic_cursor = 0;
                self.view = AppView::Board;
            }
            KeyCode::Char('/') => {
                self.guide.topic_filter_active = true;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if count > 0 {
                    self.guide.topic_cursor = (self.guide.topic_cursor + 1).min(count - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.guide.topic_cursor = self.guide.topic_cursor.saturating_sub(1);
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.guide.topic_cursor = 0;
            }
            KeyCode::Char('G') | KeyCode::End => {
                if count > 0 {
                    self.guide.topic_cursor = count - 1;
                }
            }
            KeyCode::Enter => {
                if let Some(&topic_idx) = filtered.get(self.guide.topic_cursor) {
                    self.guide.mode = GuideMode::Page;
                    self.guide.scroll = 0;
                    self.guide.cache = None;
                    self.guide.fold_level = 0;
                    self.guide.find_query.clear();
                    self.guide.find_active = false;
                    self.guide.find_matches.clear();
                    self.guide.find_current = 0;
                    // Store actual topic index for rendering.
                    self.guide.topic_cursor = topic_idx;
                }
            }
            KeyCode::Char('O') => {
                self.open_file_picker();
            }
            _ => {}
        }
    }

    // ── Page mode ────────────────────────────────────────────────────

    fn handle_guide_page_key(&mut self, key: KeyEvent) {
        // Find input mode.
        if self.guide.find_active {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('w') => {
                        delete_word_back(&mut self.guide.find_query);
                        self.guide_recompute_find_matches();
                    }
                    KeyCode::Char('u') => {
                        self.guide.find_query.clear();
                        self.guide_recompute_find_matches();
                    }
                    KeyCode::Char('n') => {
                        self.guide_find_next();
                    }
                    KeyCode::Char('p') => {
                        self.guide_find_prev();
                    }
                    _ => {}
                }
                return;
            }
            match key.code {
                KeyCode::Esc => {
                    self.guide.find_active = false;
                    self.guide.find_query.clear();
                    self.guide.find_matches.clear();
                    self.guide.find_current = 0;
                }
                KeyCode::Enter => {
                    self.guide.find_active = false;
                }
                KeyCode::Backspace => {
                    if key.modifiers.contains(KeyModifiers::SUPER) {
                        self.guide.find_query.clear();
                    } else if key.modifiers.contains(KeyModifiers::ALT) {
                        delete_word_back(&mut self.guide.find_query);
                    } else {
                        self.guide.find_query.pop();
                    }
                    self.guide_recompute_find_matches();
                }
                KeyCode::Char(c) => {
                    self.guide.find_query.push(c);
                    self.guide_recompute_find_matches();
                }
                _ => {}
            }
            return;
        }

        // Ctrl modifiers.
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('j') => {
                    self.guide.scroll += self.guide_page_size();
                }
                KeyCode::Char('k') => {
                    self.guide.scroll =
                        self.guide.scroll.saturating_sub(self.guide_page_size());
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => {
                // Back to index — restore topic_cursor to filtered position.
                self.guide.mode = GuideMode::Index;
                self.guide.find_query.clear();
                self.guide.find_matches.clear();
                self.guide.find_current = 0;
                self.guide.scroll = 0;
                self.guide.cache = None;
                // topic_cursor already holds the actual topic index; convert
                // back to filtered-list position for the index view.
                let filtered = content::filtered_indices(&self.guide.topic_filter);
                self.guide.topic_cursor = filtered
                    .iter()
                    .position(|&i| i == self.guide.topic_cursor)
                    .unwrap_or(0);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.guide.scroll += 1;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.guide.scroll = self.guide.scroll.saturating_sub(1);
            }
            KeyCode::Char(']') => {
                self.guide.scroll += 3;
            }
            KeyCode::Char('[') => {
                self.guide.scroll = self.guide.scroll.saturating_sub(3);
            }
            KeyCode::Char('J') | KeyCode::Char('d') => {
                self.guide.scroll += self.guide_page_size() / 2;
            }
            KeyCode::Char('K') | KeyCode::Char('u') => {
                self.guide.scroll = self
                    .guide
                    .scroll
                    .saturating_sub(self.guide_page_size() / 2);
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.guide.scroll = 0;
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.guide.scroll = usize::MAX / 2;
            }
            KeyCode::Char('}') => {
                self.guide_next_heading();
            }
            KeyCode::Char('{') => {
                self.guide_prev_heading();
            }
            KeyCode::Char('\'') => {
                self.guide_next_heading_level(2);
            }
            KeyCode::Char('"') => {
                self.guide_prev_heading_level(2);
            }
            KeyCode::Char(c @ '1'..='9') => {
                let index = (c as usize) - ('1' as usize);
                self.guide_goto_heading_index(index);
            }
            KeyCode::Char('/') => {
                self.guide.find_active = true;
                self.guide.find_query.clear();
                self.guide.find_matches.clear();
                self.guide.find_current = 0;
            }
            KeyCode::Char('n') => {
                self.guide_find_next();
            }
            KeyCode::Char('N') => {
                self.guide_find_prev();
            }
            KeyCode::Char('z') => {
                self.guide_fold_deeper();
            }
            KeyCode::Char('Z') => {
                self.guide_fold_shallower();
            }
            KeyCode::Char('>') => {
                self.adjust_reader_max_width(10);
                self.guide.cache = None;
            }
            KeyCode::Char('<') => {
                self.adjust_reader_max_width(-10);
                self.guide.cache = None;
            }
            KeyCode::Char('O') => {
                self.open_file_picker();
            }
            _ => {}
        }
    }

    // ── Guide navigation helpers ─────────────────────────────────────

    fn guide_next_heading(&mut self) {
        if let Some(ref cache) = self.guide.cache {
            for &off in &cache.heading_offsets_any {
                if off > self.guide.scroll {
                    self.guide.scroll = off;
                    return;
                }
            }
        }
    }

    fn guide_prev_heading(&mut self) {
        if let Some(ref cache) = self.guide.cache {
            let mut target = None;
            for &off in &cache.heading_offsets_any {
                if off >= self.guide.scroll {
                    break;
                }
                target = Some(off);
            }
            if let Some(t) = target {
                self.guide.scroll = t;
            }
        }
    }

    fn guide_next_heading_level(&mut self, level: usize) {
        let offsets = match self.guide.cache.as_ref() {
            Some(c) if level == 2 => c.heading_offsets_l2.clone(),
            Some(c) => c.heading_offsets_any.clone(),
            None => return,
        };
        for &off in &offsets {
            if off > self.guide.scroll {
                self.guide.scroll = off;
                return;
            }
        }
    }

    fn guide_prev_heading_level(&mut self, level: usize) {
        let offsets = match self.guide.cache.as_ref() {
            Some(c) if level == 2 => c.heading_offsets_l2.clone(),
            Some(c) => c.heading_offsets_any.clone(),
            None => return,
        };
        let mut target = None;
        for &off in &offsets {
            if off >= self.guide.scroll {
                break;
            }
            target = Some(off);
        }
        if let Some(t) = target {
            self.guide.scroll = t;
        }
    }

    fn guide_goto_heading_index(&mut self, index: usize) {
        if let Some(ref cache) = self.guide.cache {
            if let Some(&off) = cache.heading_offsets_l2.get(index) {
                self.guide.scroll = off;
            }
        }
    }

    fn guide_fold_deeper(&mut self) {
        match self.guide.fold_level {
            0 => self.guide.fold_level = 3,
            3 => self.guide.fold_level = 2,
            _ => {}
        }
        self.guide.cache = None; // invalidate to rebuild with new fold level
        self.set_status(format!("Fold level: h{}", self.guide.fold_level));
    }

    fn guide_fold_shallower(&mut self) {
        match self.guide.fold_level {
            2 => self.guide.fold_level = 3,
            3 => self.guide.fold_level = 0,
            _ => {}
        }
        self.guide.cache = None;
        self.set_status(format!(
            "Fold level: {}",
            if self.guide.fold_level == 0 {
                "off".to_string()
            } else {
                format!("h{}", self.guide.fold_level)
            }
        ));
    }

    fn guide_recompute_find_matches(&mut self) {
        self.guide.find_matches.clear();
        self.guide.find_current = 0;
        if self.guide.find_query.is_empty() {
            return;
        }
        if let Some(ref cache) = self.guide.cache {
            let query = self.guide.find_query.to_lowercase();
            for (i, text) in cache.line_texts.iter().enumerate() {
                if text.to_lowercase().contains(&query) {
                    self.guide.find_matches.push(i);
                }
            }
        }
    }

    fn guide_find_next(&mut self) {
        if self.guide.find_matches.is_empty() {
            return;
        }
        self.guide.find_current =
            (self.guide.find_current + 1) % self.guide.find_matches.len();
        self.guide_scroll_to_find_match();
    }

    fn guide_find_prev(&mut self) {
        if self.guide.find_matches.is_empty() {
            return;
        }
        if self.guide.find_current == 0 {
            self.guide.find_current = self.guide.find_matches.len() - 1;
        } else {
            self.guide.find_current -= 1;
        }
        self.guide_scroll_to_find_match();
    }

    fn guide_scroll_to_find_match(&mut self) {
        if let Some(&line_idx) = self.guide.find_matches.get(self.guide.find_current) {
            if let Some(ref cache) = self.guide.cache {
                let vrow = cache.content.line_to_vrow(line_idx);
                let half_page = self.guide_page_size() / 2;
                self.guide.scroll = vrow.saturating_sub(half_page);
            }
        }
    }
}
