use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

#[path = "support/mod.rs"]
mod support;

use codemerge::domain::{Language, OutputFormat};
use codemerge::processor::{merger, reader, walker};

// ---------------------------------------------------------------------------
// Group 1: walker::collect_candidates — parallel file walking
// ---------------------------------------------------------------------------
fn bench_walker(c: &mut Criterion) {
    let mut group = c.benchmark_group("walker_collect_candidates");
    for &scale in support::SCALES {
        let (_dir, root) = support::make_file_tree(scale);
        group.bench_with_input(BenchmarkId::from_parameter(scale), &root, |b, root| {
            b.iter(|| {
                walker::collect_candidates(
                    Some(root),
                    &[],
                    &["node_modules".into(), ".git".into()],
                    &[".jpg".into(), ".png".into()],
                    walker::WalkerOptions {
                        use_gitignore: false,
                        ignore_git: true,
                    },
                )
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Group 2: reader::count_chars_tokens — regex token counting at various sizes
// ---------------------------------------------------------------------------
fn bench_count_chars_tokens(c: &mut Criterion) {
    let mut group = c.benchmark_group("reader_count_chars_tokens");
    for &size_kb in &[1, 10, 100, 1000] {
        // ~25 bytes per line → 40 lines per KB
        let content = "fn main() { let x = 42; }\n".repeat(size_kb * 40);
        group.bench_with_input(
            BenchmarkId::new("size_kb", size_kb),
            &content,
            |b, content| {
                b.iter(|| reader::count_chars_tokens(content));
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Group 3: reader::compress_by_extension — per file-type compression cost
// ---------------------------------------------------------------------------
fn bench_compress(c: &mut Criterion) {
    let mut group = c.benchmark_group("reader_compress_by_extension");
    let variants: &[(&str, &str)] = &[
        (
            "html",
            "<html><body><div><p>Hello world</p></div></body></html>\n",
        ),
        (
            "css",
            "body { margin: 0; padding: 0; }\n.cls { color: red; }\n",
        ),
        ("js", "function foo() { var x = 1; return x + 2; }\n"),
        ("json", r#"{"key": "value", "nested": {"a": 1, "b": 2}}"#),
        (
            "rs",
            "fn main() {\n    let x = 42;\n    println!(\"{}\", x);\n}\n",
        ),
    ];
    for &(ext, template) in variants {
        let content = template.repeat(400); // ~10-20 KB per type
        let path = std::path::PathBuf::from(format!("test.{ext}"));
        group.bench_with_input(
            BenchmarkId::from_parameter(ext),
            &(&path, &content),
            |b, (path, content)| {
                b.iter(|| reader::compress_by_extension(path, content, true));
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Group 4: merger::render_file_entry — per-format rendering cost
// ---------------------------------------------------------------------------
fn bench_render_file_entry(c: &mut Criterion) {
    let mut group = c.benchmark_group("merger_render_file_entry");
    let file = merger::MergedFile {
        path: "src/components/dashboard/widget.tsx".to_string(),
        chars: 5000,
        tokens: 1200,
        content: "function Widget() { return <div>content</div>; }\n".repeat(100),
    };
    let formats: &[(&str, OutputFormat)] = &[
        ("Default", OutputFormat::Default),
        ("Xml", OutputFormat::Xml),
        ("PlainText", OutputFormat::PlainText),
        ("Markdown", OutputFormat::Markdown),
    ];
    for &(name, format) in formats {
        group.bench_with_input(BenchmarkId::from_parameter(name), &file, |b, file| {
            b.iter(|| merger::render_file_entry(format, file, Language::En));
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Group 5: merger::merge_content — full merge at multiple scales × formats
// ---------------------------------------------------------------------------
fn bench_merge_content(c: &mut Criterion) {
    let mut group = c.benchmark_group("merger_merge_content");
    group.sample_size(20);

    let formats: &[(&str, OutputFormat)] = &[
        ("Default", OutputFormat::Default),
        ("Xml", OutputFormat::Xml),
        ("Markdown", OutputFormat::Markdown),
    ];

    for &scale in support::SCALES {
        let files = support::merged_files(scale);
        let tree = "root/\n  src/\n    main.rs\n".to_string();
        for &(name, format) in formats {
            group.bench_with_input(
                BenchmarkId::new(name, scale),
                &(&files, &tree),
                |b, (files, tree)| {
                    b.iter(|| merger::merge_content(format, tree, files, Language::En));
                },
            );
        }
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_walker,
    bench_count_chars_tokens,
    bench_compress,
    bench_render_file_entry,
    bench_merge_content,
);
criterion_main!(benches);
