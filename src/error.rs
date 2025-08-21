use std::fmt;

/// Custom error types for the ant simulation
#[derive(Debug)]
pub enum ParseError {
    /// IO operation failed
    IoError(std::io::Error),
    /// Invalid line format in map file
    InvalidLine(String),
    /// Invalid direction string
    InvalidDirection(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::IoError(err) => write!(f, "IO error: {}", err),
            ParseError::InvalidLine(msg) => write!(f, "Invalid line: {}", msg),
            ParseError::InvalidDirection(dir) => write!(f, "Invalid direction: {}", dir),
        }
    }
}

impl std::error::Error for ParseError {}

impl From<std::io::Error> for ParseError {
    fn from(err: std::io::Error) -> Self {
        ParseError::IoError(err)
    }
}

/// Result type alias for this crate
pub type Result<T> = std::result::Result<T, ParseError>;
