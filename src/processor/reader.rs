use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

static TOKEN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\w+|[^\s\w]").expect("valid token regex"));
static WS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").expect("valid ws regex"));

pub async fn read_text(path: &Path) -> Result<String, String> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| format!("read failed: {e}"))?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

pub fn count_chars_tokens(content: &str) -> (usize, usize) {
    let chars = content.chars().count();
    let tokens = TOKEN_RE.find_iter(content).count();
    (chars, tokens)
}

pub fn compress_by_extension(
    path: &Path,
    content: &str,
    enabled: bool,
) -> (String, Option<String>) {
    if !enabled {
        return (content.to_string(), None);
    }

    let ext = path
        .extension()
        .map(|v| v.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "html" | "htm" => {
            let cfg = minify_html::Cfg {
                minify_css: true,
                minify_js: true,
                ..minify_html::Cfg::new()
            };
            let bytes = minify_html::minify(content.as_bytes(), &cfg);
            (String::from_utf8_lossy(&bytes).to_string(), None)
        }
        "css" => match minifier::css::minify(content) {
            Ok(v) => (v.to_string(), None),
            Err(e) => (content.to_string(), Some(format!("css minify failed: {e}"))),
        },
        "js" => (minifier::js::minify(content).to_string(), None),
        "json" => match serde_json::from_str::<serde_json::Value>(content) {
            Ok(v) => (
                serde_json::to_string(&v).unwrap_or_else(|_| content.to_string()),
                None,
            ),
            Err(e) => (
                content.to_string(),
                Some(format!("json minify failed: {e}")),
            ),
        },
        _ => {
            let s = WS_RE.replace_all(content, " ").to_string();
            (s, None)
        }
    }
}
