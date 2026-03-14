//! `kbmdx show` — show task details.

use std::collections::BTreeMap;
use std::io::Write;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::task;
use crate::output::Format;

/// Valid field names for `--fields` selection.
const VALID_FIELDS: &[&str] = &[
    "id",
    "title",
    "status",
    "priority",
    "created",
    "updated",
    "started",
    "completed",
    "assignee",
    "tags",
    "due",
    "estimate",
    "parent",
    "depends_on",
    "blocked",
    "block_reason",
    "claimed_by",
    "class",
    "branch",
    "worktree",
    "body",
];

#[derive(clap::Args, Clone)]
pub struct ShowArgs {
    /// Task ID
    pub id: String,
    /// Suppress body in output
    #[arg(long)]
    pub no_body: bool,
    /// Show a specific section from the body
    #[arg(long)]
    pub section: Option<String>,
    /// Token-efficient output for LLM prompts (minimal key=value format)
    #[arg(long)]
    pub prompt: bool,
    /// Comma-separated field selection for --prompt mode (e.g. --fields id,title,status,body)
    #[arg(long, value_delimiter = ',')]
    pub fields: Option<Vec<String>>,
    /// Show children status summary
    #[arg(long)]
    pub children: bool,
}

/// Children status summary for JSON output.
#[derive(serde::Serialize)]
struct ChildrenSummary {
    total: usize,
    #[serde(flatten)]
    by_status: BTreeMap<String, usize>,
}

pub fn run(cli: &Cli, args: ShowArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;

    let id = super::helpers::parse_task_id(&args.id)?;

    let (_file_path, mut t) = super::helpers::load_task(&cfg, id)?;

    if args.no_body {
        t.body.clear();
    }

    // Extract a specific section if requested.
    if let Some(ref section_name) = args.section {
        if let Some(section_body) = task::get_section(&t.body, section_name) {
            t.body = section_body;
        } else {
            t.body.clear();
        }
    }

    // Validate --fields values if provided.
    if let Some(ref fields) = args.fields {
        for f in fields {
            if !VALID_FIELDS.contains(&f.as_str()) {
                return Err(CliError::newf(
                    ErrorCode::InvalidInput,
                    format!(
                        "invalid field: {f}. Valid fields: {}",
                        VALID_FIELDS.join(", ")
                    ),
                ));
            }
        }
    }

    // Load children if requested.
    let children_by_status: Option<BTreeMap<String, usize>> = if args.children {
        let (all_tasks, _warnings) = task::read_all_lenient(&cfg.tasks_path())
            .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for child in &all_tasks {
            if child.parent == Some(id) {
                *counts.entry(child.status.clone()).or_insert(0) += 1;
            }
        }
        Some(counts)
    } else {
        None
    };

    let mut stdout = std::io::stdout();

    // --prompt mode: token-efficient key=value output.
    if args.prompt {
        write_prompt(&mut stdout, &t, args.fields.as_deref(), children_by_status.as_ref());
        return Ok(());
    }

    match format {
        Format::Json => {
            if let Some(ref children) = children_by_status {
                // Build a combined JSON value with children_summary.
                let mut value = serde_json::to_value(&t)
                    .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
                let total: usize = children.values().sum();
                let summary = ChildrenSummary {
                    total,
                    by_status: children.clone(),
                };
                if let Some(obj) = value.as_object_mut() {
                    obj.insert(
                        "children_summary".to_string(),
                        serde_json::to_value(&summary).unwrap_or_default(),
                    );
                }
                crate::output::json::json(&mut stdout, &value)
                    .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
            } else {
                crate::output::json::json(&mut stdout, &t)
                    .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
            }
        }
        Format::Compact => {
            crate::output::compact::task_detail_compact(&mut stdout, &t);
            if let Some(ref children) = children_by_status {
                write_children_compact(&mut stdout, children);
            }
        }
        Format::Table => {
            crate::output::table::task_detail(&mut stdout, &t);
            if let Some(ref children) = children_by_status {
                write_children_table(&mut stdout, children);
            }
        }
    }
    Ok(())
}

/// Writes token-efficient key=value prompt output.
fn write_prompt(
    w: &mut impl Write,
    t: &task::Task,
    fields: Option<&[String]>,
    children: Option<&BTreeMap<String, usize>>,
) {
    // If --fields is specified, only output those fields; otherwise output all non-empty fields.
    let selected: Option<Vec<&str>> = fields.map(|fs| fs.iter().map(|s| s.as_str()).collect());

    let should_output = |name: &str| -> bool {
        match &selected {
            Some(list) => list.contains(&name),
            None => true, // output all non-empty fields when no selection
        }
    };

    if should_output("id") {
        let _ = writeln!(w, "id: {}", t.id);
    }
    if should_output("title") && (selected.is_some() || !t.title.is_empty()) {
        let _ = writeln!(w, "title: {}", t.title);
    }
    if should_output("status") && (selected.is_some() || !t.status.is_empty()) {
        let _ = writeln!(w, "status: {}", t.status);
    }
    if should_output("priority") && (selected.is_some() || !t.priority.is_empty()) {
        let _ = writeln!(w, "priority: {}", t.priority);
    }
    if should_output("created") {
        let _ = writeln!(w, "created: {}", t.created.format("%Y-%m-%dT%H:%M:%SZ"));
    }
    if should_output("updated") {
        let _ = writeln!(w, "updated: {}", t.updated.format("%Y-%m-%dT%H:%M:%SZ"));
    }
    if should_output("started") {
        if let Some(ref started) = t.started {
            let _ = writeln!(w, "started: {}", started.format("%Y-%m-%dT%H:%M:%SZ"));
        } else if selected.is_some() {
            let _ = writeln!(w, "started:");
        }
    }
    if should_output("completed") {
        if let Some(ref completed) = t.completed {
            let _ = writeln!(w, "completed: {}", completed.format("%Y-%m-%dT%H:%M:%SZ"));
        } else if selected.is_some() {
            let _ = writeln!(w, "completed:");
        }
    }
    if should_output("assignee") && (selected.is_some() || !t.assignee.is_empty()) {
        let _ = writeln!(w, "assignee: {}", t.assignee);
    }
    if should_output("tags") && (selected.is_some() || !t.tags.is_empty()) {
        let _ = writeln!(w, "tags: {}", t.tags.join(","));
    }
    if should_output("due") {
        if let Some(ref due) = t.due {
            let _ = writeln!(w, "due: {due}");
        } else if selected.is_some() {
            let _ = writeln!(w, "due:");
        }
    }
    if should_output("estimate") && (selected.is_some() || !t.estimate.is_empty()) {
        let _ = writeln!(w, "estimate: {}", t.estimate);
    }
    if should_output("parent") {
        if let Some(parent) = t.parent {
            let _ = writeln!(w, "parent: {parent}");
        } else if selected.is_some() {
            let _ = writeln!(w, "parent:");
        }
    }
    if should_output("depends_on") && (selected.is_some() || !t.depends_on.is_empty()) {
        let deps: Vec<String> = t.depends_on.iter().map(|d| d.to_string()).collect();
        let _ = writeln!(w, "depends_on: {}", deps.join(","));
    }
    if should_output("blocked") && (selected.is_some() || t.blocked) {
        let _ = writeln!(w, "blocked: {}", t.blocked);
    }
    if should_output("block_reason") && (selected.is_some() || !t.block_reason.is_empty()) {
        let _ = writeln!(w, "block_reason: {}", t.block_reason);
    }
    if should_output("claimed_by") && (selected.is_some() || !t.claimed_by.is_empty()) {
        let _ = writeln!(w, "claimed_by: {}", t.claimed_by);
    }
    if should_output("class") && (selected.is_some() || !t.class.is_empty()) {
        let _ = writeln!(w, "class: {}", t.class);
    }
    if should_output("branch") && (selected.is_some() || !t.branch.is_empty()) {
        let _ = writeln!(w, "branch: {}", t.branch);
    }
    if should_output("worktree") && (selected.is_some() || !t.worktree.is_empty()) {
        let _ = writeln!(w, "worktree: {}", t.worktree);
    }
    if should_output("body") && (selected.is_some() || !t.body.is_empty()) {
        let _ = writeln!(w, "body:");
        for line in t.body.lines() {
            let _ = writeln!(w, "  {line}");
        }
    }

    // Append children summary if available.
    if let Some(counts) = children {
        let total: usize = counts.values().sum();
        if total > 0 {
            let parts: Vec<String> = counts
                .iter()
                .map(|(status, count)| format!("{status}={count}"))
                .collect();
            let _ = writeln!(w, "children: {total} ({})", parts.join(", "));
        } else {
            let _ = writeln!(w, "children: 0");
        }
    }
}

/// Writes children summary in compact format.
fn write_children_compact(w: &mut impl Write, counts: &BTreeMap<String, usize>) {
    let total: usize = counts.values().sum();
    if total == 0 {
        let _ = writeln!(w, "  children: 0");
        return;
    }
    let parts: Vec<String> = counts
        .iter()
        .map(|(status, count)| format!("{status}={count}"))
        .collect();
    let _ = writeln!(w, "  children: {total} ({})", parts.join(", "));
}

/// Writes children summary in table format.
#[cfg(test)]
fn write_prompt_to_buf(
    t: &task::Task,
    fields: Option<&[String]>,
    children: Option<&BTreeMap<String, usize>>,
) -> String {
    let mut buf = Vec::new();
    write_prompt(&mut buf, t, fields, children);
    String::from_utf8(buf).unwrap()
}

fn write_children_table(w: &mut impl Write, counts: &BTreeMap<String, usize>) {
    let total: usize = counts.values().sum();
    let _ = writeln!(w);
    let _ = writeln!(w, "Children ({total}):");
    if total == 0 {
        let _ = writeln!(w, "  No children found.");
        return;
    }
    for (status, count) in counts {
        let _ = writeln!(w, "  {status:<16} {count}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::task::Task;

    fn sample() -> Task {
        Task {
            id: 1,
            title: "Test".to_string(),
            status: "todo".to_string(),
            priority: "high".to_string(),
            tags: vec!["a".to_string(), "b".to_string()],
            body: "line1\nline2".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn prompt_outputs_all_fields() {
        let out = write_prompt_to_buf(&sample(), None, None);
        assert!(out.contains("id: 1"));
        assert!(out.contains("title: Test"));
        assert!(out.contains("status: todo"));
        assert!(out.contains("priority: high"));
        assert!(out.contains("tags: a,b"));
        assert!(out.contains("body:"));
        assert!(out.contains("  line1"));
    }

    #[test]
    fn prompt_field_selection() {
        let fields = vec!["id".to_string(), "title".to_string()];
        let out = write_prompt_to_buf(&sample(), Some(&fields), None);
        assert!(out.contains("id: 1"));
        assert!(out.contains("title: Test"));
        assert!(!out.contains("status:"));
        assert!(!out.contains("body:"));
    }

    #[test]
    fn prompt_children_summary() {
        let mut counts = BTreeMap::new();
        counts.insert("done".to_string(), 3);
        counts.insert("todo".to_string(), 2);
        let out = write_prompt_to_buf(&sample(), None, Some(&counts));
        assert!(out.contains("children: 5 (done=3, todo=2)"));
    }

    #[test]
    fn prompt_empty_fields_omitted() {
        let t = Task {
            id: 1,
            title: "X".to_string(),
            status: "todo".to_string(),
            priority: "low".to_string(),
            ..Default::default()
        };
        let out = write_prompt_to_buf(&t, None, None);
        assert!(!out.contains("assignee:"));
        assert!(!out.contains("tags:"));
        assert!(!out.contains("body:"));
    }

    #[test]
    fn children_compact_empty() {
        let mut buf = Vec::new();
        let counts = BTreeMap::new();
        write_children_compact(&mut buf, &counts);
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("children: 0"));
    }

    #[test]
    fn children_compact_with_counts() {
        let mut buf = Vec::new();
        let mut counts = BTreeMap::new();
        counts.insert("done".to_string(), 2);
        write_children_compact(&mut buf, &counts);
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("children: 2 (done=2)"));
    }

    #[test]
    fn children_table_empty() {
        let mut buf = Vec::new();
        let counts = BTreeMap::new();
        write_children_table(&mut buf, &counts);
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("No children found"));
    }

    #[test]
    fn valid_fields_list_complete() {
        assert!(VALID_FIELDS.contains(&"id"));
        assert!(VALID_FIELDS.contains(&"body"));
        assert!(VALID_FIELDS.contains(&"branch"));
        assert!(!VALID_FIELDS.contains(&"nonexistent"));
    }
}
