use codemerge::domain::OutputFormat;
use codemerge::processor::{merger, reader, walker};
use std::fs;
use tempfile::tempdir;

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

#[test]
fn collect_candidates_honors_gitignore_and_ext_blacklist() {
    let dir = tempdir().expect("create temp dir");
    let root = dir.path();
    fs::write(root.join(".gitignore"), "ignored_dir/\n*.log\n").expect("write gitignore");
    fs::create_dir_all(root.join("src")).expect("create src dir");
    fs::create_dir_all(root.join("ignored_dir")).expect("create ignored dir");
    fs::write(root.join("src").join("main.rs"), "fn main() {}").expect("write rs file");
    fs::write(root.join("src").join("skip.tmp"), "tmp").expect("write tmp file");
    fs::write(root.join("ignored_dir").join("inside.rs"), "ignored").expect("write ignored file");
    fs::write(root.join("debug.log"), "ignored by gitignore").expect("write log file");

    let out = walker::collect_candidates(
        Some(&root.to_path_buf()),
        &[],
        &[],
        &[String::from(".tmp")],
        walker::WalkerOptions {
            use_gitignore: true,
            ignore_git: false,
        },
    );

    let rels: Vec<_> = out.candidates.into_iter().map(|c| c.relative).collect();
    assert_eq!(
        rels,
        vec![".gitignore".to_string(), "src/main.rs".to_string()]
    );
    assert_eq!(out.skipped, 1);
}

#[test]
fn collect_candidates_can_disable_gitignore_rules() {
    let dir = tempdir().expect("create temp dir");
    let root = dir.path();
    fs::write(root.join(".gitignore"), "ignored.txt\n").expect("write gitignore");
    fs::write(root.join("ignored.txt"), "ignored").expect("write ignored file");
    fs::write(root.join("kept.txt"), "kept").expect("write kept file");

    let out = walker::collect_candidates(
        Some(&root.to_path_buf()),
        &[],
        &[],
        &[],
        walker::WalkerOptions {
            use_gitignore: false,
            ignore_git: false,
        },
    );

    let rels: Vec<_> = out.candidates.into_iter().map(|c| c.relative).collect();
    assert!(rels.contains(&"ignored.txt".to_string()));
    assert!(rels.contains(&"kept.txt".to_string()));
}

#[test]
fn collect_candidates_can_ignore_git_directory() {
    let dir = tempdir().expect("create temp dir");
    let root = dir.path();
    fs::create_dir_all(root.join(".git")).expect("create git dir");
    fs::write(root.join(".git").join("config"), "hidden").expect("write git config");
    fs::write(root.join("visible.txt"), "visible").expect("write visible file");

    let out = walker::collect_candidates(
        Some(&root.to_path_buf()),
        &[],
        &[],
        &[],
        walker::WalkerOptions {
            use_gitignore: false,
            ignore_git: true,
        },
    );

    let rels: Vec<_> = out.candidates.into_iter().map(|c| c.relative).collect();
    assert!(rels.contains(&"visible.txt".to_string()));
    assert!(!rels.iter().any(|rel| rel.starts_with(".git/")));
}
