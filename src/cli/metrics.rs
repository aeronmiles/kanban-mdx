//! `kbmdx metrics` — show board metrics.

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::task;
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct MetricsArgs {
    /// Show metrics since this date (YYYY-MM-DD)
    #[arg(long)]
    pub since: Option<String>,
    /// Filter by parent task ID
    #[arg(long)]
    pub parent: Option<i32>,
}

pub fn run(cli: &Cli, args: MetricsArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;

    let (all_tasks, _) = task::read_all_lenient(&cfg.tasks_path())
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;

    let since = if let Some(ref s) = args.since {
        let date = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| {
            CliError::newf(ErrorCode::InvalidDate, format!("invalid date: {s}"))
        })?;
        Some(date.and_hms_opt(0, 0, 0).unwrap().and_utc())
    } else {
        None
    };

    let metrics = crate::board::metrics::compute(&all_tasks, &cfg, since);
    let flow = crate::board::metrics::compute_flow_metrics(&cfg, &all_tasks, chrono::Utc::now());

    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            let combined = serde_json::json!({
                "total": metrics.total,
                "by_status": metrics.by_status,
                "by_priority": metrics.by_priority,
                "avg_cycle_time_hours": metrics.avg_cycle_time_hours,
                "throughput_7d": flow.throughput_7d,
                "throughput_30d": flow.throughput_30d,
                "avg_lead_time_hours": flow.avg_lead_time_hours,
                "flow_efficiency": flow.flow_efficiency,
                "aging_items": flow.aging_items,
            });
            crate::output::json::json(&mut stdout, &combined)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        _ => {
            use std::io::Write;
            writeln!(stdout, "Total tasks: {}", metrics.total).unwrap_or(());
            writeln!(stdout, "\nBy status:").unwrap_or(());
            for s in &metrics.by_status {
                writeln!(stdout, "  {}: {}", s.status, s.count).unwrap_or(());
            }
            writeln!(stdout, "\nBy priority:").unwrap_or(());
            for p in &metrics.by_priority {
                writeln!(stdout, "  {}: {}", p.priority, p.count).unwrap_or(());
            }
            if let Some(cycle) = metrics.avg_cycle_time_hours {
                writeln!(stdout, "\nAvg cycle time: {:.1}h", cycle).unwrap_or(());
            }
            if flow.throughput_7d > 0 || flow.throughput_30d > 0 {
                writeln!(stdout, "\nThroughput:").unwrap_or(());
                writeln!(stdout, "  7d:  {} tasks", flow.throughput_7d).unwrap_or(());
                writeln!(stdout, "  30d: {} tasks", flow.throughput_30d).unwrap_or(());
            }
            if let Some(lead) = flow.avg_lead_time_hours {
                writeln!(stdout, "\nAvg lead time: {:.1}h", lead).unwrap_or(());
            }
            if let Some(eff) = flow.flow_efficiency {
                writeln!(stdout, "Flow efficiency: {:.0}%", eff * 100.0).unwrap_or(());
            }
            if !flow.aging_items.is_empty() {
                writeln!(stdout, "\nAging work:").unwrap_or(());
                for item in &flow.aging_items {
                    writeln!(stdout, "  #{} [{}] {:.0}h - {}", item.id, item.status, item.age_hours, item.title).unwrap_or(());
                }
            }
        }
    }
    Ok(())
}
