pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPOSITORY_URL: &str = env!("CARGO_PKG_REPOSITORY");

pub fn version() -> &'static str {
    APP_VERSION
}

pub fn version_label() -> String {
    format!("v{}", version())
}

pub fn repository_url() -> &'static str {
    REPOSITORY_URL
}

#[cfg(test)]
mod tests {
    use super::{repository_url, version, version_label};

    #[test]
    fn version_label_uses_v_prefix() {
        assert_eq!(version_label(), format!("v{}", version()));
    }

    #[test]
    fn repository_url_is_populated() {
        assert!(repository_url().starts_with("https://"));
        assert!(repository_url().contains("github.com"));
    }
}
