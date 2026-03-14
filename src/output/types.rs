//! Shared types used by both the board and output modules.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Metrics for a single status column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSummary {
    pub status: String,
    pub count: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub wip_limit: i32,
    pub blocked: i32,
    pub overdue: i32,
}

/// Count for a priority level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityCount {
    pub priority: String,
    pub count: i32,
}

/// Count for a class of service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassCount {
    pub class: String,
    pub count: i32,
}

/// Aggregate board overview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Overview {
    pub board_name: String,
    pub total_tasks: i32,
    pub statuses: Vec<StatusSummary>,
    pub priorities: Vec<PriorityCount>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<ClassCount>,
}

/// Flow metrics computed from board history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metrics {
    pub throughput_7d: i32,
    pub throughput_30d: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_lead_time_hours: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_cycle_time_hours: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_efficiency: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aging_items: Vec<AgingItem>,
}

/// A work item that has started but not completed (aging in WIP).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgingItem {
    pub id: i32,
    pub title: String,
    pub status: String,
    pub age_hours: f64,
}

/// A single entry in the activity log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub task_id: i32,
    pub detail: String,
}

/// Grouped board view with per-group status breakdowns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupedSummary {
    pub groups: Vec<GroupSummary>,
}

/// One group within a grouped view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupSummary {
    pub key: String,
    pub statuses: Vec<StatusSummary>,
    pub total: i32,
}

use crate::util::serde_helpers::is_zero;
