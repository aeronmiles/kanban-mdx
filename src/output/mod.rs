pub mod compact;
pub mod json;
pub mod table;
pub mod types;

use std::env;

/// Output format for CLI commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Table,
    Json,
    Compact,
}

/// Detect the output format from flags and environment variables.
/// Default is Table when no explicit format is set.
pub fn detect(json_flag: bool, table_flag: bool, compact_flag: bool) -> Format {
    if json_flag {
        return Format::Json;
    }
    if compact_flag {
        return Format::Compact;
    }
    if table_flag {
        return Format::Table;
    }

    match env::var("KANBAN_OUTPUT").as_deref() {
        Ok("json") => Format::Json,
        Ok("compact" | "oneline") => Format::Compact,
        Ok("table") => Format::Table,
        _ => Format::Table,
    }
}
