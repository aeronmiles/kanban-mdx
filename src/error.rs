//! Structured error types for CLI commands.
//!
//! Errors carry a machine-readable code, a human-readable message,
//! and optional details for agent consumption.

use std::collections::HashMap;
use std::fmt;

/// Machine-readable error codes, stable across minor versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    TaskNotFound,
    BoardNotFound,
    BoardAlreadyExists,
    InvalidInput,
    InvalidStatus,
    InvalidPriority,
    InvalidDate,
    InvalidTaskId,
    WipLimitExceeded,
    DependencyNotFound,
    SelfReference,
    NoChanges,
    BoundaryError,
    StatusConflict,
    ConfirmationRequired,
    TaskClaimed,
    InvalidClass,
    ClassWipExceeded,
    ClaimRequired,
    NothingToPick,
    InvalidGroupBy,
    InternalError,
}

impl ErrorCode {
    /// Returns the uppercase underscore-separated string constant.
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::TaskNotFound => "TASK_NOT_FOUND",
            ErrorCode::BoardNotFound => "BOARD_NOT_FOUND",
            ErrorCode::BoardAlreadyExists => "BOARD_ALREADY_EXISTS",
            ErrorCode::InvalidInput => "INVALID_INPUT",
            ErrorCode::InvalidStatus => "INVALID_STATUS",
            ErrorCode::InvalidPriority => "INVALID_PRIORITY",
            ErrorCode::InvalidDate => "INVALID_DATE",
            ErrorCode::InvalidTaskId => "INVALID_TASK_ID",
            ErrorCode::WipLimitExceeded => "WIP_LIMIT_EXCEEDED",
            ErrorCode::DependencyNotFound => "DEPENDENCY_NOT_FOUND",
            ErrorCode::SelfReference => "SELF_REFERENCE",
            ErrorCode::NoChanges => "NO_CHANGES",
            ErrorCode::BoundaryError => "BOUNDARY_ERROR",
            ErrorCode::StatusConflict => "STATUS_CONFLICT",
            ErrorCode::ConfirmationRequired => "CONFIRMATION_REQUIRED",
            ErrorCode::TaskClaimed => "TASK_CLAIMED",
            ErrorCode::InvalidClass => "INVALID_CLASS",
            ErrorCode::ClassWipExceeded => "CLASS_WIP_EXCEEDED",
            ErrorCode::ClaimRequired => "CLAIM_REQUIRED",
            ErrorCode::NothingToPick => "NOTHING_TO_PICK",
            ErrorCode::InvalidGroupBy => "INVALID_GROUP_BY",
            ErrorCode::InternalError => "INTERNAL_ERROR",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A structured CLI error with a machine-readable code, human-readable message,
/// and optional details map for agent consumption.
#[derive(Debug, Clone)]
pub struct CliError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Option<HashMap<String, serde_json::Value>>,
}

impl CliError {
    /// Creates a new `CliError` with the given code and message.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        CliError {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// Creates a new `CliError` with a pre-formatted message string.
    ///
    /// Use with `format!()` at the call site:
    /// ```ignore
    /// CliError::newf(ErrorCode::TaskNotFound, format!("task #{} not found", id))
    /// ```
    pub fn newf(code: ErrorCode, message: String) -> Self {
        CliError {
            code,
            message,
            details: None,
        }
    }

    /// Attaches a details map to this error (builder pattern).
    pub fn with_details(mut self, details: HashMap<String, serde_json::Value>) -> Self {
        self.details = Some(details);
        self
    }

    /// Returns the process exit code: 2 for `InternalError`, 1 for all others.
    pub fn exit_code(&self) -> i32 {
        if self.code == ErrorCode::InternalError {
            2
        } else {
            1
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CliError {}

/// A silent error that signals an exit code without additional output.
/// Used by batch operations where results are already written to stdout.
#[derive(Debug, Clone)]
pub struct SilentError {
    pub code: i32,
}

impl SilentError {
    /// Creates a new `SilentError` with the given exit code.
    pub fn new(code: i32) -> Self {
        SilentError { code }
    }
}

impl fmt::Display for SilentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exit {}", self.code)
    }
}

impl std::error::Error for SilentError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_display() {
        assert_eq!(ErrorCode::TaskNotFound.to_string(), "TASK_NOT_FOUND");
        assert_eq!(ErrorCode::InternalError.to_string(), "INTERNAL_ERROR");
        assert_eq!(
            ErrorCode::ConfirmationRequired.to_string(),
            "CONFIRMATION_REQUIRED"
        );
    }

    #[test]
    fn cli_error_new() {
        let err = CliError::new(ErrorCode::TaskNotFound, "task not found");
        assert_eq!(err.code, ErrorCode::TaskNotFound);
        assert_eq!(err.message, "task not found");
        assert!(err.details.is_none());
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn cli_error_newf() {
        let err = CliError::newf(ErrorCode::InvalidInput, format!("bad value: {}", "foo"));
        assert_eq!(err.message, "bad value: foo");
    }

    #[test]
    fn cli_error_with_details() {
        let mut details = HashMap::new();
        details.insert("key".to_string(), serde_json::json!("value"));
        let err = CliError::new(ErrorCode::TaskNotFound, "not found").with_details(details);
        assert!(err.details.is_some());
        assert_eq!(
            err.details.as_ref().unwrap().get("key").unwrap(),
            &serde_json::json!("value")
        );
    }

    #[test]
    fn cli_error_exit_codes() {
        assert_eq!(
            CliError::new(ErrorCode::InternalError, "internal").exit_code(),
            2
        );
        assert_eq!(
            CliError::new(ErrorCode::TaskNotFound, "not found").exit_code(),
            1
        );
        assert_eq!(
            CliError::new(ErrorCode::InvalidInput, "bad").exit_code(),
            1
        );
    }

    #[test]
    fn cli_error_display() {
        let err = CliError::new(ErrorCode::BoardNotFound, "board not found");
        assert_eq!(format!("{err}"), "board not found");
    }

    #[test]
    fn silent_error_display() {
        let err = SilentError::new(1);
        assert_eq!(format!("{err}"), "exit 1");
        let err = SilentError::new(42);
        assert_eq!(format!("{err}"), "exit 42");
    }
}
