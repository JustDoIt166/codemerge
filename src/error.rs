use std::error::Error;
use std::fmt::{Display, Formatter};

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppError {
    message: String,
}

impl AppError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn context(self, prefix: impl AsRef<str>) -> Self {
        Self::new(format!("{}: {}", prefix.as_ref(), self.message))
    }
}

impl Display for AppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for AppError {}

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
        Self::new(value.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::new(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::AppError;

    #[test]
    fn context_prefixes_message() {
        let error = AppError::new("write failed").context("config");
        assert_eq!(error.to_string(), "config: write failed");
    }
}
