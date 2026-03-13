//! `kanban-md find` — semantic search for tasks.
//!
//! Uses the embedding-based semantic search index to find relevant task
//! sections by meaning rather than exact substring match. Requires semantic
//! search to be enabled in config.toml and embeddings to be synced.

use std::collections::HashMap;
use std::io::Write;

use colored::Colorize;
use comfy_table::{Cell, ContentArrangement, Table};

use crate::cli::root::Cli;
use crate::embed;
use crate::error::{CliError, ErrorCode};
use crate::model::task;
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct FindArgs {
    /// Search query
    pub query: String,
    /// Maximum results
    #[arg(long, short = 'n', default_value = "10")]
    pub limit: usize,
}

pub fn run(cli: &Cli, args: FindArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;

    // Check if semantic search is enabled.
    if !cfg.semantic_search.enabled {
        return Err(CliError::new(
            ErrorCode::InvalidInput,
            "semantic search not configured; add semantic_search section to config.toml \
             and run 'kanban-md embed sync'",
        ));
    }

    // Create the embedding manager.
    let mut mgr = embed::Manager::new(&cfg).map_err(|e| {
        CliError::newf(
            ErrorCode::InternalError,
            format!("initializing embedding manager: {e}"),
        )
    })?;

    // Read all tasks.
    let (all_tasks, _) = task::read_all_lenient(&cfg.tasks_path()).map_err(|e| {
        CliError::newf(ErrorCode::InternalError, format!("{e}"))
    })?;

    // Sync embeddings to ensure the index is up to date.
    mgr.sync(&all_tasks).map_err(|e| {
        CliError::newf(
            ErrorCode::InternalError,
            format!("syncing embeddings: {e}"),
        )
    })?;

    // Perform section-level semantic search.
    let results = mgr.find(&args.query, args.limit).map_err(|e| {
        CliError::newf(
            ErrorCode::InternalError,
            format!("semantic search failed: {e}"),
        )
    })?;

    // Build task title lookup map.
    let title_map: HashMap<i32, String> = all_tasks
        .iter()
        .map(|t| (t.id, t.title.clone()))
        .collect();

    let mut stdout = std::io::stdout();

    match format {
        Format::Json => {
            render_json(&mut stdout, &results, &title_map)?;
        }
        Format::Compact => {
            render_compact(&mut stdout, &results, &title_map);
        }
        Format::Table => {
            render_table(&mut stdout, &results, &title_map);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Output renderers
// ---------------------------------------------------------------------------

fn render_json(
    w: &mut impl Write,
    results: &[embed::FindResult],
    title_map: &HashMap<i32, String>,
) -> Result<(), CliError> {
    let json_results: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            let title = title_map
                .get(&r.task_id)
                .cloned()
                .unwrap_or_default();
            serde_json::json!({
                "task_id": r.task_id,
                "chunk": r.chunk,
                "header": r.header,
                "line": r.line,
                "score": (r.score * 1000.0).round() / 1000.0,
                "title": title,
            })
        })
        .collect();

    crate::output::json::json(w, &json_results)
        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
    Ok(())
}

fn render_compact(
    w: &mut impl Write,
    results: &[embed::FindResult],
    title_map: &HashMap<i32, String>,
) {
    if results.is_empty() {
        let _ = writeln!(w, "No results found.");
        return;
    }

    for r in results {
        let title = title_map
            .get(&r.task_id)
            .cloned()
            .unwrap_or_default();
        // Format: #42 0.85 "## Implementation" (line 15) — Task Title
        let _ = writeln!(
            w,
            "#{} {:.2} {:?} (line {}) \u{2014} {}",
            r.task_id, r.score, r.header, r.line, title
        );
    }
}

fn render_table(
    w: &mut impl Write,
    results: &[embed::FindResult],
    title_map: &HashMap<i32, String>,
) {
    if results.is_empty() {
        let _ = writeln!(w, "No results found.");
        return;
    }

    let mut table = Table::new();
    table
        .set_content_arrangement(ContentArrangement::Dynamic)
        .load_preset(comfy_table::presets::NOTHING)
        .set_header(vec![
            Cell::new("ID"),
            Cell::new("SCORE"),
            Cell::new("HEADER"),
            Cell::new("LINE"),
            Cell::new("TITLE"),
        ]);

    for r in results {
        let title = title_map
            .get(&r.task_id)
            .cloned()
            .unwrap_or_default();
        let title_display = if title.len() > 40 {
            format!("{}...", &title[..37])
        } else {
            title
        };
        let header_display = if r.header.len() > 30 {
            format!("{}...", &r.header[..27])
        } else {
            r.header.clone()
        };

        table.add_row(vec![
            Cell::new(format!("#{}", r.task_id)),
            Cell::new(format!("{:.2}", r.score).yellow()),
            Cell::new(header_display),
            Cell::new(r.line),
            Cell::new(title_display),
        ]);
    }

    let _ = writeln!(w, "{table}");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_results() -> Vec<embed::FindResult> {
        vec![
            embed::FindResult {
                task_id: 1,
                chunk: 0,
                header: "## Architecture".to_string(),
                line: 5,
                score: 0.95,
            },
            embed::FindResult {
                task_id: 2,
                chunk: 1,
                header: "## Testing".to_string(),
                line: 12,
                score: 0.82,
            },
        ]
    }

    fn make_title_map() -> HashMap<i32, String> {
        let mut m = HashMap::new();
        m.insert(1, "Design new API layer".to_string());
        m.insert(2, "Implement integration tests".to_string());
        m
    }

    // -- render_compact tests --

    #[test]
    fn test_render_compact_empty() {
        let mut buf = Vec::new();
        let results: Vec<embed::FindResult> = vec![];
        let title_map = HashMap::new();

        render_compact(&mut buf, &results, &title_map);

        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("No results found."));
    }

    #[test]
    fn test_render_compact_with_results() {
        let mut buf = Vec::new();
        let results = make_results();
        let title_map = make_title_map();

        render_compact(&mut buf, &results, &title_map);

        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("#1"), "should contain task ID #1");
        assert!(output.contains("#2"), "should contain task ID #2");
        assert!(output.contains("0.95"), "should contain score 0.95");
        assert!(output.contains("0.82"), "should contain score 0.82");
        assert!(
            output.contains("Architecture"),
            "should contain header text"
        );
        assert!(
            output.contains("Design new API layer"),
            "should contain task title"
        );
        assert!(output.contains("(line 5)"), "should contain line number");
    }

    #[test]
    fn test_render_compact_missing_title() {
        let mut buf = Vec::new();
        let results = vec![embed::FindResult {
            task_id: 99,
            chunk: 0,
            header: "## Unknown".to_string(),
            line: 1,
            score: 0.5,
        }];
        let title_map = HashMap::new(); // no entry for task 99

        render_compact(&mut buf, &results, &title_map);

        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("#99"), "should contain task ID");
        // Title defaults to empty string, line should still render
        assert!(output.contains("0.50"), "should contain formatted score");
    }

    // -- render_json tests --

    #[test]
    fn test_render_json_empty() {
        let mut buf = Vec::new();
        let results: Vec<embed::FindResult> = vec![];
        let title_map = HashMap::new();

        render_json(&mut buf, &results, &title_map).unwrap();

        let output = String::from_utf8(buf).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed.as_array().unwrap().is_empty());
    }

    #[test]
    fn test_render_json_with_results() {
        let mut buf = Vec::new();
        let results = make_results();
        let title_map = make_title_map();

        render_json(&mut buf, &results, &title_map).unwrap();

        let output = String::from_utf8(buf).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 2);

        assert_eq!(arr[0]["task_id"], 1);
        assert_eq!(arr[0]["header"], "## Architecture");
        assert_eq!(arr[0]["line"], 5);
        // Score is rounded to 3 decimal places in render_json, but f32->f64
        // conversion means exact equality won't work. Check within tolerance.
        let score0 = arr[0]["score"].as_f64().unwrap();
        assert!((score0 - 0.95).abs() < 0.01, "score should be ~0.95, got {}", score0);
        assert_eq!(arr[0]["title"], "Design new API layer");

        assert_eq!(arr[1]["task_id"], 2);
        let score1 = arr[1]["score"].as_f64().unwrap();
        assert!((score1 - 0.82).abs() < 0.01, "score should be ~0.82, got {}", score1);
    }

    #[test]
    fn test_render_json_score_rounding() {
        let mut buf = Vec::new();
        let results = vec![embed::FindResult {
            task_id: 1,
            chunk: 0,
            header: "## Test".to_string(),
            line: 1,
            score: 0.8567, // should round to 0.857
        }];
        let title_map = make_title_map();

        render_json(&mut buf, &results, &title_map).unwrap();

        let output = String::from_utf8(buf).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        let score = parsed[0]["score"].as_f64().unwrap();
        assert!((score - 0.857).abs() < 0.001, "score should be rounded to 3 decimal places, got {}", score);
    }

    // -- render_table tests --

    #[test]
    fn test_render_table_empty() {
        let mut buf = Vec::new();
        let results: Vec<embed::FindResult> = vec![];
        let title_map = HashMap::new();

        render_table(&mut buf, &results, &title_map);

        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("No results found."));
    }

    #[test]
    fn test_render_table_with_results() {
        let mut buf = Vec::new();
        let results = make_results();
        let title_map = make_title_map();

        render_table(&mut buf, &results, &title_map);

        let output = String::from_utf8(buf).unwrap();
        // Table should contain headers
        assert!(output.contains("ID"), "should contain ID column header");
        assert!(output.contains("SCORE"), "should contain SCORE column header");
        assert!(output.contains("HEADER"), "should contain HEADER column header");
        assert!(output.contains("TITLE"), "should contain TITLE column header");
        // And data
        assert!(output.contains("#1"), "should contain task #1");
        assert!(output.contains("#2"), "should contain task #2");
    }

    #[test]
    fn test_render_table_truncates_long_title() {
        let mut buf = Vec::new();
        let long_title = "This is a very long task title that exceeds forty characters by a lot";
        let results = vec![embed::FindResult {
            task_id: 1,
            chunk: 0,
            header: "## Test".to_string(),
            line: 1,
            score: 0.9,
        }];
        let mut title_map = HashMap::new();
        title_map.insert(1, long_title.to_string());

        render_table(&mut buf, &results, &title_map);

        let output = String::from_utf8(buf).unwrap();
        // The full long title should NOT appear (it gets truncated to 37 chars + "...")
        assert!(!output.contains(long_title), "long title should be truncated");
        assert!(output.contains("..."), "truncated title should end with ...");
    }

    #[test]
    fn test_render_table_truncates_long_header() {
        let mut buf = Vec::new();
        let long_header =
            "## This is a header that is much longer than thirty characters";
        let results = vec![embed::FindResult {
            task_id: 1,
            chunk: 0,
            header: long_header.to_string(),
            line: 1,
            score: 0.9,
        }];
        let title_map = make_title_map();

        render_table(&mut buf, &results, &title_map);

        let output = String::from_utf8(buf).unwrap();
        // The full long header should NOT appear
        assert!(
            !output.contains(long_header),
            "long header should be truncated"
        );
    }
}
