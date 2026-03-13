//! Config model representing the kanban board configuration (config.toml).
//!
//! Mirrors the Go `internal/config` package. Provides the full `Config` struct
//! and all related sub-types with serde serialization, validation, and
//! accessor methods.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("no kanban board found (run 'kanban-md init' to create one)")]
    NotFound,

    #[error("invalid config: {0}")]
    Invalid(String),

    #[error("{0}")]
    Io(#[from] std::io::Error),

    #[error("parsing config (yaml): {0}")]
    Yaml(#[from] serde_yml::Error),

    #[error("parsing config: {0}")]
    Toml(#[from] toml::de::Error),
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default kanban directory name.
pub const DEFAULT_DIR: &str = "kanban";
/// Default tasks subdirectory name.
pub const DEFAULT_TASKS_DIR: &str = "tasks";
/// Default status for new tasks.
pub const DEFAULT_STATUS: &str = "backlog";
/// Default priority for new tasks.
pub const DEFAULT_PRIORITY: &str = "medium";
/// Default class of service for new tasks.
pub const DEFAULT_CLASS: &str = "standard";
/// Default claim expiration duration string.
pub const DEFAULT_CLAIM_TIMEOUT: &str = "1h";
/// Default number of title lines in TUI cards.
pub const DEFAULT_TITLE_LINES: i32 = 2;
/// Default hide-empty-columns setting.
pub const DEFAULT_HIDE_EMPTY_COLUMNS: bool = false;
/// Default max content width for the reader panel (0 = no limit).
pub const DEFAULT_READER_MAX_WIDTH: i32 = 120;
/// Default reader panel width as a percentage of terminal width (board view).
pub const DEFAULT_READER_WIDTH_PCT: i32 = 40;
/// Name of the config file within the kanban directory.
pub const CONFIG_FILE_NAME: &str = "config.toml";
/// Legacy YAML config file name (for auto-migration).
pub const LEGACY_CONFIG_FILE_NAME: &str = "config.yml";
/// Current config schema version.
pub const CURRENT_VERSION: i32 = 15;
/// Reserved status name for reference material.
pub const REFERENCES_STATUS: &str = "references";
/// Reserved status name for soft-deleted tasks.
pub const ARCHIVED_STATUS: &str = "archived";

// ---------------------------------------------------------------------------
// Default collections
// ---------------------------------------------------------------------------

/// Returns the default statuses for a new board.
pub fn default_statuses() -> Vec<StatusConfig> {
    vec![
        StatusConfig { name: "backlog".into(), ..Default::default() },
        StatusConfig { name: "todo".into(), ..Default::default() },
        StatusConfig { name: "in-progress".into(), require_claim: true, ..Default::default() },
        StatusConfig { name: REFERENCES_STATUS.into(), ..Default::default() },
        StatusConfig { name: "review".into(), require_claim: true, ..Default::default() },
        StatusConfig { name: "done".into(), ..Default::default() },
        StatusConfig { name: ARCHIVED_STATUS.into(), ..Default::default() },
    ]
}

/// Returns the default priorities.
pub fn default_priorities() -> Vec<String> {
    vec!["low".into(), "medium".into(), "high".into(), "critical".into()]
}

/// Returns the default age thresholds for the TUI.
pub fn default_age_thresholds() -> Vec<AgeThreshold> {
    vec![
        AgeThreshold { after: "0s".into(),   color: "242".into() },
        AgeThreshold { after: "1h".into(),   color: "34".into()  },
        AgeThreshold { after: "24h".into(),  color: "226".into() },
        AgeThreshold { after: "72h".into(),  color: "208".into() },
        AgeThreshold { after: "168h".into(), color: "196".into() },
    ]
}

/// Returns the default collapsed columns.
pub fn default_collapsed_columns() -> Vec<String> {
    vec![
        "backlog".into(),
        "review".into(),
        REFERENCES_STATUS.into(),
        "done".into(),
        ARCHIVED_STATUS.into(),
    ]
}

/// Returns the default classes of service.
pub fn default_classes() -> Vec<ClassConfig> {
    vec![
        ClassConfig { name: "expedite".into(),   wip_limit: 1, bypass_column_wip: true  },
        ClassConfig { name: "fixed-date".into(), wip_limit: 0, bypass_column_wip: false },
        ClassConfig { name: "standard".into(),   wip_limit: 0, bypass_column_wip: false },
        ClassConfig { name: "intangible".into(), wip_limit: 0, bypass_column_wip: false },
    ]
}

// ---------------------------------------------------------------------------
// Config struct
// ---------------------------------------------------------------------------

/// Kanban board configuration, deserialized from `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: i32,
    pub board: BoardConfig,
    pub tasks_dir: String,
    pub statuses: Vec<StatusConfig>,
    pub priorities: Vec<String>,
    pub defaults: DefaultsConfig,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub wip_limits: HashMap<String, i32>,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub claim_timeout: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<ClassConfig>,

    #[serde(default)]
    pub tui: TuiConfig,

    #[serde(default)]
    pub semantic_search: SemanticSearchConfig,

    pub next_id: i32,

    /// Absolute path to the kanban directory (not serialized).
    #[serde(skip)]
    dir: PathBuf,
}

impl Config {
    // ── Construction ──────────────────────────────────────────────────

    /// Creates a Config with default values for a new board.
    pub fn new_default(name: &str) -> Self {
        Config {
            version: CURRENT_VERSION,
            board: BoardConfig {
                name: name.to_string(),
                description: String::new(),
            },
            tasks_dir: DEFAULT_TASKS_DIR.to_string(),
            statuses: default_statuses(),
            priorities: default_priorities(),
            classes: default_classes(),
            claim_timeout: DEFAULT_CLAIM_TIMEOUT.to_string(),
            tui: TuiConfig {
                title_lines: DEFAULT_TITLE_LINES,
                age_thresholds: default_age_thresholds(),
                hide_empty_columns: DEFAULT_HIDE_EMPTY_COLUMNS,
                collapsed_columns: default_collapsed_columns(),
                reader_max_width: DEFAULT_READER_MAX_WIDTH,
                reader_width_pct: DEFAULT_READER_WIDTH_PCT,
                ..Default::default()
            },
            defaults: DefaultsConfig {
                status: DEFAULT_STATUS.to_string(),
                priority: DEFAULT_PRIORITY.to_string(),
                class: DEFAULT_CLASS.to_string(),
            },
            wip_limits: HashMap::new(),
            semantic_search: SemanticSearchConfig::default(),
            next_id: 1,
            dir: PathBuf::new(),
        }
    }

    // ── Path accessors ───────────────────────────────────────────────

    /// Returns the absolute path to the kanban directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Sets the kanban directory path.
    pub fn set_dir(&mut self, dir: PathBuf) {
        self.dir = dir;
    }

    /// Returns the absolute path to the tasks directory.
    pub fn tasks_path(&self) -> PathBuf {
        self.dir.join(&self.tasks_dir)
    }

    /// Returns the absolute path to the config file.
    pub fn config_path(&self) -> PathBuf {
        self.dir.join(CONFIG_FILE_NAME)
    }

    // ── Status helpers ───────────────────────────────────────────────

    /// Returns the ordered list of status name strings.
    pub fn status_names(&self) -> Vec<String> {
        self.statuses.iter().map(|s| s.name.clone()).collect()
    }

    /// Returns true if the given status has `require_claim` set.
    pub fn status_requires_claim(&self, status: &str) -> bool {
        self.statuses
            .iter()
            .any(|s| s.name == status && s.require_claim)
    }

    /// Returns true if the given status has `require_branch` set.
    pub fn status_requires_branch(&self, status: &str) -> bool {
        self.statuses
            .iter()
            .any(|s| s.name == status && s.require_branch)
    }

    /// Returns whether the given status column should display task
    /// age/duration. Defaults to `true` when not explicitly configured.
    pub fn status_show_duration(&self, status: &str) -> bool {
        for s in &self.statuses {
            if s.name == status {
                return s.show_duration.unwrap_or(true);
            }
        }
        true
    }

    /// Returns true if the given status is a terminal status.
    /// Both the "done" status (immediately before archived) and "archived"
    /// itself are considered terminal.
    pub fn is_terminal_status(&self, s: &str) -> bool {
        let names = self.status_names();
        if names.is_empty() {
            return false;
        }
        if s == ARCHIVED_STATUS {
            return true;
        }
        let last_idx = names.len() - 1;
        if names[last_idx] == ARCHIVED_STATUS && last_idx > 0 {
            return s == names[last_idx - 1];
        }
        s == names[last_idx]
    }

    /// Returns true if the given status is the archived status.
    pub fn is_archived_status(&self, s: &str) -> bool {
        s == ARCHIVED_STATUS
            && self
                .status_names()
                .contains(&ARCHIVED_STATUS.to_string())
    }

    /// Returns statuses for board display (excluding archived).
    pub fn board_statuses(&self) -> Vec<String> {
        self.status_names()
            .into_iter()
            .filter(|s| s != ARCHIVED_STATUS)
            .collect()
    }

    /// Returns non-terminal statuses where work is happening.
    pub fn active_statuses(&self) -> Vec<String> {
        self.status_names()
            .into_iter()
            .filter(|s| !self.is_terminal_status(s))
            .collect()
    }

    /// Returns the index of a status in the configured order, or `None`.
    pub fn status_index(&self, status: &str) -> Option<usize> {
        self.statuses.iter().position(|s| s.name == status)
    }

    // ── Priority helpers ─────────────────────────────────────────────

    /// Returns the index of a priority in the configured order, or `None`.
    pub fn priority_index(&self, priority: &str) -> Option<usize> {
        self.priorities.iter().position(|p| p == priority)
    }

    // ── WIP limit helpers ────────────────────────────────────────────

    /// Returns the WIP limit for a status, or 0 (unlimited).
    pub fn wip_limit(&self, status: &str) -> i32 {
        self.wip_limits.get(status).copied().unwrap_or(0)
    }

    // ── Claim helpers ────────────────────────────────────────────────

    /// Parses the `claim_timeout` string into a [`Duration`].
    /// Returns `Duration::ZERO` (no expiry) if the field is empty or
    /// cannot be parsed.
    pub fn claim_timeout_duration(&self) -> Duration {
        parse_go_duration(&self.claim_timeout).unwrap_or(Duration::ZERO)
    }

    /// Returns parsed age thresholds as `(Duration, color_string)` pairs,
    /// sorted by duration ascending.
    pub fn age_thresholds_parsed(&self) -> Vec<(Duration, String)> {
        self.tui
            .age_thresholds
            .iter()
            .filter_map(|at| {
                parse_go_duration(&at.after).map(|d| (d, at.color.clone()))
            })
            .collect()
    }

    // ── TUI helpers ──────────────────────────────────────────────────

    /// Returns the configured number of title lines for TUI cards.
    /// Defaults to [`DEFAULT_TITLE_LINES`] if the value is unset (0).
    pub fn title_lines(&self) -> i32 {
        if self.tui.title_lines == 0 {
            DEFAULT_TITLE_LINES
        } else {
            self.tui.title_lines
        }
    }

    /// Returns the configured max content width for the reader panel.
    /// Defaults to [`DEFAULT_READER_MAX_WIDTH`] if the value is unset (0).
    pub fn reader_max_width(&self) -> i32 {
        if self.tui.reader_max_width == 0 {
            DEFAULT_READER_MAX_WIDTH
        } else {
            self.tui.reader_max_width
        }
    }

    /// Returns the configured reader panel width as a percentage of terminal width.
    /// Defaults to [`DEFAULT_READER_WIDTH_PCT`] if the value is unset (0).
    pub fn reader_width_pct(&self) -> i32 {
        if self.tui.reader_width_pct == 0 {
            DEFAULT_READER_WIDTH_PCT
        } else {
            self.tui.reader_width_pct
        }
    }

    // ── Class helpers ────────────────────────────────────────────────

    /// Returns the [`ClassConfig`] for the given name, or `None`.
    pub fn class_by_name(&self, name: &str) -> Option<&ClassConfig> {
        self.classes.iter().find(|c| c.name == name)
    }

    /// Returns the list of configured class names in order.
    pub fn class_names(&self) -> Vec<String> {
        self.classes.iter().map(|c| c.name.clone()).collect()
    }

    /// Returns the index of a class name in the configured order, or `None`.
    pub fn class_index(&self, class: &str) -> Option<usize> {
        self.classes.iter().position(|c| c.name == class)
    }

    // ── Validation ───────────────────────────────────────────────────

    /// Validates the config for errors.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.version != CURRENT_VERSION {
            return Err(ConfigError::Invalid(format!(
                "unsupported version {} (expected {})",
                self.version, CURRENT_VERSION
            )));
        }
        if self.board.name.is_empty() {
            return Err(ConfigError::Invalid("board.name is required".into()));
        }
        if self.tasks_dir.is_empty() {
            return Err(ConfigError::Invalid("tasks_dir is required".into()));
        }

        let names = self.status_names();
        if names.len() < 2 {
            return Err(ConfigError::Invalid(
                "at least 2 statuses are required".into(),
            ));
        }
        if has_duplicates(&names) {
            return Err(ConfigError::Invalid(
                "statuses contain duplicates".into(),
            ));
        }
        if self.priorities.is_empty() {
            return Err(ConfigError::Invalid(
                "at least 1 priority is required".into(),
            ));
        }
        if has_duplicates(&self.priorities) {
            return Err(ConfigError::Invalid(
                "priorities contain duplicates".into(),
            ));
        }
        if !names.contains(&self.defaults.status) {
            return Err(ConfigError::Invalid(format!(
                "default status {:?} not in statuses list",
                self.defaults.status
            )));
        }
        if !self.priorities.contains(&self.defaults.priority) {
            return Err(ConfigError::Invalid(format!(
                "default priority {:?} not in priorities list",
                self.defaults.priority
            )));
        }

        // Validate WIP limits.
        for (status, limit) in &self.wip_limits {
            if !names.contains(status) {
                return Err(ConfigError::Invalid(format!(
                    "wip_limits references unknown status {:?}",
                    status
                )));
            }
            if *limit < 0 {
                return Err(ConfigError::Invalid(format!(
                    "wip_limits for {:?} must be >= 0",
                    status
                )));
            }
        }

        // Validate classes.
        if !self.classes.is_empty() {
            let mut seen = std::collections::HashSet::new();
            for cl in &self.classes {
                if cl.name.is_empty() {
                    return Err(ConfigError::Invalid("class name is required".into()));
                }
                if !seen.insert(&cl.name) {
                    return Err(ConfigError::Invalid(format!(
                        "duplicate class name {:?}",
                        cl.name
                    )));
                }
                if cl.wip_limit < 0 {
                    return Err(ConfigError::Invalid(format!(
                        "class {:?} wip_limit must be >= 0",
                        cl.name
                    )));
                }
            }
            if !self.defaults.class.is_empty()
                && !self.classes.iter().any(|c| c.name == self.defaults.class)
            {
                return Err(ConfigError::Invalid(format!(
                    "default class {:?} not in classes list",
                    self.defaults.class
                )));
            }
        }

        // Validate claim_timeout.
        if !self.claim_timeout.is_empty() && parse_go_duration(&self.claim_timeout).is_none() {
            return Err(ConfigError::Invalid(format!(
                "invalid claim_timeout {:?}",
                self.claim_timeout
            )));
        }

        // Validate TUI settings.
        if self.tui.title_lines < 1 || self.tui.title_lines > 3 {
            return Err(ConfigError::Invalid(
                "tui.title_lines must be between 1 and 3".into(),
            ));
        }
        for (i, at) in self.tui.age_thresholds.iter().enumerate() {
            if parse_go_duration(&at.after).is_none() {
                return Err(ConfigError::Invalid(format!(
                    "tui.age_thresholds[{}].after {:?}: invalid duration",
                    i, at.after
                )));
            }
            if at.color.is_empty() {
                return Err(ConfigError::Invalid(format!(
                    "tui.age_thresholds[{}].color is required",
                    i
                )));
            }
        }

        if self.next_id < 1 {
            return Err(ConfigError::Invalid("next_id must be >= 1".into()));
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Sub-config types
// ---------------------------------------------------------------------------

/// Board metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BoardConfig {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// Default values for new tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultsConfig {
    pub status: String,
    pub priority: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub class: String,
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            status: DEFAULT_STATUS.to_string(),
            priority: DEFAULT_PRIORITY.to_string(),
            class: DEFAULT_CLASS.to_string(),
        }
    }
}

/// An age threshold mapping a duration string to an ANSI color code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgeThreshold {
    pub after: String,
    pub color: String,
}

/// TUI-specific display settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub title_lines: i32,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub age_thresholds: Vec<AgeThreshold>,

    #[serde(default, skip_serializing_if = "is_false_bool")]
    pub hide_empty_columns: bool,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub theme: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collapsed_columns: Vec<String>,

    #[serde(default, skip_serializing_if = "is_zero")]
    pub sort_mode: i32,

    #[serde(default, skip_serializing_if = "is_zero")]
    pub time_mode: i32,

    #[serde(default, skip_serializing_if = "is_false_bool")]
    pub list_mode: bool,

    #[serde(default, skip_serializing_if = "is_zero")]
    pub reader_max_width: i32,

    #[serde(default, skip_serializing_if = "is_zero")]
    pub reader_width_pct: i32,

    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub brightness: f32,

    #[serde(default = "default_saturation", skip_serializing_if = "is_default_saturation")]
    pub saturation: f32,
}

fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}

fn default_saturation() -> f32 {
    -0.2
}

fn is_default_saturation(v: &f32) -> bool {
    (*v - (-0.2)).abs() < f32::EPSILON
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            title_lines: DEFAULT_TITLE_LINES,
            age_thresholds: Vec::new(),
            hide_empty_columns: false,
            theme: String::new(),
            collapsed_columns: Vec::new(),
            sort_mode: 0,
            time_mode: 0,
            list_mode: false,
            reader_max_width: 0,
            reader_width_pct: 0,
            brightness: 0.0,
            saturation: -0.2,
        }
    }
}

/// A status column and its enforcement rules.
///
/// Supports deserialization from either a plain string `"backlog"` (old format)
/// or a mapping `{name: backlog, require_claim: true}` (new format) for
/// backward compatibility.
#[derive(Debug, Clone, Serialize, Default)]
pub struct StatusConfig {
    pub name: String,
    #[serde(skip_serializing_if = "is_false_bool")]
    pub require_claim: bool,
    #[serde(skip_serializing_if = "is_false_bool")]
    pub require_branch: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_duration: Option<bool>,
}

impl<'de> Deserialize<'de> for StatusConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de;

        struct StatusConfigVisitor;

        impl<'de> de::Visitor<'de> for StatusConfigVisitor {
            type Value = StatusConfig;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a string or a mapping")
            }

            fn visit_str<E>(self, value: &str) -> Result<StatusConfig, E>
            where
                E: de::Error,
            {
                Ok(StatusConfig {
                    name: value.to_string(),
                    ..Default::default()
                })
            }

            fn visit_map<M>(self, map: M) -> Result<StatusConfig, M::Error>
            where
                M: de::MapAccess<'de>,
            {
                #[derive(Deserialize)]
                struct Helper {
                    name: String,
                    #[serde(default)]
                    require_claim: bool,
                    #[serde(default)]
                    require_branch: bool,
                    #[serde(default)]
                    show_duration: Option<bool>,
                }
                let helper =
                    Helper::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(StatusConfig {
                    name: helper.name,
                    require_claim: helper.require_claim,
                    require_branch: helper.require_branch,
                    show_duration: helper.show_duration,
                })
            }
        }

        deserializer.deserialize_any(StatusConfigVisitor)
    }
}

/// A class of service and its WIP rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassConfig {
    pub name: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub wip_limit: i32,
    #[serde(default, skip_serializing_if = "is_false_bool")]
    pub bypass_column_wip: bool,
}

/// Default semantic search provider.
pub const DEFAULT_SEMANTIC_PROVIDER: &str = "voyage";

/// Semantic search configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_semantic_provider")]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_url: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dimensions: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub input_type: String,
}

fn default_semantic_provider() -> String {
    DEFAULT_SEMANTIC_PROVIDER.to_string()
}

impl Default for SemanticSearchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: DEFAULT_SEMANTIC_PROVIDER.to_string(),
            model: String::new(),
            base_url: String::new(),
            dimensions: 0,
            input_type: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn is_zero(v: &i32) -> bool {
    *v == 0
}

fn is_false_bool(v: &bool) -> bool {
    !v
}

fn has_duplicates(slice: &[String]) -> bool {
    let mut seen = std::collections::HashSet::new();
    for s in slice {
        if !seen.insert(s) {
            return true;
        }
    }
    false
}

/// Parses a Go-style duration string (e.g. "1h", "30m", "0s", "72h", "1h30m").
/// Returns `None` if the string is empty or cannot be parsed.
pub fn parse_go_duration(s: &str) -> Option<Duration> {
    if s.is_empty() {
        return None;
    }

    let mut total_secs: f64 = 0.0;
    let mut rest = s;

    while !rest.is_empty() {
        // Parse numeric part (may include decimal).
        let num_end = rest
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(rest.len());
        if num_end == 0 {
            return None;
        }
        let num: f64 = rest[..num_end].parse().ok()?;
        rest = &rest[num_end..];

        // Parse unit suffix.
        if rest.is_empty() {
            return None; // number without unit
        }
        let (multiplier, unit_len) = if rest.starts_with("ns") {
            (1e-9, 2)
        } else if rest.starts_with("us") || rest.starts_with("\u{00b5}s") {
            (1e-6, 2)
        } else if rest.starts_with("ms") {
            (1e-3, 2)
        } else if rest.starts_with('s') {
            (1.0, 1)
        } else if rest.starts_with('m') {
            (60.0, 1)
        } else if rest.starts_with('h') {
            (3600.0, 1)
        } else {
            return None;
        };

        total_secs += num * multiplier;
        rest = &rest[unit_len..];
    }

    Some(Duration::from_secs_f64(total_secs))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_default_statuses() {
        let cfg = Config::new_default("test");
        let names = cfg.status_names();
        assert_eq!(
            names,
            vec!["backlog", "todo", "in-progress", "references", "review", "done", "archived"]
        );
    }

    #[test]
    fn test_new_default_validates() {
        let cfg = Config::new_default("test");
        cfg.validate().expect("default config should validate");
    }

    #[test]
    fn test_is_terminal_status() {
        let cfg = Config::new_default("test");
        assert!(cfg.is_terminal_status("done"));
        assert!(cfg.is_terminal_status("archived"));
        assert!(!cfg.is_terminal_status("in-progress"));
        assert!(!cfg.is_terminal_status("backlog"));
    }

    #[test]
    fn test_board_statuses_excludes_archived() {
        let cfg = Config::new_default("test");
        let bs = cfg.board_statuses();
        assert!(!bs.contains(&"archived".to_string()));
        assert!(bs.contains(&"done".to_string()));
    }

    #[test]
    fn test_active_statuses_excludes_terminal() {
        let cfg = Config::new_default("test");
        let active = cfg.active_statuses();
        assert!(!active.contains(&"done".to_string()));
        assert!(!active.contains(&"archived".to_string()));
        assert!(active.contains(&"in-progress".to_string()));
    }

    #[test]
    fn test_status_index() {
        let cfg = Config::new_default("test");
        assert_eq!(cfg.status_index("backlog"), Some(0));
        assert_eq!(cfg.status_index("archived"), Some(6));
        assert_eq!(cfg.status_index("nonexistent"), None);
    }

    #[test]
    fn test_priority_index() {
        let cfg = Config::new_default("test");
        assert_eq!(cfg.priority_index("low"), Some(0));
        assert_eq!(cfg.priority_index("critical"), Some(3));
        assert_eq!(cfg.priority_index("nope"), None);
    }

    #[test]
    fn test_class_by_name() {
        let cfg = Config::new_default("test");
        let expedite = cfg.class_by_name("expedite").unwrap();
        assert_eq!(expedite.wip_limit, 1);
        assert!(expedite.bypass_column_wip);
        assert!(cfg.class_by_name("nonexistent").is_none());
    }

    #[test]
    fn test_class_index() {
        let cfg = Config::new_default("test");
        assert_eq!(cfg.class_index("expedite"), Some(0));
        assert_eq!(cfg.class_index("standard"), Some(2));
        assert_eq!(cfg.class_index("nope"), None);
    }

    #[test]
    fn test_claim_timeout_duration() {
        let cfg = Config::new_default("test");
        assert_eq!(cfg.claim_timeout_duration(), Duration::from_secs(3600));
    }

    #[test]
    fn test_claim_timeout_duration_empty() {
        let mut cfg = Config::new_default("test");
        cfg.claim_timeout = String::new();
        assert_eq!(cfg.claim_timeout_duration(), Duration::ZERO);
    }

    #[test]
    fn test_parse_go_duration() {
        assert_eq!(parse_go_duration("1h"), Some(Duration::from_secs(3600)));
        assert_eq!(parse_go_duration("30m"), Some(Duration::from_secs(1800)));
        assert_eq!(parse_go_duration("0s"), Some(Duration::from_secs(0)));
        assert_eq!(
            parse_go_duration("1h30m"),
            Some(Duration::from_secs(5400))
        );
        assert_eq!(parse_go_duration("72h"), Some(Duration::from_secs(259200)));
        assert_eq!(parse_go_duration(""), None);
        assert_eq!(parse_go_duration("invalid"), None);
    }

    #[test]
    fn test_status_requires_claim() {
        let cfg = Config::new_default("test");
        assert!(cfg.status_requires_claim("in-progress"));
        assert!(cfg.status_requires_claim("review"));
        assert!(!cfg.status_requires_claim("backlog"));
    }

    #[test]
    fn test_validate_rejects_bad_version() {
        let mut cfg = Config::new_default("test");
        cfg.version = 999;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_rejects_empty_board_name() {
        let cfg = Config::new_default("");
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_wip_limit() {
        let mut cfg = Config::new_default("test");
        cfg.wip_limits.insert("todo".into(), 5);
        assert_eq!(cfg.wip_limit("todo"), 5);
        assert_eq!(cfg.wip_limit("backlog"), 0);
    }

    #[test]
    fn test_title_lines_default() {
        let cfg = Config::new_default("test");
        assert_eq!(cfg.title_lines(), 2);
    }

    #[test]
    fn test_reader_max_width_default() {
        let cfg = Config::new_default("test");
        assert_eq!(cfg.reader_max_width(), 120);
    }

    #[test]
    fn test_status_config_deserialize_yaml_string() {
        let yaml = "\"backlog\"";
        let sc: StatusConfig = serde_yml::from_str(yaml).unwrap();
        assert_eq!(sc.name, "backlog");
        assert!(!sc.require_claim);
    }

    #[test]
    fn test_status_config_deserialize_yaml_mapping() {
        let yaml = "name: in-progress\nrequire_claim: true";
        let sc: StatusConfig = serde_yml::from_str(yaml).unwrap();
        assert_eq!(sc.name, "in-progress");
        assert!(sc.require_claim);
    }

    #[test]
    fn test_status_config_toml_roundtrip() {
        let cfg = Config::new_default("test");
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let loaded: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(loaded.statuses.len(), cfg.statuses.len());
        assert_eq!(loaded.statuses[2].name, "in-progress");
        assert!(loaded.statuses[2].require_claim);
    }
}
