//! Shared formatting helpers used by both compact and table output.

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

/// Converts optional hours to a formatted duration string, or `fallback` if None.
pub fn format_optional_hours(h: Option<f64>, fallback: &str) -> String {
    match h {
        Some(hours) => format_duration(chrono::Duration::seconds((hours * 3600.0) as i64)),
        None => fallback.to_string(),
    }
}

/// Converts optional ratio to a percentage string, or `fallback` if None.
pub fn format_optional_percent(f: Option<f64>, fallback: &str) -> String {
    match f {
        Some(val) => format!("{:.1}%", val * 100.0),
        None => fallback.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_days() {
        let d = chrono::Duration::hours(50);
        assert_eq!(format_duration(d), "2d 2h");
    }

    #[test]
    fn duration_hours_minutes() {
        let d = chrono::Duration::minutes(95);
        assert_eq!(format_duration(d), "1h 35m");
    }

    #[test]
    fn duration_zero() {
        let d = chrono::Duration::zero();
        assert_eq!(format_duration(d), "0h 0m");
    }

    #[test]
    fn optional_hours_some() {
        assert_eq!(format_optional_hours(Some(48.0), "--"), "2d 0h");
    }

    #[test]
    fn optional_hours_none() {
        assert_eq!(format_optional_hours(None, "--"), "--");
    }

    #[test]
    fn optional_percent_some() {
        assert_eq!(format_optional_percent(Some(0.65), "--"), "65.0%");
    }

    #[test]
    fn optional_percent_none() {
        assert_eq!(format_optional_percent(None, "--"), "--");
    }
}
