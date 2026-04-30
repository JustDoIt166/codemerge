use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

#[path = "support/mod.rs"]
mod support;

use codemerge::domain::{Language, OutputFormat, TemporaryWhitelistMode};
use codemerge::processor::merger;
use codemerge::processor::reader::{compress_by_extension, count_chars_tokens};
use codemerge::processor::walker;
use codemerge::services::tree;

// ---------------------------------------------------------------------------
// Group 6: tree::build_tree_nodes — tree construction from candidates
// ---------------------------------------------------------------------------
fn bench_build_tree_nodes(c: &mut Criterion) {
    let mut group = c.benchmark_group("tree_build_tree_nodes");
    for &scale in support::SCALES {
        let (_dir, root) = support::make_file_tree(scale);
        let candidates = support::candidate_files(&root, scale);
        group.bench_with_input(
            BenchmarkId::from_parameter(scale),
            &candidates,
            |b, candidates| {
                b.iter(|| tree::build_tree_nodes(candidates));
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Group 7: tree::build_tree_index — tree indexing with stats
// ---------------------------------------------------------------------------
fn bench_build_tree_index(c: &mut Criterion) {
    let mut group = c.benchmark_group("tree_build_tree_index");
    for &scale in support::SCALES {
        let (_dir, root) = support::make_file_tree(scale);
        let candidates = support::candidate_files(&root, scale);
        let nodes = tree::build_tree_nodes(&candidates);
        group.bench_with_input(BenchmarkId::from_parameter(scale), &nodes, |b, nodes| {
            b.iter(|| tree::build_tree_index(nodes));
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Group 8: full_pipeline — walk → read → compress → count → merge
// ---------------------------------------------------------------------------
fn bench_full_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_pipeline");
    group.sample_size(10);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime");

    // Only test 100 and 1_000 — 10_000 is too slow for iterative benchmarks
    for &scale in &[100usize, 1_000] {
        let (_dir, root) = support::make_file_tree(scale);

        group.bench_with_input(BenchmarkId::from_parameter(scale), &root, |b, root| {
            b.iter(|| {
                // Phase 1: Walk
                let walker_output = walker::collect_candidates(
                    Some(root),
                    &[],
                    walker::WalkerFilterRules {
                        folder_blacklist: &["node_modules".into(), ".git".into()],
                        ext_blacklist: &[],
                        folder_whitelist: &[],
                        ext_whitelist: &[],
                        whitelist_mode: TemporaryWhitelistMode::WhitelistThenBlacklist,
                    },
                    walker::WalkerOptions {
                        use_gitignore: false,
                        ignore_git: true,
                    },
                );

                // Phase 2: Read + compress + count for each candidate
                let merged_files: Vec<merger::MergedFile> = rt.block_on(async {
                    let mut files = Vec::with_capacity(walker_output.candidates.len());
                    for candidate in &walker_output.candidates {
                        let raw =
                            codemerge::processor::reader::read_text(&candidate.absolute).await;
                        let raw = match raw {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        let (compressed, _) =
                            compress_by_extension(&candidate.absolute, &raw, false);
                        let (chars, tokens) = count_chars_tokens(&compressed);
                        files.push(merger::MergedFile {
                            path: candidate.relative.clone(),
                            chars,
                            tokens,
                            content: compressed,
                        });
                    }
                    files
                });

                // Phase 3: Merge
                merger::merge_content(
                    OutputFormat::Default,
                    &walker_output.tree,
                    &merged_files,
                    Language::En,
                );
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_build_tree_nodes,
    bench_build_tree_index,
    bench_full_pipeline,
);
criterion_main!(benches);
