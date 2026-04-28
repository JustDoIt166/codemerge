use crate::domain::{Language, OutputFormat};
use crate::utils::i18n::tr;

#[derive(Debug, Clone)]
pub struct MergedFile {
    pub path: String,
    pub chars: usize,
    pub tokens: usize,
    pub content: String,
}

pub fn render_prefix(format: OutputFormat, tree: &str, language: Language) -> String {
    let mut out = String::new();
    let directory_structure = tr(language, "merged_directory_structure");
    match format {
        OutputFormat::Default => {
            out.push_str(directory_structure);
            out.push_str(":\n");
            out.push_str(tree);
            out.push_str("\n\n");
        }
        OutputFormat::Xml => {
            out.push_str("<codemerge>\n<directory_structure><![CDATA[\n");
            out.push_str(tree);
            out.push_str("\n]]></directory_structure>\n<files>\n");
        }
        OutputFormat::PlainText => {
            out.push_str(directory_structure);
            out.push_str(":\n");
            out.push_str(tree);
            out.push_str("\n\n");
        }
        OutputFormat::Markdown => {
            out.push_str("# ");
            out.push_str(directory_structure);
            out.push_str("\n\n```text\n");
            out.push_str(tree);
            out.push_str("\n```\n\n");
        }
    }
    out
}

pub fn render_file_entry(format: OutputFormat, file: &MergedFile, language: Language) -> String {
    let mut out = String::new();
    let file_path = tr(language, "merged_file_path");
    let chars = tr(language, "merged_chars");
    let tokens = tr(language, "merged_tokens");
    match format {
        OutputFormat::Default => {
            out.push_str("============================================================\n");
            out.push_str(&format!(
                "{file_path}: {}\n{chars}: {} | {tokens}: {}\n\n",
                file.path, file.chars, file.tokens
            ));
            out.push_str(&file.content);
            out.push_str("\n\n");
        }
        OutputFormat::Xml => {
            out.push_str(&format!(
                "  <file path=\"{}\" chars=\"{}\" tokens=\"{}\"><![CDATA[\n{}\n]]></file>\n",
                xml_escape(&file.path),
                file.chars,
                file.tokens,
                file.content
            ));
        }
        OutputFormat::PlainText => {
            out.push_str(&format!(
                "================\n{file_path}: {}\n{chars}: {}\n{tokens}: {}\n================\n",
                file.path, file.chars, file.tokens
            ));
            out.push_str(&file.content);
            out.push_str("\n\n");
        }
        OutputFormat::Markdown => {
            let lang = lang_from_path(&file.path);
            out.push_str(&format!(
                "# {}\n\n- {}: {}\n- {}: {}\n\n```{}\n{}\n```\n\n",
                file.path, chars, file.chars, tokens, file.tokens, lang, file.content
            ));
        }
    }
    out
}

pub fn render_suffix(format: OutputFormat) -> &'static str {
    match format {
        OutputFormat::Xml => "</files>\n</codemerge>\n",
        OutputFormat::Default | OutputFormat::PlainText | OutputFormat::Markdown => "",
    }
}

pub fn merge_content(
    format: OutputFormat,
    tree: &str,
    files: &[MergedFile],
    language: Language,
) -> String {
    let mut out = render_prefix(format, tree, language);
    for file in files {
        out.push_str(&render_file_entry(format, file, language));
    }
    out.push_str(render_suffix(format));
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
