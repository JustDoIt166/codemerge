use codemerge::app::model::OutputFormat;
use codemerge::processor::{merger, reader, walker};

#[test]
fn parse_gitignore_ignores_comments_empty_and_negation() {
    let text = "\n# c1\nnode_modules/\n/dist\n!important\n";
    let rules = walker::parse_gitignore_rules(text);
    assert_eq!(rules, vec!["node_modules", "dist"]);
}

#[test]
fn normalize_ext_adds_dot() {
    assert_eq!(walker::normalize_ext("jpg"), ".jpg");
    assert_eq!(walker::normalize_ext(".png"), ".png");
}

#[test]
fn token_count_works() {
    let (chars, tokens) = reader::count_chars_tokens("fn main() { println!(\"hi\"); }");
    assert!(chars > 0);
    assert!(tokens > 0);
}

#[test]
fn merge_formats_non_empty() {
    let files = vec![merger::MergedFile {
        path: "src/main.rs".to_string(),
        chars: 12,
        tokens: 6,
        content: "fn main() {}".to_string(),
    }];

    let d = merger::merge_content(OutputFormat::Default, "root/", &files);
    let x = merger::merge_content(OutputFormat::Xml, "root/", &files);
    let p = merger::merge_content(OutputFormat::PlainText, "root/", &files);
    let m = merger::merge_content(OutputFormat::Markdown, "root/", &files);

    assert!(d.contains("Directory Structure"));
    assert!(x.contains("<codemerge>"));
    assert!(p.contains("File: src/main.rs"));
    assert!(m.contains("# Directory Structure"));
}
