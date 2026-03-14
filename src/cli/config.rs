//! `kbmdx config` — get or set config values.

use std::io::Write;

use clap::Subcommand;

use crate::cli::root::Cli;
use crate::error::{CliError, ErrorCode};
use crate::output::Format;

#[derive(clap::Args, Clone)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: Option<ConfigCommands>,
}

#[derive(Subcommand, Clone)]
pub enum ConfigCommands {
    /// Get a config value
    Get {
        /// Config key (dot-separated path)
        key: String,
    },
    /// Set a config value
    Set {
        /// Config key (dot-separated path)
        key: String,
        /// New value
        value: String,
    },
}

pub fn run(cli: &Cli, args: ConfigArgs) -> Result<(), CliError> {
    let format = crate::cli::root::output_format(cli);
    let cfg = crate::cli::root::load_config(cli)?;

    match args.command {
        None => {
            // Print full config
            let mut stdout = std::io::stdout();
            match format {
                Format::Json => {
                    crate::output::json::json(&mut stdout, &cfg)
                        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
                }
                _ => {
                    let toml_str = toml::to_string_pretty(&cfg)
                        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
                    write!(stdout, "{toml_str}")
                        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
                }
            }
        }
        Some(ConfigCommands::Get { key }) => {
            let mut stdout = std::io::stdout();
            // Convert config to JSON for key lookup
            let json_val = serde_json::to_value(&cfg)
                .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
            let value = get_nested_value(&json_val, &key).ok_or_else(|| {
                CliError::newf(ErrorCode::InvalidInput, format!("config key not found: {key}"))
            })?;
            match format {
                Format::Json => {
                    crate::output::json::json(&mut stdout, &value)
                        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
                }
                _ => {
                    writeln!(stdout, "{}", format_value(&value))
                        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
                }
            }
        }
        Some(ConfigCommands::Set { key, value }) => {
            let mut cfg = crate::cli::root::load_config(cli)?;

            const VALID_KEYS: &[&str] = &[
                "board.name",
                "board.description",
                "defaults.status",
                "defaults.priority",
                "defaults.class",
                "claim_timeout",
                "tui.title_lines",
                "tui.hide_empty_columns",
                "tui.theme",
                "tui.reader_max_width",
                "tui.reader_width_pct",
            ];

            match key.as_str() {
                "board.name" => {
                    cfg.board.name = value.clone();
                }
                "board.description" => {
                    cfg.board.description = value.clone();
                }
                "defaults.status" => {
                    if !cfg.status_names().contains(&value) {
                        return Err(CliError::newf(
                            ErrorCode::InvalidInput,
                            format!(
                                "invalid status {:?}: must be one of {:?}",
                                value,
                                cfg.status_names()
                            ),
                        ));
                    }
                    cfg.defaults.status = value.clone();
                }
                "defaults.priority" => {
                    if !cfg.priorities.contains(&value) {
                        return Err(CliError::newf(
                            ErrorCode::InvalidInput,
                            format!(
                                "invalid priority {:?}: must be one of {:?}",
                                value, cfg.priorities
                            ),
                        ));
                    }
                    cfg.defaults.priority = value.clone();
                }
                "defaults.class" => {
                    if !cfg.classes.is_empty() && !cfg.class_names().contains(&value) {
                        return Err(CliError::newf(
                            ErrorCode::InvalidInput,
                            format!(
                                "invalid class {:?}: must be one of {:?}",
                                value,
                                cfg.class_names()
                            ),
                        ));
                    }
                    cfg.defaults.class = value.clone();
                }
                "claim_timeout" => {
                    if !value.is_empty()
                        && crate::model::config::parse_go_duration(&value).is_none()
                    {
                        return Err(CliError::newf(
                            ErrorCode::InvalidInput,
                            format!(
                                "invalid duration for claim_timeout: {:?} (expected format like \"1h\", \"30m\", \"1h30m\")",
                                value
                            ),
                        ));
                    }
                    cfg.claim_timeout = value.clone();
                }
                "tui.title_lines" => {
                    let n: i32 = value.parse().map_err(|_| {
                        CliError::newf(
                            ErrorCode::InvalidInput,
                            format!("invalid integer for tui.title_lines: {:?}", value),
                        )
                    })?;
                    if !(1..=3).contains(&n) {
                        return Err(CliError::newf(
                            ErrorCode::InvalidInput,
                            format!("tui.title_lines must be between 1 and 3, got {}", n),
                        ));
                    }
                    cfg.tui.title_lines = n;
                }
                "tui.hide_empty_columns" => {
                    let b: bool = value.parse().map_err(|_| {
                        CliError::newf(
                            ErrorCode::InvalidInput,
                            format!(
                                "invalid boolean for tui.hide_empty_columns: {:?} (use \"true\" or \"false\")",
                                value
                            ),
                        )
                    })?;
                    cfg.tui.hide_empty_columns = b;
                }
                "tui.theme" => {
                    cfg.tui.theme = value.clone();
                }
                "tui.reader_max_width" => {
                    let n: i32 = value.parse().map_err(|_| {
                        CliError::newf(
                            ErrorCode::InvalidInput,
                            format!("invalid integer for tui.reader_max_width: {:?}", value),
                        )
                    })?;
                    if n < 0 {
                        return Err(CliError::newf(
                            ErrorCode::InvalidInput,
                            format!("tui.reader_max_width must be >= 0, got {}", n),
                        ));
                    }
                    cfg.tui.reader_max_width = n;
                }
                "tui.reader_width_pct" => {
                    let n: i32 = value.parse().map_err(|_| {
                        CliError::newf(
                            ErrorCode::InvalidInput,
                            format!("invalid integer for tui.reader_width_pct: {:?}", value),
                        )
                    })?;
                    if !(10..=90).contains(&n) {
                        return Err(CliError::newf(
                            ErrorCode::InvalidInput,
                            format!("tui.reader_width_pct must be between 10 and 90, got {}", n),
                        ));
                    }
                    cfg.tui.reader_width_pct = n;
                }
                _ => {
                    return Err(CliError::newf(
                        ErrorCode::InvalidInput,
                        format!(
                            "unknown config key {:?}: valid keys are {:?}",
                            key, VALID_KEYS
                        ),
                    ));
                }
            }

            cfg.validate()
                .map_err(|e| CliError::new(ErrorCode::InvalidInput, e.to_string()))?;
            crate::io::config_file::save(&cfg)?;

            let mut stdout = std::io::stdout();
            match format {
                Format::Json => {
                    let obj = serde_json::json!({"key": key, "value": value});
                    crate::output::json::json(&mut stdout, &obj)
                        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
                }
                Format::Compact => {
                    writeln!(stdout, "{}={}", key, value)
                        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
                }
                Format::Table => {
                    writeln!(stdout, "Set {} to {}", key, value)
                        .map_err(|e| CliError::newf(ErrorCode::InternalError, format!("{e}")))?;
                }
            }
        }
    }
    Ok(())
}

fn get_nested_value<'a>(val: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut current = val;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

fn format_value(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => serde_json::to_string_pretty(val).unwrap_or_default(),
    }
}
