//! Table (human-readable) output formatting.
//!
//! Uses `comfy_table` for tabular layout and `colored` for ANSI styling.
//! Status/priority colors are aligned with the TUI palette.

use std::io::Write;

use colored::Colorize;
use comfy_table::{Cell, ContentArrangement, Table};

use crate::model::task::Task;

use super::types::{GroupedSummary, LogEntry, Metrics, Overview, StatusSummary};

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

/// Applies status-specific color to a string.
fn color_status(status: &str) -> String {
    match status {
        "backlog" => status.dimmed().to_string(),
        "todo" => status.blue().to_string(),
        "in-progress" => status.yellow().to_string(),
        "review" => status.cyan().to_string(),
        "done" => status.green().to_string(),
        "archived" => status.dimmed().to_string(),
        _ => status.to_string(),
    }
}

/// Applies priority-specific color to a string.
fn color_priority(priority: &str) -> String {
    match priority {
        "critical" => priority.red().bold().to_string(),
        "high" => priority.red().to_string(),
        "medium" => priority.yellow().to_string(),
        "low" => priority.dimmed().to_string(),
        _ => priority.to_string(),
    }
}

/// Returns "@agent" if claimed, or a dim "--" otherwise.
fn claim_display(t: &Task) -> String {
    if t.claimed_by.is_empty() {
        "--".dimmed().to_string()
    } else {
        format!("@{}", t.claimed_by).cyan().bold().to_string()
    }
}

/// Renders tags or a dim "--" placeholder.
fn tags_display(tags: &[String]) -> String {
    if tags.is_empty() {
        "--".dimmed().to_string()
    } else {
        tags.join(",").truecolor(135, 175, 135).to_string() // sage green
    }
}

/// Returns the due date as a string or a dim "--".
fn due_display(due: &Option<chrono::NaiveDate>) -> String {
    match due {
        Some(d) => d.to_string(),
        None => "--".dimmed().to_string(),
    }
}

/// Returns `s` if non-empty, otherwise a dim "--".
fn string_or_dash(s: &str) -> String {
    if s.is_empty() {
        "--".dimmed().to_string()
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// Public formatting functions
// ---------------------------------------------------------------------------

/// Renders a list of tasks as a formatted table.
///
/// Columns: ID | STATUS | PRIORITY | TITLE | CLAIMED | TAGS | DUE
pub fn task_table(w: &mut impl Write, tasks: &[Task]) {
    if tasks.is_empty() {
        let _ = writeln!(w, "No tasks found.");
        return;
    }

    let mut table = Table::new();
    table
        .set_content_arrangement(ContentArrangement::Dynamic)
        .load_preset(comfy_table::presets::NOTHING)
        .set_header(vec![
            Cell::new("ID"),
            Cell::new("STATUS"),
            Cell::new("PRIORITY"),
            Cell::new("TITLE"),
            Cell::new("CLAIMED"),
            Cell::new("TAGS"),
            Cell::new("DUE"),
        ]);

    for t in tasks {
        let title = if t.title.len() > 48 {
            format!("{}...", &t.title[..45])
        } else {
            t.title.clone()
        };

        table.add_row(vec![
            Cell::new(t.id),
            Cell::new(color_status(&t.status)),
            Cell::new(color_priority(&t.priority)),
            Cell::new(title),
            Cell::new(claim_display(t)),
            Cell::new(tags_display(&t.tags)),
            Cell::new(due_display(&t.due)),
        ]);
    }

    let _ = writeln!(w, "{table}");
}

/// Renders a single task with full detail (all fields).
pub fn task_detail(w: &mut impl Write, t: &Task) {
    if !t.file.is_empty() {
        let _ = writeln!(w, "{}", t.file.dimmed());
    }

    let title_line = format!("Task #{}: {}", t.id, t.title);
    let _ = writeln!(w, "{}", title_line.bold());
    let _ = writeln!(w, "{}", "\u{2500}".repeat(title_line.len()));

    print_field(w, "Status", &color_status(&t.status));
    print_field(w, "Priority", &color_priority(&t.priority));

    if !t.class.is_empty() {
        print_field(w, "Class", &t.class);
    }

    print_field(w, "Assignee", &string_or_dash(&t.assignee));

    if t.tags.is_empty() {
        print_field(w, "Tags", &"--".dimmed().to_string());
    } else {
        print_field(w, "Tags", &tags_display(&t.tags));
    }

    print_field(w, "Due", &due_display(&t.due));
    print_field(w, "Estimate", &string_or_dash(&t.estimate));
    print_field(w, "Created", &t.created.format("%Y-%m-%d %H:%M").to_string());
    print_field(w, "Updated", &t.updated.format("%Y-%m-%d %H:%M").to_string());

    if let Some(started) = &t.started {
        print_field(w, "Started", &started.format("%Y-%m-%d %H:%M").to_string());
    }

    if let Some(completed) = &t.completed {
        print_field(
            w,
            "Completed",
            &completed.format("%Y-%m-%d %H:%M").to_string(),
        );
        let lead = *completed - t.created;
        print_field(w, "Lead time", &format_duration(lead));
        if let Some(started) = &t.started {
            let cycle = *completed - *started;
            print_field(w, "Cycle time", &format_duration(cycle));
        }
    }

    if !t.claimed_by.is_empty() {
        let mut claim_str = format!("@{}", t.claimed_by).cyan().bold().to_string();
        if let Some(at) = &t.claimed_at {
            claim_str.push_str(&format!(" (since {})", at.format("%Y-%m-%d %H:%M")));
        }
        print_field(w, "Claimed by", &claim_str);
    }

    if !t.branch.is_empty() {
        print_field(w, "Branch", &t.branch);
    }
    if !t.worktree.is_empty() {
        print_field(w, "Worktree", &t.worktree);
    }

    if !t.body.is_empty() {
        let _ = writeln!(w);
        let _ = writeln!(w, "{}", t.body);
    }
}

/// Renders a board overview/summary as a formatted dashboard.
pub fn overview_table(w: &mut impl Write, overview: &Overview) {
    let _ = writeln!(w, "{}", overview.board_name.bold());
    let _ = writeln!(w, "Total: {} tasks\n", overview.total_tasks);

    // Status summary table.
    let mut table = Table::new();
    table
        .set_content_arrangement(ContentArrangement::Dynamic)
        .load_preset(comfy_table::presets::NOTHING)
        .set_header(vec![
            Cell::new("STATUS"),
            Cell::new("COUNT"),
            Cell::new("WIP"),
            Cell::new("BLOCKED"),
            Cell::new("OVERDUE"),
        ]);

    for ss in &overview.statuses {
        let wip = if ss.wip_limit > 0 {
            format!("{}/{}", ss.count, ss.wip_limit)
        } else {
            "--".dimmed().to_string()
        };
        table.add_row(vec![
            Cell::new(color_status(&ss.status)),
            Cell::new(ss.count),
            Cell::new(wip),
            Cell::new(ss.blocked),
            Cell::new(ss.overdue),
        ]);
    }
    let _ = writeln!(w, "{table}");

    // Priority summary.
    let _ = writeln!(w);
    let mut prio_table = Table::new();
    prio_table
        .set_content_arrangement(ContentArrangement::Dynamic)
        .load_preset(comfy_table::presets::NOTHING)
        .set_header(vec![Cell::new("PRIORITY"), Cell::new("COUNT")]);

    for pc in &overview.priorities {
        prio_table.add_row(vec![
            Cell::new(color_priority(&pc.priority)),
            Cell::new(pc.count),
        ]);
    }
    let _ = writeln!(w, "{prio_table}");

    // Class summary (only if non-empty).
    if !overview.classes.is_empty() {
        let _ = writeln!(w);
        let mut class_table = Table::new();
        class_table
            .set_content_arrangement(ContentArrangement::Dynamic)
            .load_preset(comfy_table::presets::NOTHING)
            .set_header(vec![Cell::new("CLASS"), Cell::new("COUNT")]);

        for cc in &overview.classes {
            class_table.add_row(vec![Cell::new(&cc.class), Cell::new(cc.count)]);
        }
        let _ = writeln!(w, "{class_table}");
    }
}

/// Renders flow metrics as a formatted dashboard.
pub fn metrics_table(w: &mut impl Write, metrics: &Metrics) {
    let _ = writeln!(w, "{}\n", "Flow Metrics".bold());

    print_field(
        w,
        "Throughput 7d",
        &format!("{} tasks", metrics.throughput_7d),
    );
    print_field(
        w,
        "Throughput 30d",
        &format!("{} tasks", metrics.throughput_30d),
    );
    print_field(
        w,
        "Avg lead time",
        &format_optional_hours(metrics.avg_lead_time_hours),
    );
    print_field(
        w,
        "Avg cycle time",
        &format_optional_hours(metrics.avg_cycle_time_hours),
    );
    print_field(
        w,
        "Flow efficiency",
        &format_optional_percent(metrics.flow_efficiency),
    );

    if !metrics.aging_items.is_empty() {
        let _ = writeln!(w);
        let mut table = Table::new();
        table
            .set_content_arrangement(ContentArrangement::Dynamic)
            .load_preset(comfy_table::presets::NOTHING)
            .set_header(vec![
                Cell::new("ID"),
                Cell::new("STATUS"),
                Cell::new("TITLE"),
                Cell::new("AGE"),
            ]);

        for a in &metrics.aging_items {
            let title = if a.title.len() > 38 {
                format!("{}...", &a.title[..35])
            } else {
                a.title.clone()
            };
            let age = format_duration(chrono::Duration::seconds(
                (a.age_hours * 3600.0) as i64,
            ));
            table.add_row(vec![
                Cell::new(a.id),
                Cell::new(color_status(&a.status)),
                Cell::new(title),
                Cell::new(age),
            ]);
        }
        let _ = writeln!(w, "{table}");
    }
}

/// Renders activity log entries as a formatted table.
pub fn activity_log_table(w: &mut impl Write, entries: &[LogEntry]) {
    if entries.is_empty() {
        let _ = writeln!(w, "No activity log entries found.");
        return;
    }

    let mut table = Table::new();
    table
        .set_content_arrangement(ContentArrangement::Dynamic)
        .load_preset(comfy_table::presets::NOTHING)
        .set_header(vec![
            Cell::new("TIMESTAMP"),
            Cell::new("ACTION"),
            Cell::new("TASK"),
            Cell::new("DETAIL"),
        ]);

    for e in entries {
        table.add_row(vec![
            Cell::new(e.timestamp.format("%Y-%m-%d %H:%M:%S")),
            Cell::new(&e.action),
            Cell::new(e.task_id),
            Cell::new(&e.detail),
        ]);
    }

    let _ = writeln!(w, "{table}");
}

/// Renders a grouped board view with per-group status breakdowns.
pub fn grouped_table(w: &mut impl Write, grouped: &GroupedSummary) {
    if grouped.groups.is_empty() {
        let _ = writeln!(w, "No groups found.");
        return;
    }

    for (i, g) in grouped.groups.iter().enumerate() {
        if i > 0 {
            let _ = writeln!(w);
        }
        let title = format!("{} ({} tasks)", g.key, g.total);
        let _ = writeln!(w, "{}", title.bold());

        for ss in &g.statuses {
            if ss.count == 0 {
                continue;
            }
            let _ = writeln!(w, "  {:<16} {}", color_status(&ss.status), ss.count);
        }
    }
}

/// Prints a simple info message line.
pub fn messagef(w: &mut impl Write, msg: &str) {
    let _ = writeln!(w, "{msg}");
}

/// Formats a `chrono::Duration` as human-readable "Xd Yh" or "Xh Ym".
pub fn format_duration(d: chrono::Duration) -> String {
    let total_secs = d.num_seconds().unsigned_abs();
    let total_minutes = total_secs / 60;
    let total_hours = total_minutes / 60;
    let days = total_hours / 24;
    let hours = total_hours % 24;
    let minutes = total_minutes % 60;

    if days > 0 {
        format!("{days}d {hours}h")
    } else {
        format!("{hours}h {minutes}m")
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn print_field(w: &mut impl Write, label: &str, value: &str) {
    let _ = writeln!(w, "  {:<12} {value}", format!("{label}:"));
}

fn format_optional_hours(h: Option<f64>) -> String {
    match h {
        Some(hours) => format_duration(chrono::Duration::seconds((hours * 3600.0) as i64)),
        None => "--".dimmed().to_string(),
    }
}

fn format_optional_percent(f: Option<f64>) -> String {
    match f {
        Some(val) => format!("{:.1}%", val * 100.0),
        None => "--".dimmed().to_string(),
    }
}

#[allow(dead_code)]
fn make_status_summary(status: &str, count: i32, wip_limit: i32) -> StatusSummary {
    StatusSummary {
        status: status.to_string(),
        count,
        wip_limit,
        blocked: 0,
        overdue: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_task() -> Task {
        Task {
            id: 1,
            title: "Test task".to_string(),
            status: "todo".to_string(),
            priority: "high".to_string(),
            created: Utc::now(),
            updated: Utc::now(),
            started: None,
            completed: None,
            assignee: String::new(),
            tags: vec!["layer-3".to_string()],
            due: None,
            estimate: String::new(),
            parent: None,
            depends_on: vec![],
            blocked: false,
            block_reason: String::new(),
            claimed_by: "agent-fox".to_string(),
            claimed_at: None,
            class: String::new(),
            branch: String::new(),
            worktree: String::new(),
            body: "A task body.".to_string(),
            file: String::new(),
        }
    }

    #[test]
    fn task_table_empty() {
        let mut buf = Vec::new();
        task_table(&mut buf, &[]);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("No tasks found"));
    }

    #[test]
    fn task_table_renders_header_and_row() {
        let mut buf = Vec::new();
        task_table(&mut buf, &[sample_task()]);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("ID"));
        assert!(output.contains("STATUS"));
        assert!(output.contains("Test task"));
    }

    #[test]
    fn task_detail_renders_fields() {
        let mut buf = Vec::new();
        let t = sample_task();
        task_detail(&mut buf, &t);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Task #1: Test task"));
        assert!(output.contains("A task body."));
    }

    #[test]
    fn format_duration_days() {
        let d = chrono::Duration::hours(50);
        assert_eq!(format_duration(d), "2d 2h");
    }

    #[test]
    fn format_duration_hours_minutes() {
        let d = chrono::Duration::minutes(95);
        assert_eq!(format_duration(d), "1h 35m");
    }

    #[test]
    fn format_duration_zero() {
        let d = chrono::Duration::zero();
        assert_eq!(format_duration(d), "0h 0m");
    }

    #[test]
    fn activity_log_table_empty() {
        let mut buf = Vec::new();
        activity_log_table(&mut buf, &[]);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("No activity log entries found"));
    }

    #[test]
    fn grouped_table_empty() {
        let mut buf = Vec::new();
        let gs = GroupedSummary { groups: vec![] };
        grouped_table(&mut buf, &gs);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("No groups found"));
    }
}
