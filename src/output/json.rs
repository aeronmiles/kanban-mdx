//! JSON output formatting.

use serde::Serialize;
use std::collections::HashMap;
use std::io::{self, Write};

/// Writes `data` as pretty-printed JSON to the writer.
pub fn json<T: Serialize>(w: &mut impl Write, data: &T) -> io::Result<()> {
    let json_str = serde_json::to_string_pretty(data)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    writeln!(w, "{json_str}")
}

/// JSON envelope for structured error output.
#[derive(Debug, Serialize)]
struct ErrorResponse<'a> {
    error: &'a str,
    code: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<&'a HashMap<String, serde_json::Value>>,
}

/// Writes a structured error to the writer as pretty-printed JSON.
pub fn json_error(
    w: &mut impl Write,
    code: &str,
    msg: &str,
    details: Option<&HashMap<String, serde_json::Value>>,
) {
    let resp = ErrorResponse {
        error: msg,
        code,
        details,
    };
    // Best-effort: if the writer fails there is nothing we can do.
    let _ = serde_json::to_writer_pretty(&mut *w, &resp);
    let _ = writeln!(w);
}

/// Represents the outcome of a single operation within a batch.
#[derive(Debug, Clone, Serialize)]
pub struct BatchResult {
    pub id: i32,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_output_is_pretty() {
        let data = serde_json::json!({"hello": "world", "count": 42});
        let mut buf = Vec::new();
        json(&mut buf, &data).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains('\n'));
        assert!(output.contains("\"hello\": \"world\""));
    }

    #[test]
    fn json_error_without_details() {
        let mut buf = Vec::new();
        json_error(&mut buf, "TASK_NOT_FOUND", "task not found", None);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("\"error\": \"task not found\""));
        assert!(output.contains("\"code\": \"TASK_NOT_FOUND\""));
        // Should not contain "details" key when None.
        assert!(!output.contains("\"details\""));
    }

    #[test]
    fn json_error_with_details() {
        let mut details = HashMap::new();
        details.insert("id".to_string(), serde_json::json!(42));
        let mut buf = Vec::new();
        json_error(
            &mut buf,
            "INVALID_INPUT",
            "bad input",
            Some(&details),
        );
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("\"details\""));
        assert!(output.contains("\"id\": 42"));
    }

    #[test]
    fn batch_result_serializes_ok() {
        let result = BatchResult {
            id: 1,
            ok: true,
            error: None,
            code: None,
        };
        let json_str = serde_json::to_string(&result).unwrap();
        assert!(json_str.contains("\"ok\":true"));
        assert!(!json_str.contains("\"error\""));
        assert!(!json_str.contains("\"code\""));
    }

    #[test]
    fn batch_result_serializes_error() {
        let result = BatchResult {
            id: 5,
            ok: false,
            error: Some("not found".into()),
            code: Some("TASK_NOT_FOUND".into()),
        };
        let json_str = serde_json::to_string(&result).unwrap();
        assert!(json_str.contains("\"ok\":false"));
        assert!(json_str.contains("\"error\":\"not found\""));
        assert!(json_str.contains("\"code\":\"TASK_NOT_FOUND\""));
    }
}
