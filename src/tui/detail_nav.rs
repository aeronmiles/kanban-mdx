//! Detail navigation — heading offsets, find-in-detail, fold, jump list.

use super::app::App;
use super::jump::JumpEntry;
use super::types::{AppView, HeadingOffsetsCache, THEME_QUANTIZE};
use crate::model::task::Task;

impl App {
    // ── Heading offsets ──────────────────────────────────────────────

    /// Return cached heading offsets, recomputing only when task/theme/width changes.
    ///
    /// Scans the actual rendered detail lines (from `build_detail_lines`) so
    /// heading positions always match what is displayed — no separate markdown
    /// parse, no fragile metadata-line counting.
    ///
    /// Offsets are returned as **visual row** positions (accounting for line
    /// wrapping at `content_width`) so they can be used directly as scroll
    /// values with `Paragraph::scroll()`.
    ///
    /// When `exact_level` is `None` (any heading), stacked headings are
    /// collapsed (matching Go's `detailHeadingOffsets`).
    /// When `exact_level` is `Some(n)`, every heading at that level is
    /// returned without collapsing (`headingOffsetsForLevel`).
    pub(crate) fn heading_offsets(
        &mut self,
        task: &Task,
        exact_level: Option<usize>,
        content_width: u16,
    ) -> Vec<usize> {
        let bq = (self.brightness * THEME_QUANTIZE) as i32;
        let sq = (self.saturation * THEME_QUANTIZE) as i32;

        // Check cache.
        if let Some(ref entry) = self.detail.heading_cache {
            if entry.task_id == task.id as u32
                && entry.body == task.body
                && entry.theme == self.theme_kind
                && entry.brightness_q == bq
                && entry.saturation_q == sq
                && entry.content_width == content_width
                && entry.fold_level == self.detail.fold_level
            {
                return match exact_level {
                    None => entry.offsets_any.clone(),
                    Some(2) => entry.offsets_l2.clone(),
                    Some(level) => Self::compute_level_offsets(
                        &entry.body_line_texts,
                        entry.meta_count,
                        level,
                    ),
                };
            }
        }

        // Cache miss — scan the actual rendered detail lines.
        let rendered = super::render::build_detail_lines(self, task, content_width);

        // Collect line texts and identify headings directly from the rendered output.
        let mut line_texts: Vec<String> = Vec::with_capacity(rendered.lines.len());
        let mut all_headings: Vec<usize> = Vec::new();
        let mut l2_headings: Vec<usize> = Vec::new();

        for (i, line) in rendered.lines.iter().enumerate() {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            let is_code = line.style.bg.is_some();

            if !is_code {
                if let Some(first_span) = line.spans.first() {
                    let content = first_span.content.as_ref();
                    if content.starts_with('#') {
                        all_headings.push(i);
                        // Levels 1-2: matches "# " and "## " but not "### ".
                        if !content.starts_with("###") {
                            l2_headings.push(i);
                        }
                    }
                }
            }

            line_texts.push(text);
        }

        // Collapse stacked any-level headings (consecutive headings with no
        // non-blank content between them — keep only the first of each group).
        let collapsed_line_indices = Self::collapse_rendered_headings(&all_headings, &line_texts);

        // Convert line indices to visual row offsets (accounting for wrapping).
        let offsets_any: Vec<usize> = collapsed_line_indices
            .iter()
            .map(|&idx| rendered.line_to_vrow(idx))
            .collect();
        let offsets_l2: Vec<usize> = l2_headings
            .iter()
            .map(|&idx| rendered.line_to_vrow(idx))
            .collect();

        // Determine where the body starts (for find-match and level computation).
        // The body starts after the "───" separator line in the metadata block.
        let meta_count = line_texts
            .iter()
            .position(|t| t.starts_with('#'))
            .unwrap_or(0);

        // Body line texts = everything from meta_count onwards (for find matching).
        let body_line_texts: Vec<String> = line_texts[meta_count..].to_vec();

        // Store in cache.
        self.detail.heading_cache = Some(HeadingOffsetsCache {
            task_id: task.id as u32,
            body: task.body.clone(),
            theme: self.theme_kind,
            brightness_q: bq,
            saturation_q: sq,
            content_width,
            fold_level: self.detail.fold_level,
            offsets_any: offsets_any.clone(),
            offsets_l2: offsets_l2.clone(),
            body_line_texts,
            meta_count,
        });

        match exact_level {
            None => offsets_any,
            Some(2) => offsets_l2,
            Some(level) => {
                let line_indices = Self::compute_level_offsets(
                    &self.detail.heading_cache.as_ref().unwrap().body_line_texts,
                    meta_count,
                    level,
                );
                line_indices
                    .iter()
                    .map(|&idx| rendered.line_to_vrow(idx))
                    .collect()
            }
        }
    }

    /// Collapse stacked headings from absolute line indices: consecutive
    /// headings with only blank lines between them are grouped — keep only
    /// the first of each group.
    pub(crate) fn collapse_rendered_headings(headings: &[usize], line_texts: &[String]) -> Vec<usize> {
        if headings.len() <= 1 {
            return headings.to_vec();
        }
        let mut offsets = Vec::with_capacity(headings.len());
        offsets.push(headings[0]);
        for k in 1..headings.len() {
            let prev_idx = headings[k - 1];
            let cur_idx = headings[k];
            let has_content =
                (prev_idx + 1..cur_idx).any(|j| !line_texts[j].trim().is_empty());
            if has_content {
                offsets.push(cur_idx);
            }
        }
        offsets
    }

    /// Compute heading offsets up to (and including) the given level from body
    /// line texts.  E.g. `level=2` matches both `# ` and `## ` headings.
    /// `meta_count` is the offset of body lines within the full rendered view.
    pub(crate) fn compute_level_offsets(body_texts: &[String], meta_count: usize, level: usize) -> Vec<usize> {
        let max_prefix = "#".repeat(level);
        let deeper = format!("{}#", max_prefix);
        let mut offsets = Vec::new();
        for (i, text) in body_texts.iter().enumerate() {
            if text.starts_with('#') && !text.starts_with(&deeper) {
                offsets.push(meta_count + i);
            }
        }
        offsets
    }

    // ── Heading navigation ───────────────────────────────────────────

    pub(crate) fn reader_next_heading(&mut self) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.reader_content_width();
            let offsets = self.heading_offsets(&task, None, w);
            for &off in &offsets {
                if off > self.reader_scroll {
                    self.reader_scroll = off;
                    return;
                }
            }
        }
    }

    pub(crate) fn reader_prev_heading(&mut self) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.reader_content_width();
            let offsets = self.heading_offsets(&task, None, w);
            let mut target = None;
            for &off in &offsets {
                if off >= self.reader_scroll {
                    break;
                }
                target = Some(off);
            }
            if let Some(t) = target {
                self.reader_scroll = t;
            }
        }
    }

    pub(crate) fn reader_next_heading_level(&mut self, level: usize) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.reader_content_width();
            let offsets = self.heading_offsets(&task, Some(level), w);
            for &off in &offsets {
                if off > self.reader_scroll {
                    self.reader_scroll = off;
                    return;
                }
            }
        }
    }

    pub(crate) fn reader_prev_heading_level(&mut self, level: usize) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.reader_content_width();
            let offsets = self.heading_offsets(&task, Some(level), w);
            let mut target = None;
            for &off in &offsets {
                if off >= self.reader_scroll {
                    break;
                }
                target = Some(off);
            }
            if let Some(t) = target {
                self.reader_scroll = t;
            }
        }
    }

    pub(crate) fn detail_next_heading(&mut self) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.detail_content_width();
            let offsets = self.heading_offsets(&task, None, w);
            for &off in &offsets {
                if off > self.detail.scroll {
                    self.detail.scroll = off;
                    return;
                }
            }
        }
    }

    pub(crate) fn detail_prev_heading(&mut self) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.detail_content_width();
            let offsets = self.heading_offsets(&task, None, w);
            let mut target = None;
            for &off in &offsets {
                if off >= self.detail.scroll {
                    break;
                }
                target = Some(off);
            }
            if let Some(t) = target {
                self.detail.scroll = t;
            }
        }
    }

    pub(crate) fn detail_next_heading_level(&mut self, level: usize) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.detail_content_width();
            let offsets = self.heading_offsets(&task, Some(level), w);
            for &off in &offsets {
                if off > self.detail.scroll {
                    self.detail.scroll = off;
                    return;
                }
            }
        }
    }

    pub(crate) fn detail_prev_heading_level(&mut self, level: usize) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.detail_content_width();
            let offsets = self.heading_offsets(&task, Some(level), w);
            let mut target = None;
            for &off in &offsets {
                if off >= self.detail.scroll {
                    break;
                }
                target = Some(off);
            }
            if let Some(t) = target {
                self.detail.scroll = t;
            }
        }
    }

    /// Jump to the Nth `##` heading (0-indexed) in the detail view.
    pub(crate) fn detail_goto_heading_index(&mut self, index: usize) {
        if let Some(task) = self.active_task().cloned() {
            let w = self.detail_content_width();
            let offsets = self.heading_offsets(&task, Some(2), w);
            if let Some(&off) = offsets.get(index) {
                self.detail.scroll = off;
            }
        }
    }

    // ── Jump list helpers ────────────────────────────────────────────

    /// Create a snapshot of the current navigation state.
    pub(crate) fn current_snapshot(&self) -> JumpEntry {
        let (scroll, fold_level) = match self.view {
            AppView::Detail => (self.detail.scroll, self.detail.fold_level),
            _ => (0, 0),
        };
        JumpEntry {
            view: self.view,
            task_id: self.active_task().map(|t| t.id),
            col: self.active_col,
            row: self.active_row,
            scroll,
            fold_level,
            collapsed: self.columns.iter().map(|c| c.collapsed).collect(),
            search_query: self.search.query.clone(),
            find_query: self.detail.find_query.clone(),
        }
    }

    /// Context switch: push current position, forking forward history.
    /// Used when entering detail (Enter) or switching tasks (goto).
    pub(crate) fn push_jump(&mut self) {
        let snap = self.current_snapshot();
        self.jump_list.push(snap);
    }

    /// Exit context: save final position without forking forward history.
    /// Used when leaving detail (q/Esc) — lets the user resume forward
    /// navigation after reviewing a past position.
    pub(crate) fn exit_jump(&mut self) {
        let snap = self.current_snapshot();
        self.jump_list.update_in_place(snap);
    }

    /// Restore navigation state from a jump entry.
    pub(crate) fn restore_jump(&mut self, entry: JumpEntry) {
        // Restore board layout (collapsed state).
        for (col, &was_collapsed) in self.columns.iter_mut().zip(entry.collapsed.iter()) {
            col.collapsed = was_collapsed;
        }
        // Restore cursor — try task_id first, fall back to col/row.
        if let Some(id) = entry.task_id {
            self.select_task_by_id(id);
        } else {
            self.active_col = entry.col.min(self.columns.len().saturating_sub(1));
            self.active_row = entry.row;
            self.clamp_active_row();
        }
        self.view = entry.view;
        if entry.view == AppView::Detail {
            self.detail.scroll = entry.scroll;
            self.detail.fold_level = entry.fold_level;
        }

        // Restore search/find queries.
        if self.search.query != entry.search_query {
            self.search.query = entry.search_query;
            self.on_search_query_changed();
        }
        if entry.view == AppView::Detail && self.detail.find_query != entry.find_query {
            self.detail.find_query = entry.find_query;
            if self.detail.find_query.is_empty() {
                self.detail.find_matches.clear();
                self.detail.find_current = 0;
            } else {
                self.on_find_query_changed();
            }
        }
    }

    // ── Find-in-detail (#49) ─────────────────────────────────────────

    /// Recompute find matches from the current find_query against the
    /// active task's detail lines (metadata + body).
    ///
    /// Uses cached body line texts from the heading cache when available,
    /// avoiding a redundant markdown parse on every keystroke.
    pub fn recompute_find_matches(&mut self) {
        self.detail.find_matches.clear();
        self.detail.find_current = 0;

        if self.detail.find_query.is_empty() {
            return;
        }

        if let Some(task) = self.active_task().cloned() {
            let query = self.detail.find_query.to_lowercase();

            // Check title.
            if task.title.to_lowercase().contains(&query) {
                self.detail.find_matches.push(0);
            }

            // Ensure heading cache is populated (scans rendered detail lines).
            let w = self.detail_content_width();
            let _ = self.heading_offsets(&task, None, w);

            // Read cached body line texts — collect matching indices into a
            // local vec to avoid borrowing self.detail simultaneously for
            // the cache read and the find_matches push.
            let mut new_matches: Vec<usize> = Vec::new();
            if let Some(ref entry) = self.detail.heading_cache {
                if entry.task_id == task.id as u32 && entry.body == task.body {
                    for (i, text) in entry.body_line_texts.iter().enumerate() {
                        if text.to_lowercase().contains(&query) {
                            new_matches.push(entry.meta_count + i);
                        }
                    }
                }
            }
            self.detail.find_matches.extend(new_matches);
        }
    }

    pub(crate) fn find_next(&mut self) {
        if self.detail.find_matches.is_empty() {
            return;
        }
        self.detail.find_current = (self.detail.find_current + 1) % self.detail.find_matches.len();
        self.scroll_to_find_match();
    }

    pub(crate) fn find_prev(&mut self) {
        if self.detail.find_matches.is_empty() {
            return;
        }
        if self.detail.find_current == 0 {
            self.detail.find_current = self.detail.find_matches.len() - 1;
        } else {
            self.detail.find_current -= 1;
        }
        self.scroll_to_find_match();
    }

    pub(crate) fn scroll_to_find_match(&mut self) {
        if let Some(&line_idx) = self.detail.find_matches.get(self.detail.find_current) {
            let w = self.detail_content_width();
            let vrow = if let Some(ref entry) = self.detail.cache {
                if entry.width == w {
                    entry.vrow_offsets.get(line_idx).copied().unwrap_or(line_idx)
                } else {
                    line_idx
                }
            } else {
                line_idx
            };
            let half_page = self.detail_page_size() / 2;
            self.detail.scroll = vrow.saturating_sub(half_page);
        }
    }

    // ── Fold ─────────────────────────────────────────────────────────

    pub fn fold_level(&self) -> usize {
        self.detail.fold_level
    }

    pub(crate) fn fold_deeper(&mut self) {
        let old = self.detail.fold_level;
        // Cycle: 0 → 3 → 2 (fold ### first, then ##).
        match self.detail.fold_level {
            0 => self.detail.fold_level = 3,
            3 => self.detail.fold_level = 2,
            _ => {} // already at max fold
        }
        if self.detail.fold_level != old {
            self.anchor_scroll_across_fold();
            self.set_status(format!("Fold level: h{}", self.detail.fold_level));
        }
    }

    pub(crate) fn fold_shallower(&mut self) {
        let old = self.detail.fold_level;
        // Cycle: 2 → 3 → 0.
        match self.detail.fold_level {
            2 => self.detail.fold_level = 3,
            3 => self.detail.fold_level = 0,
            _ => {} // already fully expanded
        }
        if self.detail.fold_level != old {
            self.anchor_scroll_across_fold();
            self.set_status(format!(
                "Fold level: {}",
                if self.detail.fold_level == 0 { "off".to_string() } else { format!("h{}", self.detail.fold_level) }
            ));
        }
    }

    /// After a fold-level change, adjust the scroll offset so that the
    /// content at the viewport stays at the same screen position.
    ///
    /// Uses heading ordinal matching: the N-th heading in the old
    /// rendering corresponds to the N-th heading in the new rendering
    /// (heading order is stable across fold levels). The scroll offset
    /// is shifted by exactly the amount the anchor heading moved.
    ///
    /// All positions are in **visual rows** (post-wrapping) to match
    /// `Paragraph::scroll()` semantics.
    pub(crate) fn anchor_scroll_across_fold(&mut self) {
        let scroll = match self.view {
            AppView::Detail => self.detail.scroll,
            _ => self.reader_scroll,
        };

        // 1. Collect heading visual-row positions from the OLD rendered lines.
        //    Extract all needed values into locals before mutating the cache.
        let (old_heading_vrows, cached_width) = {
            let entry = match self.detail.cache.as_ref() {
                Some(e) => e,
                None => return,
            };
            // Find heading line indices (skip code blocks for consistency).
            let heading_line_indices: Vec<usize> = entry
                .lines
                .iter()
                .enumerate()
                .filter_map(|(i, line)| {
                    let is_code = line.style.bg.is_some();
                    if is_code {
                        return None;
                    }
                    line.spans
                        .first()
                        .filter(|s| s.content.starts_with('#'))
                        .map(|_| i)
                })
                .collect();
            // Convert to visual rows using cached offsets (O(1) per lookup).
            let vrows: Vec<usize> = heading_line_indices
                .iter()
                .map(|&idx| entry.vrow_offsets.get(idx).copied().unwrap_or(idx))
                .collect();
            (vrows, entry.width)
        };

        // Find the heading at or just before the scroll position (in visual rows).
        let anchor_idx = match old_heading_vrows.iter().rposition(|&pos| pos <= scroll) {
            Some(idx) => idx,
            None => return, // no heading before scroll — nothing to anchor
        };
        let old_vrow = old_heading_vrows[anchor_idx];

        // 2. Invalidate caches (fold_level already changed) and rebuild.
        self.detail.cache = None;
        self.detail.heading_cache = None;

        let task = match self.active_task() {
            Some(t) => t.clone(),
            None => return,
        };
        let content = super::render::build_detail_lines(self, &task, cached_width);

        // 3. Collect heading visual-row positions from the NEW rendered lines.
        let new_heading_line_indices: Vec<usize> = content
            .lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| {
                let is_code = line.style.bg.is_some();
                if is_code {
                    return None;
                }
                line.spans
                    .first()
                    .filter(|s| s.content.starts_with('#'))
                    .map(|_| i)
            })
            .collect();

        let new_vrow = match new_heading_line_indices.get(anchor_idx) {
            Some(&idx) => content.line_to_vrow(idx),
            None => return,
        };

        // 4. Shift scroll by exactly how far the anchor heading moved
        //    in visual-row space.
        let shift = new_vrow as isize - old_vrow as isize;
        let new_scroll = (scroll as isize + shift).max(0) as usize;
        match self.view {
            AppView::Detail => self.detail.scroll = new_scroll,
            _ => self.reader_scroll = new_scroll,
        }
    }
}
