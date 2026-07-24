use crate::domain::{Language, OutputFormat, OutputMetadataOptions};
use crate::utils::i18n::tr;

#[derive(Debug, Clone)]
pub struct MergedFile {
    pub path: String,
    pub chars: usize,
    pub tokens: usize,
    pub content: String,
}

pub fn render_prefix(format: OutputFormat, tree: &str, language: Language) -> String {
    render_prefix_with_metadata(format, tree, language, &OutputMetadataOptions::default())
}

pub fn render_prefix_with_metadata(
    format: OutputFormat,
    tree: &str,
    language: Language,
    metadata: &OutputMetadataOptions,
) -> String {
    let mut out = String::new();
    if !metadata.directory_structure {
        if matches!(format, OutputFormat::Xml) && metadata.separator {
            out.push_str("<codemerge>\n<files>\n");
        }
        return out;
    }
    let directory_structure = tr(language, "merged_directory_structure");
    match format {
        OutputFormat::Default => {
            out.push_str(directory_structure);
            out.push_str(":\n");
            out.push_str(tree);
            out.push_str("\n\n");
        }
        OutputFormat::Xml => {
            if metadata.separator {
                out.push_str("<codemerge>\n<directory_structure><![CDATA[\n");
                out.push_str(tree);
                out.push_str("\n]]></directory_structure>\n<files>\n");
            } else {
                out.push_str(directory_structure);
                out.push_str(":\n");
                out.push_str(tree);
                out.push_str("\n\n");
            }
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
            if metadata.separator {
                out.push_str("\n\n```text\n");
                out.push_str(tree);
                out.push_str("\n```\n\n");
            } else {
                out.push_str("\n\n");
                out.push_str(tree);
                out.push_str("\n\n");
            }
        }
    }
    out
}

pub fn render_file_entry(format: OutputFormat, file: &MergedFile, language: Language) -> String {
    render_file_entry_with_metadata(format, file, language, &OutputMetadataOptions::default())
}

pub fn render_file_entry_with_metadata(
    format: OutputFormat,
    file: &MergedFile,
    language: Language,
    metadata: &OutputMetadataOptions,
) -> String {
    if metadata.is_empty() {
        return file.content.clone();
    }

    let mut out = String::new();
    let file_path = tr(language, "merged_file_path");
    let chars = tr(language, "merged_chars");
    let tokens = tr(language, "merged_tokens");
    match format {
        OutputFormat::Default => {
            if metadata.separator {
                out.push_str("============================================================\n");
            }
            if metadata.file_path {
                out.push_str(&format!("{file_path}: {}\n", file.path));
            }
            if metadata.char_token_counts {
                out.push_str(&format!(
                    "{chars}: {} | {tokens}: {}\n",
                    file.chars, file.tokens
                ));
            }
            if metadata.file_path || metadata.char_token_counts {
                out.push('\n');
            }
            out.push_str(&file.content);
            if metadata.separator {
                out.push_str("\n\n");
            }
        }
        OutputFormat::Xml => {
            if metadata.separator {
                out.push_str("  <file");
                if metadata.file_path {
                    out.push_str(&format!(" path=\"{}\"", xml_escape(&file.path)));
                }
                if metadata.char_token_counts {
                    out.push_str(&format!(
                        " chars=\"{}\" tokens=\"{}\"",
                        file.chars, file.tokens
                    ));
                }
                out.push_str("><![CDATA[\n");
                out.push_str(&file.content);
                out.push_str("\n]]></file>\n");
            } else {
                append_plain_metadata(&mut out, file, file_path, chars, tokens, metadata);
                out.push_str(&file.content);
            }
        }
        OutputFormat::PlainText => {
            if metadata.separator {
                out.push_str("================\n");
            }
            append_plain_metadata(&mut out, file, file_path, chars, tokens, metadata);
            if metadata.separator {
                out.push_str("================\n");
            }
            out.push_str(&file.content);
            if metadata.separator {
                out.push_str("\n\n");
            }
        }
        OutputFormat::Markdown => {
            if metadata.file_path {
                out.push_str(&format!("# {}\n\n", file.path));
            }
            if metadata.char_token_counts {
                out.push_str(&format!(
                    "- {}: {}\n- {}: {}\n\n",
                    chars, file.chars, tokens, file.tokens
                ));
            }
            if metadata.separator {
                out.push_str(&format!("```{}\n", lang_from_path(&file.path)));
            }
            out.push_str(&file.content);
            if metadata.separator {
                out.push_str("\n```\n\n");
            }
        }
    }
    out
}

pub fn render_suffix(format: OutputFormat) -> &'static str {
    render_suffix_with_metadata(format, &OutputMetadataOptions::default())
}

pub fn render_suffix_with_metadata(
    format: OutputFormat,
    metadata: &OutputMetadataOptions,
) -> &'static str {
    match format {
        OutputFormat::Xml if metadata.separator => "</files>\n</codemerge>\n",
        OutputFormat::Xml => "",
        OutputFormat::Default | OutputFormat::PlainText | OutputFormat::Markdown => "",
    }
}

pub fn merge_content(
    format: OutputFormat,
    tree: &str,
    files: &[MergedFile],
    language: Language,
) -> String {
    merge_content_with_metadata(
        format,
        tree,
        files,
        language,
        &OutputMetadataOptions::default(),
    )
}

pub fn merge_content_with_metadata(
    format: OutputFormat,
    tree: &str,
    files: &[MergedFile],
    language: Language,
    metadata: &OutputMetadataOptions,
) -> String {
    let mut out = render_prefix_with_metadata(format, tree, language, metadata);
    for file in files {
        out.push_str(&render_file_entry_with_metadata(
            format, file, language, metadata,
        ));
    }
    out.push_str(render_suffix_with_metadata(format, metadata));
    out
}

fn append_plain_metadata(
    out: &mut String,
    file: &MergedFile,
    file_path: &str,
    chars: &str,
    tokens: &str,
    metadata: &OutputMetadataOptions,
) {
    if metadata.file_path {
        out.push_str(&format!("{file_path}: {}\n", file.path));
    }
    if metadata.char_token_counts {
        out.push_str(&format!(
            "{chars}: {}\n{tokens}: {}\n",
            file.chars, file.tokens
        ));
    }
    if (metadata.file_path || metadata.char_token_counts) && !metadata.separator {
        out.push('\n');
    }
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

#[cfg(test)]
mod tests {
    use super::{MergedFile, merge_content_with_metadata};
    use crate::domain::{Language, OutputFormat, OutputMetadataOptions};

    fn file() -> MergedFile {
        MergedFile {
            path: "src/lib.rs".to_string(),
            chars: 12,
            tokens: 4,
            content: "pub fn demo() {}".to_string(),
        }
    }

    #[test]
    fn default_metadata_keeps_existing_output_contract() {
        let output = merge_content_with_metadata(
            OutputFormat::Default,
            "root/\n  lib.rs",
            &[file()],
            Language::En,
            &OutputMetadataOptions::default(),
        );

        assert_eq!(
            output,
            "Directory Structure:\nroot/\n  lib.rs\n\n\
============================================================\n\
File: src/lib.rs\nChars: 12 | Tokens: 4\n\n\
pub fn demo() {}\n\n"
        );
    }

    #[test]
    fn metadata_switches_are_independent() {
        let mut metadata = OutputMetadataOptions {
            directory_structure: false,
            ..OutputMetadataOptions::default()
        };
        let output = merge_content_with_metadata(
            OutputFormat::Default,
            "root/",
            &[file()],
            Language::En,
            &metadata,
        );
        assert!(!output.contains("Directory Structure"));
        assert!(output.contains("File: src/lib.rs"));

        metadata = OutputMetadataOptions {
            file_path: false,
            ..OutputMetadataOptions::default()
        };
        let output = merge_content_with_metadata(
            OutputFormat::Default,
            "root/",
            &[file()],
            Language::En,
            &metadata,
        );
        assert!(!output.contains("File: src/lib.rs"));
        assert!(output.contains("Chars: 12 | Tokens: 4"));

        metadata = OutputMetadataOptions {
            char_token_counts: false,
            ..OutputMetadataOptions::default()
        };
        let output = merge_content_with_metadata(
            OutputFormat::Default,
            "root/",
            &[file()],
            Language::En,
            &metadata,
        );
        assert!(!output.contains("Chars: 12"));
        assert!(output.contains("File: src/lib.rs"));

        metadata = OutputMetadataOptions {
            separator: false,
            ..OutputMetadataOptions::default()
        };
        let output = merge_content_with_metadata(
            OutputFormat::Default,
            "root/",
            &[file()],
            Language::En,
            &metadata,
        );
        assert!(!output.contains("===="));
        assert!(output.contains("File: src/lib.rs"));
    }

    #[test]
    fn empty_metadata_emits_only_exact_file_content_for_every_format() {
        let metadata = OutputMetadataOptions {
            directory_structure: false,
            file_path: false,
            char_token_counts: false,
            separator: false,
        };
        let files = [
            file(),
            MergedFile {
                path: "README.md".to_string(),
                chars: 4,
                tokens: 1,
                content: "NEXT".to_string(),
            },
        ];

        for format in [
            OutputFormat::Default,
            OutputFormat::Xml,
            OutputFormat::PlainText,
            OutputFormat::Markdown,
        ] {
            assert_eq!(
                merge_content_with_metadata(format, "ignored", &files, Language::En, &metadata),
                "pub fn demo() {}NEXT"
            );
        }
    }

    #[test]
    fn disabling_separators_removes_format_wrappers() {
        let metadata = OutputMetadataOptions {
            separator: false,
            ..OutputMetadataOptions::default()
        };

        let xml = merge_content_with_metadata(
            OutputFormat::Xml,
            "root/",
            &[file()],
            Language::En,
            &metadata,
        );
        assert!(!xml.contains("<codemerge>"));
        assert!(!xml.contains("<file"));
        assert!(xml.contains("File: src/lib.rs"));

        let markdown = merge_content_with_metadata(
            OutputFormat::Markdown,
            "root/",
            &[file()],
            Language::En,
            &metadata,
        );
        assert!(!markdown.contains("```"));
        assert!(markdown.contains("# src/lib.rs"));
    }
}
