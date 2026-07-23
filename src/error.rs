use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    Unknown,
    Io,
    Config,
    Processing,
    Preview,
    TaskCancelled,
    TaskPanicked,
}

#[derive(Debug, Clone)]
pub struct AppError {
    code: ErrorCode,
    operation: Arc<str>,
    message: Arc<str>,
    source: Option<Arc<dyn Error + Send + Sync>>,
}

impl AppError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::Unknown,
            operation: Arc::from("application"),
            message: Arc::from(message.into()),
            source: None,
        }
    }

    pub fn with_code(
        code: ErrorCode,
        operation: impl Into<Arc<str>>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            operation: operation.into(),
            message: Arc::from(message.into()),
            source: None,
        }
    }

    pub fn from_source<E>(
        code: ErrorCode,
        operation: impl Into<Arc<str>>,
        message: impl Into<String>,
        source: E,
    ) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self {
            code,
            operation: operation.into(),
            message: Arc::from(message.into()),
            source: Some(Arc::new(source)),
        }
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn operation(&self) -> &str {
        &self.operation
    }

    pub fn context(self, prefix: impl AsRef<str>) -> Self {
        Self {
            code: self.code,
            operation: Arc::from(prefix.as_ref()),
            message: Arc::from(format!("{}: {}", prefix.as_ref(), self.message)),
            source: Some(Arc::new(self)),
        }
    }
}

impl PartialEq for AppError {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code
            && self.operation == other.operation
            && self.message == other.message
    }
}

impl Eq for AppError {}

impl Display for AppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

impl From<String> for AppError {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for AppError {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        let message = value.to_string();
        Self::from_source(ErrorCode::Io, "io", message, value)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        let message = value.to_string();
        Self::from_source(ErrorCode::Config, "json", message, value)
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::{AppError, ErrorCode};

    #[test]
    fn context_prefixes_message() {
        let error =
            AppError::with_code(ErrorCode::Config, "write", "write failed").context("config");
        assert_eq!(error.to_string(), "config: write failed");
        assert_eq!(error.code(), ErrorCode::Config);
        assert_eq!(error.operation(), "config");
        assert_eq!(
            error.source().map(ToString::to_string).as_deref(),
            Some("write failed")
        );
    }

    #[test]
    fn io_conversion_preserves_source() {
        let error = AppError::from(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        ));

        assert_eq!(error.code(), ErrorCode::Io);
        assert_eq!(error.operation(), "io");
        assert_eq!(
            error.source().map(ToString::to_string).as_deref(),
            Some("denied")
        );
    }

    #[test]
    fn nested_context_keeps_the_complete_source_chain() {
        let error = AppError::from(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"))
            .context("processor")
            .context("workspace");

        let first = error.source().expect("processor source");
        let second = first.source().expect("io wrapper source");
        let third = second.source().expect("io source");
        assert_eq!(first.to_string(), "processor: missing");
        assert_eq!(second.to_string(), "missing");
        assert_eq!(third.to_string(), "missing");
        assert!(third.source().is_none());
    }

    #[test]
    fn json_conversion_preserves_parser_source() {
        let source = serde_json::from_str::<serde_json::Value>("{").expect_err("invalid json");
        let error = AppError::from(source);

        assert_eq!(error.code(), ErrorCode::Config);
        assert!(error.source().is_some());
    }
}
