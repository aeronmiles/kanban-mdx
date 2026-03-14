//! Compact (one-line-per-record) output formatting.
//!
//! Produces ~70% fewer tokens than JSON, optimised for AI agent consumption.

use std::io::Write;

use crate::model::task::Task;

use super::formatters::{self, format_duration};
use super::types::{GroupedSummary, LogEntry, Metrics, Overview};

// ---------------------------------------------------------------------------
// Public formatting functions
// ---------------------------------------------------------------------------

/// Renders a list of tasks in one-line-per-record compact format.
///
/// Each line: `#ID [status/priority] title @claimer (tags) due:YYYY-MM-DD`
pub fn task_compact(w: &mut impl Write, tasks: &[Task]) {
    if tasks.is_empty() {
        let _ = writeln!(w, "No tasks found.");
        return;
    }

    for t in tasks {
        let _ = writeln!(w, "{}", format_task_line(t));
    }
}

/// Renders a single task with detail in compact format.
pub fn task_detail_compact(w: &mut impl Write, t: &Task) {
    if !t.file.is_empty() {
        let _ = writeln!(w, "{}", t.file);
    }

    let mut line = format_task_line(t);
    if !t.estimate.is_empty() {
        line.push_str(&format!(" est:{}", t.estimate));
    }
    if !t.branch.is_empty() {
        line.push_str(&format!(" branch:{}", t.branch));
    }
    if !t.worktree.is_empty() {
        line.push_str(&format!(" worktree:{}", t.worktree));
    }
    let _ = writeln!(w, "{line}");

    // Timestamps line.
    let mut ts = format!(
        "  created:{} updated:{}",
        t.created.format("%Y-%m-%d"),
        t.updated.format("%Y-%m-%d"),
    );
    if let Some(started) = &t.started {
        ts.push_str(&format!(" started:{}", started.format("%Y-%m-%d")));
    }
    if let Some(completed) = &t.completed {
        ts.push_str(&format!(" completed:{}", completed.format("%Y-%m-%d")));
    }
    let _ = writeln!(w, "{ts}");

    if !t.body.is_empty() {
        for body_line in t.body.lines() {
            let _ = writeln!(w, "  {body_line}");
        }
    }
}

/// Renders a board overview/summary in compact format.
pub fn overview_compact(w: &mut impl Write, overview: &Overview) {
    let _ = writeln!(w, "{} ({} tasks)", overview.board_name, overview.total_tasks);

    for ss in &overview.statuses {
        let mut line = format!("  {}: {}", ss.status, ss.count);
        if ss.wip_limit > 0 {
            line.push_str(&format!("/{}", ss.wip_limit));
        }
        let mut annotations = Vec::new();
        if ss.blocked > 0 {
            annotations.push(format!("{} blocked", ss.blocked));
        }
        if ss.overdue > 0 {
            annotations.push(format!("{} overdue", ss.overdue));
        }
        if !annotations.is_empty() {
            line.push_str(&format!(" ({})", annotations.join(", ")));
        }
        let _ = writeln!(w, "{line}");
    }

    if !overview.priorities.is_empty() {
        let parts: Vec<String> = overview
            .priorities
            .iter()
            .map(|pc| format!("{}={}", pc.priority, pc.count))
            .collect();
        let _ = writeln!(w, "Priority: {}", parts.join(" "));
    }
}

/// Renders flow metrics in compact format.
pub fn metrics_compact(w: &mut impl Write, metrics: &Metrics) {
    let parts = vec![
        format!(
            "Throughput: {}/7d {}/30d",
            metrics.throughput_7d, metrics.throughput_30d
        ),
        format!("Lead: {}", compact_duration(metrics.avg_lead_time_hours)),
        format!("Cycle: {}", compact_duration(metrics.avg_cycle_time_hours)),
        format!(
            "Efficiency: {}",
            format_optional_percent(metrics.flow_efficiency)
        ),
    ];
    let _ = writeln!(w, "{}", parts.join(" | "));

    for a in &metrics.aging_items {
        let title = if a.title.len() > 60 {
            format!("{}...", &a.title[..57])
        } else {
            a.title.clone()
        };
        let age = format_duration(chrono::Duration::seconds((a.age_hours * 3600.0) as i64));
        let _ = writeln!(w, "Aging: #{} [{}] {} ({age})", a.id, a.status, title);
    }
}

/// Renders activity log entries in compact format.
pub fn activity_log_compact(w: &mut impl Write, entries: &[LogEntry]) {
    if entries.is_empty() {
        let _ = writeln!(w, "No activity log entries found.");
        return;
    }

    for e in entries {
        let _ = writeln!(
            w,
            "{} {} #{} {}",
            e.timestamp.format("%Y-%m-%d %H:%M:%S"),
            e.action,
            e.task_id,
            e.detail,
        );
    }
}

/// Renders a grouped board view in compact format.
pub fn grouped_compact(w: &mut impl Write, grouped: &GroupedSummary) {
    if grouped.groups.is_empty() {
        let _ = writeln!(w, "No groups found.");
        return;
    }

    for (i, g) in grouped.groups.iter().enumerate() {
        if i > 0 {
            let _ = writeln!(w);
        }
        let _ = writeln!(w, "{} ({} tasks)", g.key, g.total);
        for ss in &g.statuses {
            if ss.count == 0 {
                continue;
            }
            let _ = writeln!(w, "  {}: {}", ss.status, ss.count);
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Builds the one-line representation of a task.
fn format_task_line(t: &Task) -> String {
    let mut line = format!("#{} [{}/{}] {}", t.id, t.status, t.priority, t.title);

    if !t.claimed_by.is_empty() {
        line.push_str(&format!(" @{}", t.claimed_by));
    }
    if !t.branch.is_empty() {
        line.push_str(&format!(" branch:{}", t.branch));
    }
    if !t.tags.is_empty() {
        line.push_str(&format!(" ({})", t.tags.join(", ")));
    }
    if let Some(due) = &t.due {
        line.push_str(&format!(" due:{due}"));
    }

    line
}

fn compact_duration(h: Option<f64>) -> String {
    formatters::format_optional_hours(h, "--")
}

fn format_optional_percent(f: Option<f64>) -> String {
    formatters::format_optional_percent(f, "--")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::types::{PriorityCount, StatusSummary};
    use chrono::Utc;

    fn sample_task() -> Task {
        Task {
            id: 42,
            title: "Implement compact output".to_string(),
            status: "in-progress".to_string(),
            priority: "high".to_string(),
            created: Utc::now(),
            updated: Utc::now(),
            started: None,
            completed: None,
            assignee: String::new(),
            tags: vec!["layer-3".to_string(), "feature".to_string()],
            due: None,
            estimate: String::new(),
            parent: None,
            depends_on: vec![],
            blocked: false,
            block_reason: String::new(),
            claimed_by: "agent-fox".to_string(),
            claimed_at: None,
            class: String::new(),
            branch: "task/42-compact".to_string(),
            worktree: String::new(),
            body: String::new(),
            file: String::new(),
        }
    }

    #[test]
    fn task_compact_empty() {
        let mut buf = Vec::new();
        task_compact(&mut buf, &[]);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("No tasks found"));
    }

    #[test]
    fn task_compact_single() {
        let mut buf = Vec::new();
        task_compact(&mut buf, &[sample_task()]);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("#42"));
        assert!(output.contains("[in-progress/high]"));
        assert!(output.contains("Implement compact output"));
        assert!(output.contains("@agent-fox"));
        assert!(output.contains("branch:task/42-compact"));
        assert!(output.contains("(layer-3, feature)"));
    }

    #[test]
    fn task_detail_compact_with_body() {
        let mut t = sample_task();
        t.body = "Line one\nLine two".to_string();
        t.estimate = "2h".to_string();
        let mut buf = Vec::new();
        task_detail_compact(&mut buf, &t);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("est:2h"));
        assert!(output.contains("created:"));
        assert!(output.contains("  Line one"));
        assert!(output.contains("  Line two"));
    }

    #[test]
    fn overview_compact_renders() {
        let overview = Overview {
            board_name: "My Board".to_string(),
            total_tasks: 10,
            statuses: vec![StatusSummary {
                status: "todo".to_string(),
                count: 5,
                wip_limit: 8,
                blocked: 1,
                overdue: 0,
            }],
            priorities: vec![PriorityCount {
                priority: "high".to_string(),
                count: 3,
            }],
            classes: vec![],
        };
        let mut buf = Vec::new();
        overview_compact(&mut buf, &overview);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("My Board (10 tasks)"));
        assert!(output.contains("todo: 5/8"));
        assert!(output.contains("1 blocked"));
        assert!(output.contains("Priority: high=3"));
    }

    #[test]
    fn metrics_compact_renders() {
        let metrics = Metrics {
            throughput_7d: 5,
            throughput_30d: 20,
            avg_lead_time_hours: Some(48.0),
            avg_cycle_time_hours: None,
            flow_efficiency: Some(0.65),
            aging_items: vec![],
        };
        let mut buf = Vec::new();
        metrics_compact(&mut buf, &metrics);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Throughput: 5/7d 20/30d"));
        assert!(output.contains("Lead: 2d 0h"));
        assert!(output.contains("Cycle: --"));
        assert!(output.contains("Efficiency: 65.0%"));
    }

    #[test]
    fn activity_log_compact_empty() {
        let mut buf = Vec::new();
        activity_log_compact(&mut buf, &[]);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("No activity log entries found"));
    }
}
