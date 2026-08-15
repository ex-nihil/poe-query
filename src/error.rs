use std::fmt::Display;

/// Errors produced while parsing or evaluating a query.
///
/// Everything the evaluator can fail on is represented here so embedders
/// (CLI, serve mode, tests) decide how to report it instead of the
/// evaluator terminating the process.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QueryError {
    #[error("parse error at line {line}, column {column}: {message}")]
    Parse { line: usize, column: usize, message: String },

    #[error("unknown table '{name}'{}", suggestion_suffix(.suggestion))]
    UnknownTable { name: String, suggestion: Option<String> },

    #[error("table '{table}' has no column '{column}'{}", suggestion_suffix(.suggestion))]
    UnknownColumn { table: String, column: String, suggestion: Option<String> },

    #[error("{operation} not supported on {found}")]
    TypeError { operation: String, found: String },

    #[error("table '{table}' has no data file in this installation")]
    MissingDataFile { table: String },

    #[error("schema mismatch for table '{table}': {detail}")]
    SchemaMismatch { table: String, detail: String },

    #[error("{0}")]
    Unsupported(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl QueryError {
    pub fn internal(message: impl Into<String>) -> Self {
        QueryError::Internal(message.into())
    }

    pub fn type_error(operation: impl Into<String>, found: impl Display) -> Self {
        QueryError::TypeError { operation: operation.into(), found: found.to_string() }
    }

    /// Stable machine-readable discriminator, e.g. for protocol responses.
    pub fn kind(&self) -> &'static str {
        match self {
            QueryError::Parse { .. } => "parse",
            QueryError::UnknownTable { .. } => "unknown_table",
            QueryError::UnknownColumn { .. } => "unknown_column",
            QueryError::TypeError { .. } => "type_error",
            QueryError::MissingDataFile { .. } => "missing_data_file",
            QueryError::SchemaMismatch { .. } => "schema_mismatch",
            QueryError::Unsupported(_) => "unsupported",
            QueryError::Internal(_) => "internal",
        }
    }
}

fn suggestion_suffix(suggestion: &Option<String>) -> String {
    match suggestion {
        Some(name) => format!(". Did you mean '{}'?", name),
        None => String::new(),
    }
}
