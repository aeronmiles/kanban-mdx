//! `kanban-md import` — bulk-create tasks from a JSON or YAML spec.
//!
//! Creates multiple tasks at once from a structured input file.
//! Tasks can reference each other via local "ref" IDs, and dependencies
//! are wired automatically by mapping refs to created kanban IDs.
//!
//! Accepts JSON or YAML input from a file argument or stdin (use "-" for stdin).

use std::collections::HashMap;
use std::io::{self, Read, Write};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::task::{self, Task};
use crate::output::Format;

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

#[derive(clap::Args, Clone)]
pub struct ImportArgs {
    /// Path to the import spec file (JSON or YAML). Use "-" for stdin.
    pub file: Option<String>,
}

// ---------------------------------------------------------------------------
// Import spec types (deserialized from input)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ImportSpec {
    #[serde(default)]
    parent: Option<ImportParent>,
    tasks: Vec<ImportTask>,
}

#[derive(Debug, Deserialize)]
struct ImportParent {
    title: String,
    #[serde(default)]
    priority: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    body: String,
}

#[derive(Debug, Deserialize)]
struct ImportTask {
    #[serde(rename = "ref")]
    ref_id: String,
    title: String,
    #[serde(default)]
    priority: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    assignee: String,
}

// ---------------------------------------------------------------------------
// Output types (serialized for JSON output)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ImportOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<i32>,
    created: usize,
    mapping: Vec<ImportResult>,
}

#[derive(Debug, Serialize)]
struct ImportResult {
    #[serde(rename = "ref")]
    ref_id: String,
    id: i32,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn run(cli: &Cli, args: ImportArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);

    // 1. Read input data.
    let data = read_import_input(&args)?;

    // 2. Parse spec (try JSON first, then YAML).
    let spec = parse_import_spec(&data)?;

    // 3. Validate spec.
    validate_import_spec(&spec)?;

    // 4. Load config.
    let mut cfg = crate::cli::root::load_config(cli)?;
    let now = Utc::now();

    // 5. Create parent task if present.
    let mut parent_id: Option<i32> = None;
    if let Some(ref parent) = spec.parent {
        let pid = create_import_parent(&mut cfg, parent, now)?;
        parent_id = Some(pid);
    }

    // 6. Create child tasks.
    let mapping = create_import_tasks(&mut cfg, &spec.tasks, parent_id, now)?;

    // 7. Save updated config (bumped next_id).
    crate::io::config_file::save(&cfg)?;

    // 8. Output results.
    output_import_result(format, parent_id, &mapping)
}

// ---------------------------------------------------------------------------
// Input reading
// ---------------------------------------------------------------------------

fn read_import_input(args: &ImportArgs) -> Result<Vec<u8>, CliError> {
    match args.file.as_deref() {
        None | Some("-") => {
            let mut data = Vec::new();
            io::stdin().read_to_end(&mut data).map_err(|e| {
                CliError::newf(ErrorCode::InternalError, format!("reading stdin: {e}"))
            })?;
            if data.is_empty() {
                return Err(CliError::new(
                    ErrorCode::InvalidInput,
                    "no input provided on stdin",
                ));
            }
            Ok(data)
        }
        Some(path) => {
            std::fs::read(path).map_err(|e| {
                CliError::newf(
                    ErrorCode::InternalError,
                    format!("reading file {path}: {e}"),
                )
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing (JSON first, then YAML)
// ---------------------------------------------------------------------------

fn parse_import_spec(data: &[u8]) -> Result<ImportSpec, CliError> {
    // Try JSON first.
    if let Ok(text) = std::str::from_utf8(data) {
        if serde_json::from_str::<serde_json::Value>(text).is_ok() {
            return serde_json::from_str::<ImportSpec>(text).map_err(|e| {
                CliError::newf(ErrorCode::InvalidInput, format!("invalid JSON: {e}"))
            });
        }
    }

    // Fall back to YAML.
    let text = std::str::from_utf8(data).map_err(|e| {
        CliError::newf(ErrorCode::InvalidInput, format!("invalid UTF-8: {e}"))
    })?;
    serde_yml::from_str::<ImportSpec>(text).map_err(|e| {
        CliError::newf(ErrorCode::InvalidInput, format!("invalid YAML: {e}"))
    })
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn validate_import_spec(spec: &ImportSpec) -> Result<(), CliError> {
    if spec.tasks.is_empty() {
        return Err(CliError::new(
            ErrorCode::InvalidInput,
            "import spec must contain at least one task",
        ));
    }

    if let Some(ref parent) = spec.parent {
        if parent.title.trim().is_empty() {
            return Err(CliError::new(
                ErrorCode::InvalidInput,
                "parent task title is required",
            ));
        }
    }

    let mut refs: HashMap<&str, bool> = HashMap::new();
    for (i, t) in spec.tasks.iter().enumerate() {
        if t.ref_id.trim().is_empty() {
            return Err(CliError::newf(
                ErrorCode::InvalidInput,
                format!("task[{i}]: ref is required"),
            ));
        }
        if refs.contains_key(t.ref_id.as_str()) {
            return Err(CliError::newf(
                ErrorCode::InvalidInput,
                format!("duplicate ref {:?}", t.ref_id),
            ));
        }
        refs.insert(&t.ref_id, true);

        if t.title.trim().is_empty() {
            return Err(CliError::newf(
                ErrorCode::InvalidInput,
                format!("task {:?}: title is required", t.ref_id),
            ));
        }

        for dep in &t.depends_on {
            if dep == &t.ref_id {
                return Err(CliError::newf(
                    ErrorCode::InvalidInput,
                    format!("task {:?}: self-referencing dependency", t.ref_id),
                ));
            }
        }
    }

    // Check that all depends_on refs exist in the spec.
    for t in &spec.tasks {
        for dep in &t.depends_on {
            if !refs.contains_key(dep.as_str()) {
                return Err(CliError::newf(
                    ErrorCode::InvalidInput,
                    format!(
                        "task {:?}: depends_on ref {:?} does not match any task in the spec",
                        t.ref_id, dep
                    ),
                ));
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Parent task creation
// ---------------------------------------------------------------------------

fn create_import_parent(
    cfg: &mut crate::model::config::Config,
    parent: &ImportParent,
    now: chrono::DateTime<Utc>,
) -> Result<i32, CliError> {
    let status = if parent.status.is_empty() {
        cfg.defaults.status.clone()
    } else {
        task::validate_status(&parent.status, &cfg.status_names())
            .map_err(|e| CliError::newf(ErrorCode::InvalidStatus, format!("parent: {e}")))?;
        parent.status.clone()
    };

    let priority = if parent.priority.is_empty() {
        cfg.defaults.priority.clone()
    } else {
        task::validate_priority(&parent.priority, &cfg.priorities)
            .map_err(|e| CliError::newf(ErrorCode::InvalidPriority, format!("parent: {e}")))?;
        parent.priority.clone()
    };

    let id = cfg.next_id;
    let mut t = Task {
        id,
        title: parent.title.clone(),
        status,
        priority,
        class: cfg.defaults.class.clone(),
        created: now,
        updated: now,
        ..Default::default()
    };

    if !parent.tags.is_empty() {
        t.tags = parent.tags.clone();
    }
    if !parent.body.is_empty() {
        t.body = parent.body.clone();
    }

    let slug = task::generate_slug(&t.title);
    let filename = task::generate_filename(id, &slug);
    let file_path = cfg.tasks_path().join(&filename);

    crate::io::task_file::write(&file_path, &t)?;
    crate::board::log::log_mutation(cfg.dir(), "create", id, &t.title);
    cfg.next_id += 1;

    Ok(id)
}

// ---------------------------------------------------------------------------
// Child task creation
// ---------------------------------------------------------------------------

fn create_import_tasks(
    cfg: &mut crate::model::config::Config,
    tasks: &[ImportTask],
    parent_id: Option<i32>,
    now: chrono::DateTime<Utc>,
) -> Result<Vec<ImportResult>, CliError> {
    let mut ref_map: HashMap<String, i32> = HashMap::new();
    let mut mapping: Vec<ImportResult> = Vec::with_capacity(tasks.len());

    for st in tasks {
        let status = if st.status.is_empty() {
            cfg.defaults.status.clone()
        } else {
            task::validate_status(&st.status, &cfg.status_names()).map_err(|e| {
                CliError::newf(
                    ErrorCode::InvalidStatus,
                    format!("task {:?}: {e}", st.ref_id),
                )
            })?;
            st.status.clone()
        };

        let priority = if st.priority.is_empty() {
            cfg.defaults.priority.clone()
        } else {
            task::validate_priority(&st.priority, &cfg.priorities).map_err(|e| {
                CliError::newf(
                    ErrorCode::InvalidPriority,
                    format!("task {:?}: {e}", st.ref_id),
                )
            })?;
            st.priority.clone()
        };

        let id = cfg.next_id;
        let mut t = Task {
            id,
            title: st.title.clone(),
            status,
            priority,
            class: cfg.defaults.class.clone(),
            created: now,
            updated: now,
            parent: parent_id,
            ..Default::default()
        };

        if !st.tags.is_empty() {
            t.tags = st.tags.clone();
        }
        if !st.body.is_empty() {
            t.body = st.body.clone();
        }
        if !st.assignee.is_empty() {
            t.assignee = st.assignee.clone();
        }

        // Resolve depends_on refs to kanban IDs.
        for dep_ref in &st.depends_on {
            match ref_map.get(dep_ref) {
                Some(&dep_id) => {
                    t.depends_on.push(dep_id);
                }
                None => {
                    return Err(CliError::newf(
                        ErrorCode::InvalidInput,
                        format!(
                            "task {:?} depends on unknown ref {:?} (refs must be declared before use)",
                            st.ref_id, dep_ref
                        ),
                    ));
                }
            }
        }

        let slug = task::generate_slug(&t.title);
        let filename = task::generate_filename(id, &slug);
        let file_path = cfg.tasks_path().join(&filename);

        crate::io::task_file::write(&file_path, &t)?;
        crate::board::log::log_mutation(cfg.dir(), "create", id, &t.title);

        ref_map.insert(st.ref_id.clone(), id);
        mapping.push(ImportResult {
            ref_id: st.ref_id.clone(),
            id,
        });
        cfg.next_id += 1;
    }

    Ok(mapping)
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

fn output_import_result(
    format: Format,
    parent_id: Option<i32>,
    mapping: &[ImportResult],
) -> Result<(), CliError> {
    let out = ImportOutput {
        parent: parent_id,
        created: mapping.len(),
        mapping: mapping.to_vec(),
    };

    let mut stdout = io::stdout();
    match format {
        Format::Json => {
            crate::output::json::json(&mut stdout, &out)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        Format::Compact | Format::Table => {
            if let Some(pid) = parent_id {
                writeln!(stdout, "Created {} tasks under parent #{}", mapping.len(), pid)
                    .unwrap_or(());
            } else {
                writeln!(stdout, "Created {} task{}", mapping.len(), if mapping.len() == 1 { "" } else { "s" })
                    .unwrap_or(());
            }
            for m in mapping {
                writeln!(stdout, "  {} -> #{}", m.ref_id, m.id).unwrap_or(());
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Allow ImportResult to be cloned for the Vec in ImportOutput
// ---------------------------------------------------------------------------

impl Clone for ImportResult {
    fn clone(&self) -> Self {
        ImportResult {
            ref_id: self.ref_id.clone(),
            id: self.id,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_spec() {
        let data = br#"{
            "tasks": [
                {"ref": "T0", "title": "First task"},
                {"ref": "T1", "title": "Second task", "depends_on": ["T0"]}
            ]
        }"#;
        let spec = parse_import_spec(data).unwrap();
        assert!(spec.parent.is_none());
        assert_eq!(spec.tasks.len(), 2);
        assert_eq!(spec.tasks[0].ref_id, "T0");
        assert_eq!(spec.tasks[1].depends_on, vec!["T0"]);
    }

    #[test]
    fn test_parse_yaml_spec() {
        let data = b"tasks:\n  - ref: Y0\n    title: YAML task one\n  - ref: Y1\n    title: YAML task two\n    depends_on:\n      - Y0\n";
        let spec = parse_import_spec(data).unwrap();
        assert_eq!(spec.tasks.len(), 2);
        assert_eq!(spec.tasks[0].ref_id, "Y0");
    }

    #[test]
    fn test_parse_json_with_parent() {
        let data = br#"{
            "parent": {"title": "Epic task", "priority": "high", "tags": ["epic"]},
            "tasks": [
                {"ref": "A", "title": "Subtask A"}
            ]
        }"#;
        let spec = parse_import_spec(data).unwrap();
        let parent = spec.parent.unwrap();
        assert_eq!(parent.title, "Epic task");
        assert_eq!(parent.priority, "high");
        assert_eq!(parent.tags, vec!["epic"]);
    }

    #[test]
    fn test_validate_empty_tasks() {
        let spec = ImportSpec {
            parent: None,
            tasks: vec![],
        };
        let err = validate_import_spec(&spec).unwrap_err();
        assert!(err.message.contains("at least one task"));
    }

    #[test]
    fn test_validate_missing_ref() {
        let spec = ImportSpec {
            parent: None,
            tasks: vec![ImportTask {
                ref_id: String::new(),
                title: "A task".into(),
                priority: String::new(),
                tags: vec![],
                body: String::new(),
                depends_on: vec![],
                status: String::new(),
                assignee: String::new(),
            }],
        };
        let err = validate_import_spec(&spec).unwrap_err();
        assert!(err.message.contains("ref is required"));
    }

    #[test]
    fn test_validate_duplicate_ref() {
        let spec = ImportSpec {
            parent: None,
            tasks: vec![
                ImportTask {
                    ref_id: "X".into(),
                    title: "Task one".into(),
                    priority: String::new(),
                    tags: vec![],
                    body: String::new(),
                    depends_on: vec![],
                    status: String::new(),
                    assignee: String::new(),
                },
                ImportTask {
                    ref_id: "X".into(),
                    title: "Task two".into(),
                    priority: String::new(),
                    tags: vec![],
                    body: String::new(),
                    depends_on: vec![],
                    status: String::new(),
                    assignee: String::new(),
                },
            ],
        };
        let err = validate_import_spec(&spec).unwrap_err();
        assert!(err.message.contains("duplicate"));
    }

    #[test]
    fn test_validate_missing_title() {
        let spec = ImportSpec {
            parent: None,
            tasks: vec![ImportTask {
                ref_id: "A".into(),
                title: String::new(),
                priority: String::new(),
                tags: vec![],
                body: String::new(),
                depends_on: vec![],
                status: String::new(),
                assignee: String::new(),
            }],
        };
        let err = validate_import_spec(&spec).unwrap_err();
        assert!(err.message.contains("title is required"));
    }

    #[test]
    fn test_validate_self_reference() {
        let spec = ImportSpec {
            parent: None,
            tasks: vec![ImportTask {
                ref_id: "A".into(),
                title: "Task A".into(),
                priority: String::new(),
                tags: vec![],
                body: String::new(),
                depends_on: vec!["A".into()],
                status: String::new(),
                assignee: String::new(),
            }],
        };
        let err = validate_import_spec(&spec).unwrap_err();
        assert!(err.message.contains("self-referencing"));
    }

    #[test]
    fn test_validate_unknown_dep_ref() {
        let spec = ImportSpec {
            parent: None,
            tasks: vec![ImportTask {
                ref_id: "A".into(),
                title: "Task A".into(),
                priority: String::new(),
                tags: vec![],
                body: String::new(),
                depends_on: vec!["Z".into()],
                status: String::new(),
                assignee: String::new(),
            }],
        };
        let err = validate_import_spec(&spec).unwrap_err();
        assert!(err.message.contains("does not match any task"));
    }

    #[test]
    fn test_validate_parent_empty_title() {
        let spec = ImportSpec {
            parent: Some(ImportParent {
                title: "   ".into(),
                priority: String::new(),
                tags: vec![],
                status: String::new(),
                body: String::new(),
            }),
            tasks: vec![ImportTask {
                ref_id: "A".into(),
                title: "Task A".into(),
                priority: String::new(),
                tags: vec![],
                body: String::new(),
                depends_on: vec![],
                status: String::new(),
                assignee: String::new(),
            }],
        };
        let err = validate_import_spec(&spec).unwrap_err();
        assert!(err.message.contains("parent task title"));
    }

    #[test]
    fn test_validate_valid_spec() {
        let spec = ImportSpec {
            parent: Some(ImportParent {
                title: "Epic".into(),
                priority: String::new(),
                tags: vec![],
                status: String::new(),
                body: String::new(),
            }),
            tasks: vec![
                ImportTask {
                    ref_id: "A".into(),
                    title: "Task A".into(),
                    priority: String::new(),
                    tags: vec![],
                    body: String::new(),
                    depends_on: vec![],
                    status: String::new(),
                    assignee: String::new(),
                },
                ImportTask {
                    ref_id: "B".into(),
                    title: "Task B".into(),
                    priority: String::new(),
                    tags: vec![],
                    body: String::new(),
                    depends_on: vec!["A".into()],
                    status: String::new(),
                    assignee: String::new(),
                },
            ],
        };
        assert!(validate_import_spec(&spec).is_ok());
    }

    #[test]
    fn test_parse_full_task_fields() {
        let data = br#"{
            "tasks": [
                {
                    "ref": "F0",
                    "title": "Full field task",
                    "priority": "high",
                    "tags": ["backend", "api"],
                    "body": "Detailed description",
                    "status": "todo",
                    "assignee": "alice"
                }
            ]
        }"#;
        let spec = parse_import_spec(data).unwrap();
        let t = &spec.tasks[0];
        assert_eq!(t.ref_id, "F0");
        assert_eq!(t.title, "Full field task");
        assert_eq!(t.priority, "high");
        assert_eq!(t.tags, vec!["backend", "api"]);
        assert_eq!(t.body, "Detailed description");
        assert_eq!(t.status, "todo");
        assert_eq!(t.assignee, "alice");
    }

    #[test]
    fn test_import_output_serialization() {
        let out = ImportOutput {
            parent: Some(1),
            created: 2,
            mapping: vec![
                ImportResult { ref_id: "A".into(), id: 2 },
                ImportResult { ref_id: "B".into(), id: 3 },
            ],
        };
        let json_str = serde_json::to_string(&out).unwrap();
        assert!(json_str.contains("\"parent\":1"));
        assert!(json_str.contains("\"created\":2"));
        assert!(json_str.contains("\"ref\":\"A\""));
        assert!(json_str.contains("\"id\":2"));
    }

    #[test]
    fn test_import_output_no_parent() {
        let out = ImportOutput {
            parent: None,
            created: 1,
            mapping: vec![ImportResult { ref_id: "X".into(), id: 1 }],
        };
        let json_str = serde_json::to_string(&out).unwrap();
        assert!(!json_str.contains("\"parent\""));
    }
}
