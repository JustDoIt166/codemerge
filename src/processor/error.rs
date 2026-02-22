use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProcessorError {
    #[error("No valid files found")]
    NoValidFiles,
    #[error("IO error: {0}")]
    Io(String),
    #[error("Cancelled")]
    Cancelled,
}
