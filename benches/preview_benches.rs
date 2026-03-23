use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

#[path = "support/mod.rs"]
mod support;

use codemerge::services::preview;

// ---------------------------------------------------------------------------
// Group 9: preview::index_document — line-offset index building
// ---------------------------------------------------------------------------
fn bench_index_document(c: &mut Criterion) {
    let mut group = c.benchmark_group("preview_index_document");
    for &line_count in &[100usize, 1_000, 10_000, 100_000] {
        let (_dir, path) = support::make_preview_file(line_count);
        group.bench_with_input(BenchmarkId::new("lines", line_count), &path, |b, path| {
            b.iter(|| preview::index_document(path).expect("index document"));
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Group 10: preview::load_range — seek + sequential read at various positions
// ---------------------------------------------------------------------------
fn bench_load_range(c: &mut Criterion) {
    let mut group = c.benchmark_group("preview_load_range");
    let (_dir, path) = support::make_preview_file(100_000);
    let document = preview::index_document(&path).expect("index document");

    let cases: &[(&str, std::ops::Range<usize>)] = &[
        ("small_start", 0..50),
        ("small_middle", 50_000..50_050),
        ("small_end", 99_950..100_000),
        ("medium_1k", 10_000..11_000),
        ("large_10k", 0..10_000),
    ];
    for (name, range) in cases {
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            &(&document, range),
            |b, (doc, range)| {
                b.iter(|| preview::load_range(doc, (*range).clone()).expect("load range"));
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_index_document, bench_load_range);
criterion_main!(benches);
