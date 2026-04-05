/// Benchmarks for [`dari::tui::search::apply_fuzzy_filter`].
///
/// Exercises fuzzy matching on a 10 000-entry tree to surface regressions in
/// the `nucleo_matcher` scoring path.
use criterion::{Criterion, criterion_group, criterion_main};
use dari::{
    models::archive::{ArchiveIndexEntry, ArchiveIndexEntryWrapper},
    tui::{search::apply_fuzzy_filter, tree::build_tree},
};
use bytemuck::Zeroable;
use std::hint::black_box;

rust_i18n::i18n!("locales", fallback = "en");

/// Build a `TreeNode` root from 10 000 synthetic archive entries spread across
/// a realistic directory hierarchy.
fn make_large_tree() -> dari::tui::tree::TreeNode {
    let dirs = [
        "src", "tests", "docs", "assets", "scripts", "config", "build", "dist", "lib", "vendor",
    ];
    let exts = ["rs", "toml", "md", "png", "sh", "json", "txt", "yaml", "lock", "log"];

    let entries: Vec<ArchiveIndexEntryWrapper> = (0..10_000u32)
        .map(|i| {
            let dir = dirs[(i as usize) % dirs.len()];
            let ext = exts[(i as usize) % exts.len()];
            let path = format!("{}/subdir_{}/file_{}.{}", dir, i / 100, i, ext);
            ArchiveIndexEntryWrapper::new(
                ArchiveIndexEntry::zeroed(),
                path,
                String::new(),
            )
        })
        .collect();

    build_tree(&entries)
}

fn bench_fuzzy_filter_matching(c: &mut Criterion) {
    let root = make_large_tree();

    c.bench_function("fuzzy_filter/10k entries — matching query", |b| {
        b.iter(|| {
            let results = apply_fuzzy_filter(black_box("main"), black_box(&root));
            black_box(results);
        })
    });
}

fn bench_fuzzy_filter_no_match(c: &mut Criterion) {
    let root = make_large_tree();

    c.bench_function("fuzzy_filter/10k entries — no match", |b| {
        b.iter(|| {
            let results = apply_fuzzy_filter(black_box("zzznomatch"), black_box(&root));
            black_box(results);
        })
    });
}

fn bench_fuzzy_filter_empty_query(c: &mut Criterion) {
    let root = make_large_tree();

    c.bench_function("fuzzy_filter/10k entries — empty query (flatten)", |b| {
        b.iter(|| {
            let results = apply_fuzzy_filter(black_box(""), black_box(&root));
            black_box(results);
        })
    });
}

criterion_group!(
    benches,
    bench_fuzzy_filter_matching,
    bench_fuzzy_filter_no_match,
    bench_fuzzy_filter_empty_query,
);
criterion_main!(benches);
