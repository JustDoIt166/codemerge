use std::fmt::Display;

use crate::error::{AppError, AppResult};
use crate::utils::app_metadata;

pub fn open_repository() -> AppResult<()> {
    open_repository_with(|url| open::that_detached(url))
}

fn open_repository_with<F, E>(opener: F) -> AppResult<()>
where
    F: FnOnce(&str) -> Result<(), E>,
    E: Display,
{
    open_url_with(app_metadata::repository_url(), opener)
}

fn open_url_with<F, E>(url: &str, opener: F) -> AppResult<()>
where
    F: FnOnce(&str) -> Result<(), E>,
    E: Display,
{
    let url = validate_url(url)?;
    opener(url).map_err(|err| AppError::new(format!("open url failed: {err}")))
}

fn validate_url(url: &str) -> AppResult<&str> {
    let url = url.trim();
    if url.is_empty() {
        return Err(AppError::new("repository url is empty"));
    }

    let Some((scheme, rest)) = url.split_once("://") else {
        return Err(AppError::new(
            "repository url must start with http:// or https://",
        ));
    };

    if !matches!(scheme, "http" | "https") {
        return Err(AppError::new(
            "repository url must start with http:// or https://",
        ));
    }

    if rest.is_empty() || url.chars().any(char::is_whitespace) {
        return Err(AppError::new("repository url is invalid"));
    }

    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::{open_repository_with, open_url_with};
    use crate::utils::app_metadata;

    #[test]
    fn open_repository_uses_repository_url() {
        let mut opened_url = None;

        let result = open_repository_with(|url| {
            opened_url = Some(url.to_string());
            Ok::<(), &'static str>(())
        });

        assert!(result.is_ok());
        assert_eq!(opened_url.as_deref(), Some(app_metadata::repository_url()));
    }

    #[test]
    fn open_url_rejects_blank_url() {
        let result = open_url_with("   ", |_| Ok::<(), &'static str>(()));

        assert_eq!(result.unwrap_err().to_string(), "repository url is empty");
    }

    #[test]
    fn open_url_rejects_non_http_urls() {
        let result = open_url_with("github.com/hellotime/codemerge", |_| {
            Ok::<(), &'static str>(())
        });

        assert_eq!(
            result.unwrap_err().to_string(),
            "repository url must start with http:// or https://"
        );
    }

    #[test]
    fn open_url_maps_opener_errors() {
        let result = open_url_with("https://github.com/hellotime/codemerge", |_| {
            Err::<(), _>("launcher unavailable")
        });

        assert_eq!(
            result.unwrap_err().to_string(),
            "open url failed: launcher unavailable"
        );
    }
}
