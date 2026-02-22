use crate::app::model::OutputFormat;

#[derive(Debug, Clone)]
pub struct MergedFile {
    pub path: String,
    pub chars: usize,
    pub tokens: usize,
    pub content: String,
}

pub fn merge_content(format: OutputFormat, tree: &str, files: &[MergedFile]) -> String {
    match format {
        OutputFormat::Default => merge_default(tree, files),
        OutputFormat::Xml => merge_xml(tree, files),
        OutputFormat::PlainText => merge_plain(tree, files),
        OutputFormat::Markdown => merge_markdown(tree, files),
    }
}

fn merge_default(tree: &str, files: &[MergedFile]) -> String {
    let mut out = String::new();
    out.push_str("Directory Structure:\n");
    out.push_str(tree);
    out.push_str("\n\n");
    for f in files {
        out.push_str("============================================================\n");
        out.push_str(&format!(
            "文件路径: {}\n字符数: {} | Token估算: {}\n\n",
            f.path, f.chars, f.tokens
        ));
        out.push_str(&f.content);
        out.push_str("\n\n");
    }
    out
}

fn merge_xml(tree: &str, files: &[MergedFile]) -> String {
    let mut out = String::from("<codemerge>\n<directory_structure><![CDATA[\n");
    out.push_str(tree);
    out.push_str("\n]]></directory_structure>\n<files>\n");
    for f in files {
        out.push_str(&format!(
            "  <file path=\"{}\" chars=\"{}\" tokens=\"{}\"><![CDATA[\n{}\n]]></file>\n",
            xml_escape(&f.path),
            f.chars,
            f.tokens,
            f.content
        ));
    }
    out.push_str("</files>\n</codemerge>\n");
    out
}

fn merge_plain(tree: &str, files: &[MergedFile]) -> String {
    let mut out = String::new();
    out.push_str("---------------- Directory Structure ----------------\n");
    out.push_str(tree);
    out.push_str("\n\n---------------- Files ----------------\n");
    for f in files {
        out.push_str(&format!(
            "\nFile: {}\nChars: {} Tokens: {}\n",
            f.path, f.chars, f.tokens
        ));
        out.push_str(&f.content);
        out.push('\n');
    }
    out
}

fn merge_markdown(tree: &str, files: &[MergedFile]) -> String {
    let mut out = String::from("# Directory Structure\n\n```text\n");
    out.push_str(tree);
    out.push_str("\n```\n\n# Files\n\n");
    for f in files {
        let lang = lang_from_path(&f.path);
        out.push_str(&format!(
            "## {}\n\n- chars: {}\n- tokens: {}\n\n```{}\n{}\n```\n\n",
            f.path, f.chars, f.tokens, lang, f.content
        ));
    }
    out
}

fn lang_from_path(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".rs") {
        "rust"
    } else if lower.ends_with(".js") {
        "javascript"
    } else if lower.ends_with(".ts") {
        "typescript"
    } else if lower.ends_with(".py") {
        "python"
    } else if lower.ends_with(".html") {
        "html"
    } else if lower.ends_with(".css") {
        "css"
    } else if lower.ends_with(".json") {
        "json"
    } else if lower.ends_with(".md") {
        "markdown"
    } else {
        "text"
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
