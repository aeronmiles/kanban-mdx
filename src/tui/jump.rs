//! Jump list — back/forward navigation history for detail-view contexts.

use super::types::AppView;

/// A snapshot of navigation state at a point in time.
#[derive(Debug, Clone)]
pub struct JumpEntry {
    pub view: AppView,
    pub task_id: Option<i32>,
    pub col: usize,
    pub row: usize,
    pub scroll: usize,
    pub fold_level: usize,
    /// Per-column collapsed state (parallel to `App::columns`).
    pub collapsed: Vec<bool>,
    /// Board-level search/filter query active at this point.
    pub search_query: String,
    /// Detail-level find query active at this point.
    pub find_query: String,
}

impl JumpEntry {
    /// Whether two entries represent the same navigation destination
    /// (same view + task_id, ignoring scroll/fold/layout state).
    fn same_destination(&self, other: &JumpEntry) -> bool {
        self.view == other.view && self.task_id == other.task_id
    }

    /// Serialize to a tab-delimited string.
    /// Format: `view\ttask_id\tcol\trow\tscroll\tfold\tcollapsed_csv\tsearch_query\tfind_query`
    fn serialize(&self) -> String {
        let view_id = match self.view {
            AppView::Board => 0,
            AppView::Detail => 1,
            _ => 0,
        };
        let task_str = self
            .task_id
            .map(|id| id.to_string())
            .unwrap_or_default();
        let collapsed_csv: String = self
            .collapsed
            .iter()
            .map(|&b| if b { "1" } else { "0" })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            view_id, task_str, self.col, self.row, self.scroll, self.fold_level, collapsed_csv,
            self.search_query, self.find_query
        )
    }

    /// Deserialize from a tab-delimited string.
    fn deserialize(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('\t').collect();
        if parts.len() < 7 {
            return None;
        }
        let view = match parts[0].parse::<u8>().ok()? {
            0 => AppView::Board,
            1 => AppView::Detail,
            _ => return None,
        };
        let task_id = if parts[1].is_empty() {
            None
        } else {
            Some(parts[1].parse::<i32>().ok()?)
        };
        let col = parts[2].parse().ok()?;
        let row = parts[3].parse().ok()?;
        let scroll = parts[4].parse().ok()?;
        let fold_level = parts[5].parse().ok()?;
        let collapsed = if parts[6].is_empty() {
            Vec::new()
        } else {
            parts[6]
                .split(',')
                .map(|v| v == "1")
                .collect()
        };
        let search_query = parts.get(7).unwrap_or(&"").to_string();
        let find_query = parts.get(8).unwrap_or(&"").to_string();
        Some(Self {
            view,
            task_id,
            col,
            row,
            scroll,
            fold_level,
            collapsed,
            search_query,
            find_query,
        })
    }
}

/// Destination stack of significant navigation positions.
///
/// Only detail-view context transitions are recorded (exiting detail,
/// switching tasks via goto while in detail). Heading navigation and
/// board-level movements are local and do not push.
///
/// Entries are updated in-place when leaving via back/forward so they
/// always reflect the *final* position in that context, not the arrival.
pub struct JumpList {
    entries: Vec<JumpEntry>,
    /// Equal to `entries.len()` means "at the present / tip".
    cursor: usize,
    max_entries: usize,
    /// Optional file path for persistence between sessions.
    path: Option<std::path::PathBuf>,
}

impl JumpList {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            cursor: 0,
            max_entries,
            path: None,
        }
    }

    /// Create a persistent jump list backed by a file.
    pub fn with_path(path: std::path::PathBuf, max_entries: usize) -> Self {
        let mut jl = Self {
            entries: Vec::new(),
            cursor: 0,
            max_entries,
            path: Some(path),
        };
        jl.load();
        jl.cursor = jl.entries.len(); // start at tip
        jl
    }

    /// Push a new entry, truncating any forward history.
    ///
    /// Before inserting, collapses any trailing ping-pong pattern
    /// (e.g. `A,B,A,B,A` → `A`) so that "back" skips redundant bouncing
    /// and reaches the previous *distinct* destination directly.
    /// Also deduplicates any remaining earlier entry for the same
    /// destination as the new entry.
    pub fn push(&mut self, entry: JumpEntry) {
        self.entries.truncate(self.cursor);
        self.sanitize_pingpong();
        // Remove earlier entries for the same destination (view + task_id).
        self.entries.retain(|e| !e.same_destination(&entry));
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
        self.cursor = self.entries.len();
        self.save();
    }

    /// Collapse a trailing ping-pong pattern in the entries list.
    ///
    /// A ping-pong is 3+ consecutive entries that alternate between exactly
    /// two destinations: `[…, X, A, B, A, B, A]`.  The alternating run
    /// `A, B, A, B, A` is collapsed to its final entry `A`, so going back
    /// skips the redundant bouncing and reaches `X` directly.
    fn sanitize_pingpong(&mut self) {
        let len = self.entries.len();
        if len < 3 {
            return;
        }

        let last = len - 1;
        // The two candidate ping-pong destinations (Copy types).
        let a_view = self.entries[last].view;
        let a_tid = self.entries[last].task_id;
        let b_view = self.entries[last - 1].view;
        let b_tid = self.entries[last - 1].task_id;

        // Must be two distinct destinations to form a ping-pong.
        if a_view == b_view && a_tid == b_tid {
            return;
        }

        // Walk backwards checking if entries continue alternating a ↔ b.
        let mut start = last - 1;
        for i in (0..last - 1).rev() {
            let (ev, et) = if (last - i) % 2 == 0 {
                (a_view, a_tid)
            } else {
                (b_view, b_tid)
            };
            if self.entries[i].view == ev && self.entries[i].task_id == et {
                start = i;
            } else {
                break;
            }
        }

        if last - start + 1 >= 3 {
            // Drain everything except the final entry of the run.
            self.entries.drain(start..last);
        }
    }

    /// Update the entry at cursor in-place without forking forward history.
    /// At the tip (no entry at cursor), falls back to a regular `push`.
    ///
    /// Used when exiting a context (q/Esc) — preserves forward history so
    /// the user can continue navigating after reviewing a past position.
    pub fn update_in_place(&mut self, entry: JumpEntry) {
        if self.cursor < self.entries.len() {
            self.entries[self.cursor] = entry;
            self.save();
        } else {
            self.push(entry);
        }
    }

    /// Go back one entry.  Updates the entry being left with `current`
    /// so the destination reflects the final position in that context.
    pub fn back(&mut self, current: JumpEntry) -> Option<&JumpEntry> {
        if self.entries.is_empty() {
            return None;
        }
        if self.cursor == self.entries.len() {
            // At the tip — save current so forward() can return here.
            self.entries.push(current);
        } else {
            // Mid-history — update the entry we're leaving.
            self.entries[self.cursor] = current;
        }
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        self.save();
        self.entries.get(self.cursor)
    }

    /// Go forward one entry.  Updates the entry being left with `current`.
    pub fn forward(&mut self, current: JumpEntry) -> Option<&JumpEntry> {
        if self.cursor + 1 >= self.entries.len() {
            return None;
        }
        self.entries[self.cursor] = current;
        self.cursor += 1;
        self.save();
        self.entries.get(self.cursor)
    }

    /// Number of entries behind the cursor.
    pub fn back_count(&self) -> usize {
        self.cursor
    }

    /// Number of entries ahead of the cursor.
    pub fn forward_count(&self) -> usize {
        self.entries.len().saturating_sub(self.cursor + 1)
    }

    /// Whether the jump list has any history worth displaying.
    pub fn has_history(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Load entries from disk (tab-delimited, one entry per line).
    fn load(&mut self) {
        let path = match &self.path {
            Some(p) => p,
            None => return,
        };
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(entry) = JumpEntry::deserialize(trimmed) {
                self.entries.push(entry);
            }
        }
        if self.entries.len() > self.max_entries {
            let excess = self.entries.len() - self.max_entries;
            self.entries.drain(..excess);
        }
    }

    /// Save entries to disk (tab-delimited, one entry per line).
    fn save(&self) {
        let path = match &self.path {
            Some(p) => p,
            None => return,
        };
        let content: String = self
            .entries
            .iter()
            .map(|e| e.serialize())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let _ = std::fs::write(path, content);
    }
}
