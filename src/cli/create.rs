//! `kanban-md create` — create a new task.

use std::io::Write;

use chrono::Utc;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::model::task::{self, Task};
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct CreateArgs {
    /// Task title
    pub title: String,
    /// Status (default from config)
    #[arg(long, short = 's')]
    pub status: Option<String>,
    /// Priority (default from config)
    #[arg(long, short = 'p')]
    pub priority: Option<String>,
    /// Assignee
    #[arg(long, short = 'a')]
    pub assignee: Option<String>,
    /// Tags (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub tags: Vec<String>,
    /// Due date (YYYY-MM-DD)
    #[arg(long)]
    pub due: Option<String>,
    /// Time estimate
    #[arg(long)]
    pub estimate: Option<String>,
    /// Parent task ID
    #[arg(long)]
    pub parent: Option<i32>,
    /// Depends on task IDs (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub depends_on: Vec<i32>,
    /// Body text
    #[arg(long, short = 'b')]
    pub body: Option<String>,
    /// Class of service
    #[arg(long)]
    pub class: Option<String>,
    /// Claim for agent
    #[arg(long, env = "KANBAN_AGENT")]
    pub claim: Option<String>,
}

pub fn run(cli: &Cli, args: CreateArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let mut cfg = crate::cli::root::load_config(cli)?;
    let now = Utc::now();

    let status = args
        .status
        .unwrap_or_else(|| cfg.defaults.status.clone());
    let priority = args
        .priority
        .unwrap_or_else(|| cfg.defaults.priority.clone());

    // Validate status and priority.
    task::validate_status(&status, &cfg.status_names())
        .map_err(|e| CliError::newf(ErrorCode::InvalidStatus, format!("{e}")))?;
    task::validate_priority(&priority, &cfg.priorities)
        .map_err(|e| CliError::newf(ErrorCode::InvalidPriority, format!("{e}")))?;

    // Parse due date.
    let due = if let Some(ref d) = args.due {
        let date = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| {
            CliError::newf(ErrorCode::InvalidDate, format!("invalid date: {d}"))
        })?;
        Some(date)
    } else {
        None
    };

    // Validate class if provided.
    let class = args.class.unwrap_or_default();
    if !class.is_empty() {
        task::validate_class(&class, &cfg.class_names())
            .map_err(|e| CliError::newf(ErrorCode::InvalidClass, format!("{e}")))?;
    }

    // WIP limit enforcement (class-level board-wide + column-level).
    // For new tasks, exclude_id=0 since the task doesn't exist yet.
    crate::cli::wip::enforce_wip_limits(&cfg, &class, &status, 0)?;

    // Allocate ID.
    let id = cfg.next_id;
    cfg.next_id += 1;

    let mut t = Task {
        id,
        title: args.title,
        status,
        priority,
        created: now,
        updated: now,
        assignee: args.assignee.unwrap_or_default(),
        tags: args.tags,
        due,
        estimate: args.estimate.unwrap_or_default(),
        parent: args.parent,
        depends_on: args.depends_on,
        class,
        body: args.body.unwrap_or_default(),
        ..Default::default()
    };

    // Apply claim if requested.
    if let Some(ref claim) = args.claim {
        t.claimed_by = claim.clone();
        t.claimed_at = Some(now);
    }

    // Generate filename and write.
    let slug = task::generate_slug(&t.title);
    let filename = task::generate_filename(id, &slug);
    let file_path = cfg.tasks_path().join(&filename);

    crate::io::task_file::write(&file_path, &t)?;

    // Save updated config (bumped next_id).
    crate::io::config_file::save(&cfg)?;

    // Log the mutation.
    crate::board::log::log_mutation(cfg.dir(), "create", id, &t.title);

    t.file = file_path.to_string_lossy().to_string();

    let mut stdout = std::io::stdout();
    match format {
        Format::Json => {
            crate::output::json::json(&mut stdout, &t)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
        }
        Format::Compact => {
            writeln!(stdout, "#{id} created [{}/{}] {}", t.status, t.priority, t.title)
                .unwrap_or(());
        }
        Format::Table => {
            writeln!(stdout, "Created task #{id}: {}", t.title).unwrap_or(());
        }
    }
    Ok(())
}
