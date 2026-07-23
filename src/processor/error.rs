use std::error::Error;
use std::fmt::{Display, Formatter};

pub type ProcessorResult<T> = Result<T, ProcessorError>;

#[derive(Debug)]
pub enum ProcessorError {
    Io {
        operation: &'static str,
        source: std::io::Error,
    },
    Archive {
        operation: &'static str,
        source: zip::result::ZipError,
    },
}

impl ProcessorError {
    pub fn io(operation: &'static str, source: std::io::Error) -> Self {
        Self::Io { operation, source }
    }

    pub fn archive(operation: &'static str, source: zip::result::ZipError) -> Self {
        Self::Archive { operation, source }
    }
}

impl Display for ProcessorError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { operation, source } => {
                write!(f, "{operation}: {source}")
            }
            Self::Archive { operation, source } => {
                write!(f, "{operation}: {source}")
            }
        }
    }
}

impl Error for ProcessorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Archive { source, .. } => Some(source),
        }
    }
}
