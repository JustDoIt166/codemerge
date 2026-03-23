use codemerge::domain::OutputFormat;
use codemerge::processor::{merger, reader, walker};
use std::fs;
use std::io::Write;
use tempfile::tempdir;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

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
fn merge_formats_match_expected_structure() {
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
    assert!(d.contains("文件路径: src/main.rs"));
    assert!(d.contains("字符数: 12 | Token估算: 6"));
    assert!(x.contains("<codemerge>"));
    assert!(x.contains("<directory_structure><![CDATA[\nroot/\n]]></directory_structure>"));
    assert!(x.contains("<file path=\"src/main.rs\" chars=\"12\" tokens=\"6\"><![CDATA["));
    assert!(p.contains("File: src/main.rs"));
    assert!(p.contains("================"));
    assert!(p.contains("Directory Structure:\nroot/"));
    assert!(m.contains("# Directory Structure"));
    assert!(m.contains("# src/main.rs"));
    assert!(m.contains("```rust"));
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

#[test]
fn collect_candidates_keeps_explicit_selected_file_even_if_blacklisted() {
    let dir = tempdir().expect("create temp dir");
    let file_path = dir.path().join("blocked.log");
    fs::write(&file_path, "selected file").expect("write selected file");

    let out = walker::collect_candidates(
        None,
        std::slice::from_ref(&file_path),
        &[String::from("blocked.log")],
        &[String::from(".log")],
        walker::WalkerOptions {
            use_gitignore: false,
            ignore_git: false,
        },
    );

    let rels: Vec<_> = out.candidates.into_iter().map(|c| c.relative).collect();
    assert_eq!(rels, vec!["blocked.log".to_string()]);
    assert_eq!(out.skipped, 0);
}

#[test]
fn collect_candidates_selected_zip_honors_blacklist_inside_archive() {
    let dir = tempdir().expect("create temp dir");
    let root = dir.path();
    let zip_path = root.join("bundle.zip");
    write_test_zip(
        &zip_path,
        &[
            ("src/lib.rs", "pub fn zipped() {}\n"),
            ("README.md", "# zipped\n"),
            ("assets/logo.png", "binary"),
        ],
    );

    let out = walker::collect_candidates(
        None,
        std::slice::from_ref(&zip_path),
        &[String::from("src")],
        &[String::from(".png")],
        walker::WalkerOptions {
            use_gitignore: false,
            ignore_git: false,
        },
    );

    let rels: Vec<_> = out.candidates.into_iter().map(|c| c.relative).collect();
    assert_eq!(rels, vec!["bundle.zip/README.md".to_string()]);
    assert_eq!(out.skipped, 2);
}

#[test]
fn collect_candidates_folder_scan_still_honors_blacklist_inside_archive() {
    let dir = tempdir().expect("create temp dir");
    let root = dir.path();
    let zip_path = root.join("bundle.zip");
    write_test_zip(
        &zip_path,
        &[
            ("src/lib.rs", "pub fn zipped() {}\n"),
            ("README.md", "# zipped\n"),
            ("assets/logo.png", "binary"),
        ],
    );

    let out = walker::collect_candidates(
        Some(&root.to_path_buf()),
        &[],
        &[String::from("src")],
        &[String::from(".png")],
        walker::WalkerOptions {
            use_gitignore: false,
            ignore_git: false,
        },
    );

    let rels: Vec<_> = out.candidates.into_iter().map(|c| c.relative).collect();
    assert_eq!(rels, vec!["bundle.zip/README.md".to_string()]);
    assert_eq!(out.skipped, 2);
}

fn write_test_zip(path: &std::path::Path, files: &[(&str, &str)]) {
    let file = fs::File::create(path).expect("create zip");
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, content) in files {
        zip.start_file(name, options).expect("start file");
        zip.write_all(content.as_bytes()).expect("write zip entry");
    }
    zip.finish().expect("finish zip");
}
