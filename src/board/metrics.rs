//! Board metrics (cycle time, throughput, lead time, flow efficiency, aging).

use chrono::{DateTime, Duration, Utc};

use crate::model::config::Config;
use crate::model::task::Task;

/// Summary metrics for the board.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct BoardMetrics {
    pub total: usize,
    pub by_status: Vec<StatusCount>,
    pub by_priority: Vec<PriorityCount>,
    pub avg_cycle_time_hours: Option<f64>,
    pub throughput_per_week: Option<f64>,
}

/// Count of tasks in a status.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StatusCount {
    pub status: String,
    pub count: usize,
}

/// Count of tasks at a priority level.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PriorityCount {
    pub priority: String,
    pub count: usize,
}

/// Compute board metrics for the given tasks.
pub fn compute(tasks: &[Task], cfg: &Config, since: Option<DateTime<Utc>>) -> BoardMetrics {
    let filtered: Vec<&Task> = if let Some(since) = since {
        tasks.iter().filter(|t| t.updated >= since).collect()
    } else {
        tasks.iter().collect()
    };

    let total = filtered.len();

    let mut by_status: Vec<StatusCount> = Vec::new();
    for status_name in cfg.status_names() {
        let count = filtered
            .iter()
            .filter(|t| t.status == *status_name)
            .count();
        by_status.push(StatusCount {
            status: status_name.to_string(),
            count,
        });
    }

    let mut by_priority: Vec<PriorityCount> = Vec::new();
    for priority in &cfg.priorities {
        let count = filtered
            .iter()
            .filter(|t| t.priority == *priority)
            .count();
        by_priority.push(PriorityCount {
            priority: priority.clone(),
            count,
        });
    }

    // Compute average cycle time for completed tasks.
    let completed: Vec<&&Task> = filtered
        .iter()
        .filter(|t| t.started.is_some() && t.completed.is_some())
        .collect();

    let avg_cycle_time_hours = if completed.is_empty() {
        None
    } else {
        let total_hours: f64 = completed
            .iter()
            .map(|t| {
                let started = t.started.unwrap();
                let completed_at = t.completed.unwrap();
                completed_at
                    .signed_duration_since(started)
                    .num_milliseconds() as f64
                    / 3_600_000.0
            })
            .sum();
        Some(total_hours / completed.len() as f64)
    };

    BoardMetrics {
        total,
        by_status,
        by_priority,
        avg_cycle_time_hours,
        throughput_per_week: None, // Computed by compute_flow_metrics
    }
}

// ---------------------------------------------------------------------------
// Full flow metrics (matching Go's ComputeMetrics)
// ---------------------------------------------------------------------------

/// Aggregate board flow metrics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FlowMetrics {
    /// Number of tasks completed in the last 7 days.
    pub throughput_7d: i32,
    /// Number of tasks completed in the last 30 days.
    pub throughput_30d: i32,
    /// Average hours from created to completed (for completed tasks).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_lead_time_hours: Option<f64>,
    /// Average hours from started to completed (for completed tasks with start time).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_cycle_time_hours: Option<f64>,
    /// Ratio of cycle_time / lead_time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_efficiency: Option<f64>,
    /// Non-terminal tasks sorted by age (hours since started).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aging_items: Vec<AgingItem>,
}

/// A work item that has started but not completed, with its age.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AgingItem {
    pub id: i32,
    pub title: String,
    pub status: String,
    pub age_hours: f64,
}

/// Computes aggregate flow metrics from all tasks.
///
/// - **throughput_7d / 30d**: count of tasks completed within the window.
/// - **avg_lead_time**: average hours from `created` to `completed`.
/// - **avg_cycle_time**: average hours from `started` to `completed`.
/// - **flow_efficiency**: `cycle_time / lead_time`.
/// - **aging_items**: non-terminal tasks that have started but not completed,
///   sorted by age (hours since `started`).
pub fn compute_flow_metrics(cfg: &Config, tasks: &[Task], now: DateTime<Utc>) -> FlowMetrics {
    let mut m = FlowMetrics {
        throughput_7d: 0,
        throughput_30d: 0,
        avg_lead_time_hours: None,
        avg_cycle_time_hours: None,
        flow_efficiency: None,
        aging_items: Vec::new(),
    };

    let window_7 = now - Duration::days(7);
    let window_30 = now - Duration::days(30);

    let mut lead_sum: f64 = 0.0;
    let mut lead_count: i32 = 0;
    let mut cycle_sum: f64 = 0.0;
    let mut cycle_count: i32 = 0;

    for t in tasks {
        if let Some(completed) = t.completed {
            if completed > window_7 {
                m.throughput_7d += 1;
            }
            if completed > window_30 {
                m.throughput_30d += 1;
            }

            let lead_hours = completed
                .signed_duration_since(t.created)
                .num_milliseconds() as f64
                / 3_600_000.0;
            lead_sum += lead_hours;
            lead_count += 1;

            if let Some(started) = t.started {
                let cycle_hours = completed
                    .signed_duration_since(started)
                    .num_milliseconds() as f64
                    / 3_600_000.0;
                cycle_sum += cycle_hours;
                cycle_count += 1;
            }
        }

        // Aging: started but not completed, not in terminal status.
        if t.started.is_some() && t.completed.is_none() && !cfg.is_terminal_status(&t.status) {
            let started = t.started.unwrap();
            let age_hours = now
                .signed_duration_since(started)
                .num_milliseconds() as f64
                / 3_600_000.0;
            m.aging_items.push(AgingItem {
                id: t.id,
                title: t.title.clone(),
                status: t.status.clone(),
                age_hours,
            });
        }
    }

    if lead_count > 0 {
        m.avg_lead_time_hours = Some(lead_sum / lead_count as f64);
    }
    if cycle_count > 0 {
        m.avg_cycle_time_hours = Some(cycle_sum / cycle_count as f64);
    }
    if let (Some(lead), Some(cycle)) = (m.avg_lead_time_hours, m.avg_cycle_time_hours) {
        if lead > 0.0 {
            m.flow_efficiency = Some(cycle / lead);
        }
    }

    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(id: i32) -> Task {
        Task {
            id,
            title: format!("Task {}", id),
            status: "todo".to_string(),
            priority: "medium".to_string(),
            created: Utc::now() - Duration::days(10),
            updated: Utc::now(),
            ..Default::default()
        }
    }

    #[test]
    fn test_throughput() {
        let now = Utc::now();
        let cfg = Config::new_default("test");

        let mut t1 = make_task(1);
        t1.completed = Some(now - Duration::days(3));
        let mut t2 = make_task(2);
        t2.completed = Some(now - Duration::days(15));
        let mut t3 = make_task(3);
        t3.completed = Some(now - Duration::days(45));
        let tasks = vec![t1, t2, t3];

        let m = compute_flow_metrics(&cfg, &tasks, now);
        assert_eq!(m.throughput_7d, 1);
        assert_eq!(m.throughput_30d, 2);
    }

    #[test]
    fn test_aging_items() {
        let now = Utc::now();
        let cfg = Config::new_default("test");

        let mut t1 = make_task(1);
        t1.status = "in-progress".to_string();
        t1.started = Some(now - Duration::hours(48));
        let tasks = vec![t1];

        let m = compute_flow_metrics(&cfg, &tasks, now);
        assert_eq!(m.aging_items.len(), 1);
        assert!(m.aging_items[0].age_hours >= 47.0);
    }

    #[test]
    fn test_flow_efficiency() {
        let now = Utc::now();
        let cfg = Config::new_default("test");

        let mut t1 = make_task(1);
        t1.created = now - Duration::hours(100);
        t1.started = Some(now - Duration::hours(50));
        t1.completed = Some(now);
        let tasks = vec![t1];

        let m = compute_flow_metrics(&cfg, &tasks, now);
        assert!(m.avg_lead_time_hours.is_some());
        assert!(m.avg_cycle_time_hours.is_some());
        assert!(m.flow_efficiency.is_some());
        // cycle = 50h, lead = 100h => efficiency = 0.5
        let eff = m.flow_efficiency.unwrap();
        assert!((eff - 0.5).abs() < 0.01);
    }
}
