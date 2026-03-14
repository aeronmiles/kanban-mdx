//! Semantic search — debounce, async dispatch, result handling.

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::app::App;
use super::types::{AppView, SemFindResult, SemSearchResult};

/// Debounce delay for semantic search (reset on each keystroke).
const SEMANTIC_DEBOUNCE_MS: u64 = 300;

impl App {
    // ── Semantic search (~prefix) helpers ──────────────────────────

    /// Returns true if the query contains a `~` semantic search component.
    pub fn is_semantic_query(query: &str) -> bool {
        query.contains('~')
    }

    /// Extracts the semantic search portion (text after `~`), trimmed.
    pub(crate) fn sem_query_text(query: &str) -> &str {
        match query.find('~') {
            Some(pos) => query[pos + 1..].trim_start(),
            None => "",
        }
    }

    /// Extracts the DSL portion (text before `~`), trimmed.
    pub(crate) fn dsl_query_text(query: &str) -> &str {
        match query.find('~') {
            Some(pos) => query[..pos].trim_end(),
            None => query,
        }
    }

    /// Returns true if semantic search is configured (provider is set).
    /// The index is auto-synced on first query if it doesn't exist yet.
    pub(crate) fn sem_available(&self) -> bool {
        !self.cfg.semantic_search.provider.is_empty()
    }

    /// Resets all semantic search state (board scores + detail find).
    pub(crate) fn clear_sem_state(&mut self) {
        self.search.sem_last_key = None;
        self.search.sem_pending = false;
        self.search.sem_loading = false;
        self.search.sem_error = None;
        self.search.sem_search_rx = None;
        self.search.sem_scores.clear();
        self.search.sem_find_rx = None;
    }

    /// Resets only the detail-find semantic state, preserving board-level
    /// `sem_scores` and `sem_search_rx`.
    pub(crate) fn clear_sem_find_state(&mut self) {
        self.search.sem_last_key = None;
        self.search.sem_pending = false;
        self.search.sem_loading = false;
        self.search.sem_error = None;
        self.search.sem_find_rx = None;
    }

    /// Called when the search query changes (board `/` mode).
    /// If query contains `~`, arms debounce for semantic search (DSL tokens
    /// before `~` are applied live by `filtered_tasks`).
    /// Otherwise, semantic state is cleared.
    pub(crate) fn on_search_query_changed(&mut self) {
        if Self::is_semantic_query(&self.search.query) {
            if self.sem_available() {
                self.search.sem_error = None;
                self.search.sem_last_key = Some(Instant::now());
                self.search.sem_pending = true;
            } else {
                self.search.sem_error = Some("semantic search not configured (set semantic_search.provider in config.toml)".into());
                self.search.sem_scores.clear();
            }
        } else {
            // Plain DSL: clear semantic state, filtering is live in rendered view.
            self.clear_sem_state();
        }
    }

    /// Called when the find query changes (detail `/` mode).
    /// If query contains `~`, arms debounce for semantic find.
    /// Otherwise, delegates to `recompute_find_matches()`.
    /// Uses `clear_sem_find_state()` to preserve board-level `sem_scores`.
    pub(crate) fn on_find_query_changed(&mut self) {
        if Self::is_semantic_query(&self.detail.find_query) {
            if self.sem_available() {
                self.search.sem_error = None;
                self.search.sem_last_key = Some(Instant::now());
                self.search.sem_pending = true;
            } else {
                self.search.sem_error = Some("semantic search not configured (set semantic_search.provider in config.toml)".into());
            }
        } else {
            self.clear_sem_find_state();
            self.recompute_find_matches();
        }
    }

    /// Checks debounce timer and async result channels. Called from event loop.
    pub fn tick_semantic_debounce(&mut self) {
        // Check for completed board-level semantic search results.
        if let Some(rx) = &self.search.sem_search_rx {
            if let Ok(result) = rx.try_recv() {
                let current_sem = Self::sem_query_text(&self.search.query);
                if result.query == current_sem {
                    if let Some(err) = result.error {
                        self.search.sem_error = Some(err);
                        self.search.sem_scores.clear();
                    } else {
                        self.search.sem_scores = result.scores;
                        self.search.sem_error = None;
                        // Navigate to the best (highest-scoring) match.
                        if let Some((&best_id, _)) = self.search.sem_scores.iter()
                            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                        {
                            self.select_task_by_id(best_id);
                        }
                    }
                    self.search.sem_loading = false;
                }
                self.debug.needs_redraw = true;
            }
        }

        // Check for completed detail-level semantic find results.
        if let Some(rx) = &self.search.sem_find_rx {
            if let Ok(result) = rx.try_recv() {
                let current_sem = Self::sem_query_text(&self.detail.find_query);
                if result.query == current_sem {
                    if let Some(err) = result.error {
                        self.search.sem_error = Some(err);
                        self.detail.find_matches.clear();
                    } else {
                        self.detail.find_matches = result.line_indices;
                        self.detail.find_current = 0;
                        self.search.sem_error = None;
                        self.scroll_to_find_match();
                    }
                    self.search.sem_loading = false;
                }
                self.debug.needs_redraw = true;
            }
        }

        // Fire debounced search if timer has elapsed.
        if self.search.sem_pending {
            if let Some(last) = self.search.sem_last_key {
                if last.elapsed() >= Duration::from_millis(SEMANTIC_DEBOUNCE_MS) {
                    self.search.sem_pending = false;
                    self.fire_semantic();
                }
            }
        }
    }

    /// Dispatches the appropriate semantic search based on current view.
    pub(crate) fn fire_semantic(&mut self) {
        match self.view {
            AppView::Search | AppView::Board => self.fire_sem_board_search(),
            AppView::Detail => self.fire_sem_detail_find(),
            _ => {}
        }
    }

    /// Launches board-level semantic search in a background thread.
    pub(crate) fn fire_sem_board_search(&mut self) {
        let query = Self::sem_query_text(&self.search.query).to_string();
        if query.is_empty() {
            return;
        }

        self.search.sem_loading = true;
        self.debug.needs_redraw = true;

        let cfg = self.cfg.clone();
        let (tx, rx) = mpsc::channel();
        self.search.sem_search_rx = Some(rx);

        std::thread::spawn(move || {
            let result = match crate::embed::Manager::new(&cfg) {
                Ok(mut mgr) => {
                    // Auto-sync if the index is empty (first use or after clear).
                    if mgr.doc_count() == 0 {
                        let sync_err = match crate::model::task::read_all_lenient(&cfg.tasks_path()) {
                            Ok((tasks, _)) => mgr.sync(&tasks).err().map(|e| format!("sync: {e}")),
                            Err(e) => Some(format!("loading tasks: {e}")),
                        };
                        if let Some(err) = sync_err {
                            let _ = tx.send(SemSearchResult {
                                query,
                                scores: HashMap::new(),
                                error: Some(err),
                            });
                            return;
                        }
                    }
                    match mgr.search(&query, 0) {
                        Ok(results) => SemSearchResult {
                            query,
                            scores: results.iter().map(|r| (r.task_id, r.score)).collect(),
                            error: None,
                        },
                        Err(e) => SemSearchResult {
                            query,
                            scores: HashMap::new(),
                            error: Some(e.to_string()),
                        },
                    }
                }
                Err(e) => SemSearchResult {
                    query,
                    scores: HashMap::new(),
                    error: Some(e.to_string()),
                },
            };
            let _ = tx.send(result);
        });
    }

    /// Launches detail-level semantic find in a background thread.
    pub(crate) fn fire_sem_detail_find(&mut self) {
        let query = Self::sem_query_text(&self.detail.find_query).to_string();
        if query.is_empty() {
            return;
        }

        let task = match self.active_task().cloned() {
            Some(t) => t,
            None => return,
        };
        let task_id = task.id;

        self.search.sem_loading = true;
        self.debug.needs_redraw = true;

        // Ensure heading cache is populated so we have rendered body line texts.
        let w = self.detail_content_width();
        let _ = self.heading_offsets(&task, None, w);

        let cfg = self.cfg.clone();
        // Capture the cached body line texts so we can map chunk headers/lines
        // to rendered detail-view line indices on the background thread.
        let (meta_count, body_line_texts) = match self.detail.heading_cache.as_ref() {
            Some(c) if c.task_id == task_id as u32 => {
                (c.meta_count, c.body_line_texts.clone())
            }
            _ => (0, Vec::new()),
        };

        let (tx, rx) = mpsc::channel();
        self.search.sem_find_rx = Some(rx);

        std::thread::spawn(move || {
            let result = match crate::embed::Manager::new(&cfg) {
                Ok(mut mgr) => {
                    // Auto-sync if the index is empty (first use or after clear).
                    if mgr.doc_count() == 0 {
                        let sync_err = match crate::model::task::read_all_lenient(&cfg.tasks_path()) {
                            Ok((tasks, _)) => mgr.sync(&tasks).err().map(|e| format!("sync: {e}")),
                            Err(e) => Some(format!("loading tasks: {e}")),
                        };
                        if let Some(err) = sync_err {
                            let _ = tx.send(SemFindResult {
                                query,
                                line_indices: Vec::new(),
                                error: Some(err),
                            });
                            return;
                        }
                    }
                    // Use a large limit so the current task's chunks aren't excluded
                    // by higher-scoring chunks from other tasks.
                    match mgr.find(&query, 500) {
                        Ok(results) => {
                            // Filter to chunks belonging to this task.
                            let task_results: Vec<_> = results
                                .iter()
                                .filter(|r| r.task_id == task_id)
                                .collect();

                            // Map each matching chunk to rendered detail-view line
                            // indices. A chunk's `r.line` is the raw body line where
                            // its heading starts. Find the closest body_line_text
                            // that contains the heading to get the rendered index.
                            let mut line_indices: Vec<usize> = Vec::new();
                            for r in &task_results {
                                if !r.header.is_empty() {
                                    // Find rendered line matching this section header.
                                    if let Some(idx) =
                                        body_line_texts.iter().position(|t| {
                                            t.trim() == r.header.trim()
                                                || t.trim()
                                                    .starts_with(r.header.trim())
                                        })
                                    {
                                        line_indices.push(meta_count + idx);
                                    }
                                } else {
                                    // Preamble or headerless chunk — match at body start.
                                    line_indices.push(meta_count);
                                }
                            }
                            line_indices.dedup();

                            SemFindResult {
                                query,
                                line_indices,
                                error: None,
                            }
                        }
                        Err(e) => SemFindResult {
                            query,
                            line_indices: Vec::new(),
                            error: Some(e.to_string()),
                        },
                    }
                }
                Err(e) => SemFindResult {
                    query,
                    line_indices: Vec::new(),
                    error: Some(e.to_string()),
                },
            };
            let _ = tx.send(result);
        });
    }
}
