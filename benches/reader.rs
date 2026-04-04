/// Benchmarks for [`dari::reader::load_archive`].
///
/// Tests archive parsing performance on a large (1 000-entry) archive to catch
/// regressions in index reading, footer parsing, and entry deserialization.
use criterion::{Criterion, criterion_group, criterion_main};
use dari::{
    archive_builder::ArchiveBuilder,
    i18n::detect_locale,
    pipeline::PipelineConfig,
    reader::load_archive,
};
use std::hint::black_box;
use std::io::Cursor;

rust_i18n::i18n!("locales", fallback = "en");

/// Build a 1 000-entry in-memory archive and return its raw bytes.
///
/// Uses `ArchiveBuilder` over a `Cursor<Vec<u8>>` to avoid any filesystem I/O
/// during the benchmark warm-up phase.
fn build_large_archive() -> Vec<u8> {
    let config = PipelineConfig {
        compress_images: false,
        encryption_passphrase: None,
    };

    let tmpdir = tempfile::tempdir().expect("tempdir");
    let content = b"benchmark entry content for reader test";

    // Write a single shared file that all 1 000 entries point to.
    let src = tmpdir.path().join("entry.txt");
    std::fs::write(&src, content).unwrap();

    let archive_file = tmpdir.path().join("bench.dar");
    let fh = std::fs::File::create(&archive_file).unwrap();
    let mut builder = ArchiveBuilder::with_config(fh, config);
    builder.write_header().unwrap();

    for i in 0..1_000u32 {
        let archive_name = format!("dir_{}/file_{}.txt", i / 100, i);
        builder.add_file(&src, &archive_name).unwrap();
    }

    builder.build().unwrap();

    std::fs::read(&archive_file).unwrap()
}

fn bench_load_archive(c: &mut Criterion) {
    let archive_bytes = build_large_archive();
    let locale = detect_locale();

    c.bench_function("reader/load_archive 1000 entries", |b| {
        b.iter(|| {
            let mut cursor = Cursor::new(black_box(archive_bytes.clone()));
            let state = load_archive(&mut cursor, "bench.dar", &locale).unwrap();
            black_box(state);
        })
    });
}

criterion_group!(benches, bench_load_archive);
criterion_main!(benches);
