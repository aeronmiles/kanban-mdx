//! Search DSL parser for advanced task filtering.
//!
//! Supports the following query syntax:
//!
//! - **ID filters**: `#5`, `id:5`, `id:1,3,7`, `id:5-10`, `id:1,3-5,15`
//! - **Time filters**: `@48h`, `@>2w`, `@today`, `created:3d`, `created:>1w`,
//!   `updated:12h`
//! - **Priority filters**: `p:high`, `p:medium+`, `p:high-`, `p:c` (prefix)
//! - **Free text**: anything else is substring-matched against title, body,
//!   tags, and `#id`
//! - **Semantic search**: `~query` triggers embedding-based search. Can be
//!   combined with DSL filters: `@<24h p:high ~error handling`
//!
//! All filter types are AND-combined. Multiple IDs within one filter are
//! OR-combined. The `~` delimiter separates DSL tokens (before) from the
//! semantic query (after); both are applied together.

use crate::model::task::Task;

use super::app::{priority_sort_key, task_age_hours, task_created_age_hours};

// ── Filter types ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TimeFilter {
    /// "updated", "created", or "current" (resolved via time_mode at match time)
    pub field: &'static str,
    /// true = within duration, false = older than
    pub within: bool,
    /// duration in hours
    pub dur_hours: i64,
}

#[derive(Debug, Clone)]
pub struct PriorityFilter {
    /// resolved priority name (e.g. "high", "critical")
    pub name: String,
    /// 0 = exact, 1 = at-or-above, -1 = at-or-below
    pub mode: i8,
}

#[derive(Debug, Clone, Default)]
pub struct SearchFilter {
    /// ID filters — OR-matched (task must match at least one)
    pub ids: Vec<i32>,
    /// Time filters — AND-matched
    pub time_filters: Vec<TimeFilter>,
    /// Priority filters — AND-matched
    pub priority_filters: Vec<PriorityFilter>,
    /// Free-form substring (AND with everything else)
    pub text: String,
    /// Semantic query text extracted after `~` delimiter
    pub semantic_query: Option<String>,
}

// ── Parsing ──────────────────────────────────────────────────────────

impl SearchFilter {
    /// Parse a search query string into a structured filter.
    ///
    /// The `~` character acts as a delimiter: everything after it is treated
    /// as a semantic search query, while everything before it is parsed as
    /// the usual DSL tokens (IDs, time, priority, free text).
    pub fn parse(query: &str) -> Self {
        let mut filter = SearchFilter::default();

        // Split on `~` to extract optional semantic portion.
        let (dsl_part, semantic_part) = match query.find('~') {
            Some(pos) => (&query[..pos], Some(query[pos + 1..].trim().to_string())),
            None => (query, None),
        };

        filter.semantic_query = semantic_part.filter(|s| !s.is_empty());

        let mut text_parts: Vec<&str> = Vec::new();

        for token in dsl_part.split_whitespace() {
            if let Some(ids) = try_parse_id_token(token) {
                filter.ids.extend(ids);
            } else if let Some(tf) = try_parse_time_token(token) {
                filter.time_filters.push(tf);
            } else if let Some(pf) = try_parse_priority_token(token) {
                filter.priority_filters.push(pf);
            } else {
                text_parts.push(token);
            }
        }

        filter.text = text_parts.join(" ");
        filter
    }

    /// Test whether a task matches this filter.
    ///
    /// `time_mode` controls how `@`-prefixed time filters resolve:
    /// `"created"` uses task creation time, `"updated"` (or anything else)
    /// uses last-updated time.
    pub fn matches(&self, task: &Task, time_mode: &str) -> bool {
        // ID filter (OR): if any IDs specified, task must match one.
        if !self.ids.is_empty() && !self.ids.contains(&task.id) {
            return false;
        }

        // Time filters (AND).
        for tf in &self.time_filters {
            let resolved_field = if tf.field == "current" {
                time_mode
            } else {
                tf.field
            };
            let hours = match resolved_field {
                "created" => task_created_age_hours(task),
                _ => task_age_hours(task),
            };
            if tf.within {
                if hours > tf.dur_hours {
                    return false;
                }
            } else if hours <= tf.dur_hours {
                return false;
            }
        }

        // Priority filters (AND).
        for pf in &self.priority_filters {
            let task_rank = priority_sort_key(&task.priority);
            let filter_rank = priority_sort_key(&pf.name);
            match pf.mode {
                1 => {
                    // at-or-above: task rank must be <= filter rank
                    // (lower number = higher priority)
                    if task_rank > filter_rank {
                        return false;
                    }
                }
                -1 => {
                    // at-or-below: task rank must be >= filter rank
                    if task_rank < filter_rank {
                        return false;
                    }
                }
                _ => {
                    // exact
                    if task_rank != filter_rank {
                        return false;
                    }
                }
            }
        }

        // Text filter (substring).
        if !self.text.is_empty() {
            let q = self.text.to_lowercase();
            let matches_text = task.title.to_lowercase().contains(&q)
                || task.body.to_lowercase().contains(&q)
                || task.tags.iter().any(|tag| tag.to_lowercase().contains(&q))
                || format!("#{}", task.id).contains(&q);
            if !matches_text {
                return false;
            }
        }

        true
    }
}

// ── Token parsers ────────────────────────────────────────────────────

/// Try to parse an ID token: `#N`, `id:N`, `id:1,3,7`, `id:5-10`, or mixed.
fn try_parse_id_token(token: &str) -> Option<Vec<i32>> {
    // #N shorthand
    if let Some(rest) = token.strip_prefix('#') {
        if let Ok(n) = rest.parse::<i32>() {
            return Some(vec![n]);
        }
        // Not a valid #N, fall through to text.
        return None;
    }

    // id:...
    let rest = token.strip_prefix("id:")?;
    if rest.is_empty() {
        return None;
    }

    let mut ids = Vec::new();
    for part in rest.split(',') {
        if part.is_empty() {
            continue;
        }
        if let Some(dash_pos) = part.find('-') {
            // Range: N-M
            let start_str = &part[..dash_pos];
            let end_str = &part[dash_pos + 1..];
            let start = start_str.parse::<i32>().ok()?;
            let end = end_str.parse::<i32>().ok()?;
            if start > end {
                return None;
            }
            for i in start..=end {
                ids.push(i);
            }
        } else {
            ids.push(part.parse::<i32>().ok()?);
        }
    }

    if ids.is_empty() {
        None
    } else {
        Some(ids)
    }
}

/// Try to parse a time token: `@48h`, `@>2w`, `@today`, `created:3d`,
/// `created:>1w`, `updated:12h`.
fn try_parse_time_token(token: &str) -> Option<TimeFilter> {
    // @ prefix → follows the current time_mode (resolved at match time)
    if let Some(rest) = token.strip_prefix('@') {
        let (within, dur_str) = if let Some(s) = rest.strip_prefix('>') {
            (false, s)
        } else if let Some(s) = rest.strip_prefix('<') {
            (true, s) // explicit "within" — @<24h same as @24h
        } else {
            (true, rest)
        };
        let dur_hours = parse_duration_hours(dur_str)?;
        return Some(TimeFilter {
            field: "current",
            within,
            dur_hours,
        });
    }

    // created:... or updated:...
    let (field, rest) = if let Some(r) = token.strip_prefix("created:") {
        ("created", r)
    } else if let Some(r) = token.strip_prefix("updated:") {
        ("updated", r)
    } else {
        return None;
    };

    if rest.is_empty() {
        return None;
    }

    let (within, dur_str) = if let Some(s) = rest.strip_prefix('>') {
        (false, s)
    } else if let Some(s) = rest.strip_prefix('<') {
        (true, s) // explicit "within"
    } else {
        (true, rest)
    };
    let dur_hours = parse_duration_hours(dur_str)?;
    Some(TimeFilter {
        field,
        within,
        dur_hours,
    })
}

/// Try to parse a priority token: `p:high`, `p:medium+`, `p:high-`, `p:c`.
fn try_parse_priority_token(token: &str) -> Option<PriorityFilter> {
    let rest = token.strip_prefix("p:")?;
    if rest.is_empty() {
        return None;
    }

    let (name_part, mode) = if let Some(s) = rest.strip_suffix('+') {
        (s, 1i8) // at-or-above
    } else if let Some(s) = rest.strip_suffix('-') {
        (s, -1i8) // at-or-below
    } else {
        (rest, 0i8) // exact
    };

    let name = resolve_priority_prefix(name_part)?;
    Some(PriorityFilter { name, mode })
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Parse a duration string like "48h", "2w", "3d", "30m", "1mo", "today"
/// into hours.
fn parse_duration_hours(s: &str) -> Option<i64> {
    if s == "today" {
        return Some(24);
    }
    // Try suffixes in order: "mo" before "m" to avoid ambiguity.
    if let Some(num_str) = s.strip_suffix("mo") {
        return num_str.parse::<i64>().ok().map(|n| n * 720);
    }
    if let Some(num_str) = s.strip_suffix('w') {
        return num_str.parse::<i64>().ok().map(|n| n * 168);
    }
    if let Some(num_str) = s.strip_suffix('d') {
        return num_str.parse::<i64>().ok().map(|n| n * 24);
    }
    if let Some(num_str) = s.strip_suffix('h') {
        return num_str.parse::<i64>().ok();
    }
    if let Some(num_str) = s.strip_suffix('m') {
        return num_str
            .parse::<i64>()
            .ok()
            .map(|n| (n + 59) / 60); // ceil to at least 1 hour
    }
    None
}

/// Resolve a (possibly abbreviated) priority name to the full canonical name.
/// Returns `None` if the prefix is ambiguous or matches nothing.
fn resolve_priority_prefix(prefix: &str) -> Option<String> {
    let candidates = ["critical", "high", "medium", "low"];
    let lower = prefix.to_lowercase();
    let matches: Vec<&&str> = candidates
        .iter()
        .filter(|c| c.starts_with(&lower))
        .collect();
    if matches.len() == 1 {
        Some(matches[0].to_string())
    } else {
        None
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    /// Helper to build a minimal Task for testing.
    fn make_task(id: i32, priority: &str, title: &str, hours_ago_updated: i64, hours_ago_created: i64) -> Task {
        let now = Utc::now();
        Task {
            id,
            title: title.to_string(),
            status: "todo".to_string(),
            priority: priority.to_string(),
            created: now - Duration::hours(hours_ago_created),
            updated: now - Duration::hours(hours_ago_updated),
            started: None,
            completed: None,
            assignee: String::new(),
            tags: vec!["rust".to_string(), "tui".to_string()],
            due: None,
            estimate: String::new(),
            parent: None,
            depends_on: Vec::new(),
            blocked: false,
            block_reason: String::new(),
            claimed_by: String::new(),
            claimed_at: None,
            class: String::new(),
            branch: String::new(),
            worktree: String::new(),
            body: "Some body text".to_string(),
            file: String::new(),
        }
    }

    // ── Duration parsing ────────────────────────────────────────────

    #[test]
    fn test_parse_duration_hours_basic() {
        assert_eq!(parse_duration_hours("48h"), Some(48));
        assert_eq!(parse_duration_hours("3d"), Some(72));
        assert_eq!(parse_duration_hours("2w"), Some(336));
        assert_eq!(parse_duration_hours("1mo"), Some(720));
        assert_eq!(parse_duration_hours("today"), Some(24));
    }

    #[test]
    fn test_parse_duration_hours_minutes() {
        // 30m → ceil(30/60) = 1h
        assert_eq!(parse_duration_hours("30m"), Some(1));
        // 120m → ceil(120/60) = 2h
        assert_eq!(parse_duration_hours("120m"), Some(2));
        // 61m → ceil(61/60) = ceil(121/60) technically (61+59)/60 = 2
        assert_eq!(parse_duration_hours("61m"), Some(2));
    }

    #[test]
    fn test_parse_duration_hours_invalid() {
        assert_eq!(parse_duration_hours("abc"), None);
        assert_eq!(parse_duration_hours(""), None);
        assert_eq!(parse_duration_hours("h"), None);
    }

    // ── Priority prefix resolution ──────────────────────────────────

    #[test]
    fn test_resolve_priority_prefix_exact() {
        assert_eq!(resolve_priority_prefix("critical"), Some("critical".into()));
        assert_eq!(resolve_priority_prefix("high"), Some("high".into()));
        assert_eq!(resolve_priority_prefix("medium"), Some("medium".into()));
        assert_eq!(resolve_priority_prefix("low"), Some("low".into()));
    }

    #[test]
    fn test_resolve_priority_prefix_abbreviated() {
        assert_eq!(resolve_priority_prefix("c"), Some("critical".into()));
        assert_eq!(resolve_priority_prefix("h"), Some("high".into()));
        assert_eq!(resolve_priority_prefix("m"), Some("medium".into()));
        assert_eq!(resolve_priority_prefix("l"), Some("low".into()));
        assert_eq!(resolve_priority_prefix("crit"), Some("critical".into()));
        assert_eq!(resolve_priority_prefix("med"), Some("medium".into()));
    }

    #[test]
    fn test_resolve_priority_prefix_invalid() {
        assert_eq!(resolve_priority_prefix("x"), None);
        assert_eq!(resolve_priority_prefix(""), None);
    }

    // ── ID token parsing ────────────────────────────────────────────

    #[test]
    fn test_parse_id_hash() {
        assert_eq!(try_parse_id_token("#5"), Some(vec![5]));
        assert_eq!(try_parse_id_token("#123"), Some(vec![123]));
    }

    #[test]
    fn test_parse_id_prefix_single() {
        assert_eq!(try_parse_id_token("id:5"), Some(vec![5]));
    }

    #[test]
    fn test_parse_id_prefix_list() {
        assert_eq!(try_parse_id_token("id:1,3,7"), Some(vec![1, 3, 7]));
    }

    #[test]
    fn test_parse_id_prefix_range() {
        assert_eq!(try_parse_id_token("id:5-10"), Some(vec![5, 6, 7, 8, 9, 10]));
    }

    #[test]
    fn test_parse_id_prefix_mixed() {
        assert_eq!(
            try_parse_id_token("id:1,3-5,15"),
            Some(vec![1, 3, 4, 5, 15])
        );
    }

    #[test]
    fn test_parse_id_not_id() {
        assert_eq!(try_parse_id_token("hello"), None);
        assert_eq!(try_parse_id_token("id:"), None);
        assert_eq!(try_parse_id_token("#abc"), None);
    }

    // ── Time token parsing ──────────────────────────────────────────

    #[test]
    fn test_parse_time_at_within() {
        let tf = try_parse_time_token("@48h").unwrap();
        assert_eq!(tf.field, "current");
        assert!(tf.within);
        assert_eq!(tf.dur_hours, 48);
    }

    #[test]
    fn test_parse_time_at_older() {
        let tf = try_parse_time_token("@>2w").unwrap();
        assert_eq!(tf.field, "current");
        assert!(!tf.within);
        assert_eq!(tf.dur_hours, 336);
    }

    #[test]
    fn test_parse_time_at_today() {
        let tf = try_parse_time_token("@today").unwrap();
        assert_eq!(tf.field, "current");
        assert!(tf.within);
        assert_eq!(tf.dur_hours, 24);
    }

    #[test]
    fn test_parse_time_at_less_than() {
        let tf = try_parse_time_token("@<24h").unwrap();
        assert_eq!(tf.field, "current");
        assert!(tf.within);
        assert_eq!(tf.dur_hours, 24);
    }

    #[test]
    fn test_parse_time_created_less_than() {
        let tf = try_parse_time_token("created:<3d").unwrap();
        assert_eq!(tf.field, "created");
        assert!(tf.within);
        assert_eq!(tf.dur_hours, 72);
    }

    #[test]
    fn test_parse_time_created() {
        let tf = try_parse_time_token("created:3d").unwrap();
        assert_eq!(tf.field, "created");
        assert!(tf.within);
        assert_eq!(tf.dur_hours, 72);
    }

    #[test]
    fn test_parse_time_created_older() {
        let tf = try_parse_time_token("created:>1w").unwrap();
        assert_eq!(tf.field, "created");
        assert!(!tf.within);
        assert_eq!(tf.dur_hours, 168);
    }

    #[test]
    fn test_parse_time_updated_explicit() {
        let tf = try_parse_time_token("updated:12h").unwrap();
        assert_eq!(tf.field, "updated");
        assert!(tf.within);
        assert_eq!(tf.dur_hours, 12);
    }

    #[test]
    fn test_parse_time_not_time() {
        assert!(try_parse_time_token("hello").is_none());
        assert!(try_parse_time_token("p:high").is_none());
    }

    // ── Priority token parsing ──────────────────────────────────────

    #[test]
    fn test_parse_priority_exact() {
        let pf = try_parse_priority_token("p:high").unwrap();
        assert_eq!(pf.name, "high");
        assert_eq!(pf.mode, 0);
    }

    #[test]
    fn test_parse_priority_above() {
        let pf = try_parse_priority_token("p:medium+").unwrap();
        assert_eq!(pf.name, "medium");
        assert_eq!(pf.mode, 1);
    }

    #[test]
    fn test_parse_priority_below() {
        let pf = try_parse_priority_token("p:high-").unwrap();
        assert_eq!(pf.name, "high");
        assert_eq!(pf.mode, -1);
    }

    #[test]
    fn test_parse_priority_prefix() {
        let pf = try_parse_priority_token("p:c").unwrap();
        assert_eq!(pf.name, "critical");
        assert_eq!(pf.mode, 0);
    }

    #[test]
    fn test_parse_priority_not_priority() {
        assert!(try_parse_priority_token("hello").is_none());
        assert!(try_parse_priority_token("p:").is_none());
        assert!(try_parse_priority_token("p:xyz").is_none());
    }

    // ── Full filter parsing ─────────────────────────────────────────

    #[test]
    fn test_parse_full_query() {
        let f = SearchFilter::parse("#5 p:high @48h fix bug");
        assert_eq!(f.ids, vec![5]);
        assert_eq!(f.priority_filters.len(), 1);
        assert_eq!(f.priority_filters[0].name, "high");
        assert_eq!(f.time_filters.len(), 1);
        assert_eq!(f.time_filters[0].dur_hours, 48);
        assert_eq!(f.text, "fix bug");
    }

    #[test]
    fn test_parse_empty_query() {
        let f = SearchFilter::parse("");
        assert!(f.ids.is_empty());
        assert!(f.time_filters.is_empty());
        assert!(f.priority_filters.is_empty());
        assert!(f.text.is_empty());
    }

    #[test]
    fn test_parse_text_only() {
        let f = SearchFilter::parse("fix important bug");
        assert!(f.ids.is_empty());
        assert!(f.time_filters.is_empty());
        assert!(f.priority_filters.is_empty());
        assert_eq!(f.text, "fix important bug");
    }

    // ── Matching ────────────────────────────────────────────────────

    #[test]
    fn test_matches_empty_filter_matches_all() {
        let f = SearchFilter::parse("");
        let task = make_task(1, "high", "Test task", 0, 48);
        assert!(f.matches(&task, "updated"));
    }

    #[test]
    fn test_matches_id_filter() {
        let f = SearchFilter::parse("id:1,3,5");
        let t1 = make_task(1, "high", "A", 0, 48);
        let t2 = make_task(2, "high", "B", 0, 48);
        let t5 = make_task(5, "high", "C", 0, 48);
        assert!(f.matches(&t1, "updated"));
        assert!(!f.matches(&t2, "updated"));
        assert!(f.matches(&t5, "updated"));
    }

    #[test]
    fn test_matches_priority_exact() {
        let f = SearchFilter::parse("p:high");
        let high = make_task(1, "high", "A", 0, 48);
        let med = make_task(2, "medium", "B", 0, 48);
        assert!(f.matches(&high, "updated"));
        assert!(!f.matches(&med, "updated"));
    }

    #[test]
    fn test_matches_priority_above() {
        // p:medium+ means medium or higher (medium, high, critical)
        let f = SearchFilter::parse("p:medium+");
        let crit = make_task(1, "critical", "A", 0, 48);
        let high = make_task(2, "high", "B", 0, 48);
        let med = make_task(3, "medium", "C", 0, 48);
        let low = make_task(4, "low", "D", 0, 48);
        assert!(f.matches(&crit, "updated"));
        assert!(f.matches(&high, "updated"));
        assert!(f.matches(&med, "updated"));
        assert!(!f.matches(&low, "updated"));
    }

    #[test]
    fn test_matches_priority_below() {
        // p:high- means high or lower (high, medium, low)
        let f = SearchFilter::parse("p:high-");
        let crit = make_task(1, "critical", "A", 0, 48);
        let high = make_task(2, "high", "B", 0, 48);
        let med = make_task(3, "medium", "C", 0, 48);
        let low = make_task(4, "low", "D", 0, 48);
        assert!(!f.matches(&crit, "updated"));
        assert!(f.matches(&high, "updated"));
        assert!(f.matches(&med, "updated"));
        assert!(f.matches(&low, "updated"));
    }

    #[test]
    fn test_matches_time_within() {
        // @48h: uses "current" field, resolved to "updated" here
        let f = SearchFilter::parse("@48h");
        let recent = make_task(1, "high", "A", 12, 100);
        let old = make_task(2, "high", "B", 100, 200);
        assert!(f.matches(&recent, "updated"));
        assert!(!f.matches(&old, "updated"));
    }

    #[test]
    fn test_matches_time_older() {
        // @>2w: uses "current" field, resolved to "updated" here
        let f = SearchFilter::parse("@>2w");
        let recent = make_task(1, "high", "A", 12, 100);
        let old = make_task(2, "high", "B", 500, 600);
        assert!(!f.matches(&recent, "updated"));
        assert!(f.matches(&old, "updated"));
    }

    #[test]
    fn test_matches_time_at_respects_created_mode() {
        // @48h with time_mode="created" should use created timestamp
        let f = SearchFilter::parse("@48h");
        // updated 12h ago, created 100h ago → matches "updated" but not "created"
        let task = make_task(1, "high", "A", 12, 100);
        assert!(f.matches(&task, "updated"));
        assert!(!f.matches(&task, "created"));
    }

    #[test]
    fn test_matches_time_at_created_mode_within() {
        // @24h with time_mode="created" should filter by creation time
        let f = SearchFilter::parse("@24h");
        // updated 0h ago, created 12h ago → within 24h for both modes
        let new = make_task(1, "high", "A", 0, 12);
        // updated 0h ago, created 100h ago → within 24h for updated, not for created
        let old_created = make_task(2, "high", "B", 0, 100);
        assert!(f.matches(&new, "created"));
        assert!(!f.matches(&old_created, "created"));
    }

    #[test]
    fn test_matches_explicit_created_ignores_time_mode() {
        // created:3d always uses created time regardless of time_mode
        let f = SearchFilter::parse("created:3d");
        let new = make_task(1, "high", "A", 0, 24);
        let old = make_task(2, "high", "B", 0, 200);
        assert!(f.matches(&new, "updated"));
        assert!(!f.matches(&old, "updated"));
    }

    #[test]
    fn test_matches_explicit_updated_ignores_time_mode() {
        // updated:48h always uses updated time regardless of time_mode
        let f = SearchFilter::parse("updated:48h");
        // updated 12h ago, created 100h ago
        let task = make_task(1, "high", "A", 12, 100);
        assert!(f.matches(&task, "created")); // time_mode is created but filter says updated
    }

    #[test]
    fn test_matches_created_within() {
        // created:3d: created within 72 hours
        let f = SearchFilter::parse("created:3d");
        let new = make_task(1, "high", "A", 0, 24);
        let old = make_task(2, "high", "B", 0, 200);
        assert!(f.matches(&new, "updated"));
        assert!(!f.matches(&old, "updated"));
    }

    #[test]
    fn test_matches_text() {
        let f = SearchFilter::parse("important");
        let yes = make_task(1, "high", "An important task", 0, 48);
        let no = make_task(2, "high", "A trivial task", 0, 48);
        assert!(f.matches(&yes, "updated"));
        assert!(!f.matches(&no, "updated"));
    }

    #[test]
    fn test_matches_text_in_body() {
        let f = SearchFilter::parse("body text");
        let task = make_task(1, "high", "Title only", 0, 48);
        // body is "Some body text" by default from make_task
        assert!(f.matches(&task, "updated"));
    }

    #[test]
    fn test_matches_text_in_tags() {
        let f = SearchFilter::parse("rust");
        let task = make_task(1, "high", "No match in title", 0, 48);
        assert!(f.matches(&task, "updated"));
    }

    #[test]
    fn test_matches_combined_filters() {
        // Must match ALL: id in [1,2,3], priority high or above, updated within 48h
        let f = SearchFilter::parse("id:1,2,3 p:high+ @48h");
        // Matches: id=1, priority=critical, updated 6h ago
        let yes = make_task(1, "critical", "A", 6, 100);
        // Fails: id=4 (not in set)
        let no_id = make_task(4, "critical", "B", 6, 100);
        // Fails: priority=medium (below high)
        let no_pri = make_task(2, "medium", "C", 6, 100);
        // Fails: updated 100h ago (> 48h)
        let no_time = make_task(3, "high", "D", 100, 200);

        assert!(f.matches(&yes, "updated"));
        assert!(!f.matches(&no_id, "updated"));
        assert!(!f.matches(&no_pri, "updated"));
        assert!(!f.matches(&no_time, "updated"));
    }

    #[test]
    fn test_matches_id_range() {
        let f = SearchFilter::parse("id:5-10");
        for id in 5..=10 {
            let t = make_task(id, "high", "A", 0, 48);
            assert!(f.matches(&t, "updated"), "id {} should match", id);
        }
        let t4 = make_task(4, "high", "A", 0, 48);
        let t11 = make_task(11, "high", "A", 0, 48);
        assert!(!f.matches(&t4, "updated"));
        assert!(!f.matches(&t11, "updated"));
    }

    #[test]
    fn test_matches_hash_id_in_text_filter() {
        // When search is just "#3" as text (after id parsing extracts it)
        let f = SearchFilter::parse("#3");
        let t3 = make_task(3, "high", "A", 0, 48);
        let t5 = make_task(5, "high", "A", 0, 48);
        assert!(f.matches(&t3, "updated"));
        assert!(!f.matches(&t5, "updated"));
    }

    // ── Semantic query parsing ─────────────────────────────────────

    #[test]
    fn test_parse_semantic_only() {
        let f = SearchFilter::parse("~error handling");
        assert_eq!(f.semantic_query, Some("error handling".to_string()));
        assert!(f.text.is_empty());
        assert!(f.ids.is_empty());
    }

    #[test]
    fn test_parse_semantic_with_dsl() {
        let f = SearchFilter::parse("p:high ~error handling");
        assert_eq!(f.semantic_query, Some("error handling".to_string()));
        assert_eq!(f.priority_filters.len(), 1);
        assert!(f.text.is_empty());
    }

    #[test]
    fn test_parse_semantic_with_time() {
        let f = SearchFilter::parse("@48h ~performance");
        assert_eq!(f.semantic_query, Some("performance".to_string()));
        assert_eq!(f.time_filters.len(), 1);
    }

    #[test]
    fn test_parse_semantic_with_ids() {
        let f = SearchFilter::parse("#5 ~refactor");
        assert_eq!(f.semantic_query, Some("refactor".to_string()));
        assert_eq!(f.ids, vec![5]);
    }

    #[test]
    fn test_parse_no_semantic() {
        let f = SearchFilter::parse("plain text");
        assert_eq!(f.semantic_query, None);
        assert_eq!(f.text, "plain text");
    }

    #[test]
    fn test_parse_empty_semantic() {
        let f = SearchFilter::parse("~");
        assert_eq!(f.semantic_query, None);
    }

    #[test]
    fn test_parse_semantic_tilde_only_whitespace() {
        let f = SearchFilter::parse("~   ");
        assert_eq!(f.semantic_query, None);
    }

    #[test]
    fn test_parse_combined_dsl_text_and_semantic() {
        let f = SearchFilter::parse("p:high keyword ~semantic query");
        assert_eq!(f.semantic_query, Some("semantic query".to_string()));
        assert_eq!(f.priority_filters.len(), 1);
        assert_eq!(f.text, "keyword");
    }

    #[test]
    fn test_semantic_query_detection() {
        // These mirror what App::is_semantic_query does
        assert!(SearchFilter::parse("~error").semantic_query.is_some());
        assert!(SearchFilter::parse("p:high ~err").semantic_query.is_some());
        assert!(SearchFilter::parse("plain text").semantic_query.is_none());
    }
}
