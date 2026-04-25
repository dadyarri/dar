/// Benchmarks for [`dari::reader::load_archive`].
///
/// Tests archive parsing performance on a large (1 000-entry) archive to catch
/// regressions in index reading, footer parsing, and entry deserialization.
///
/// The archive is built entirely in-memory — no filesystem I/O occurs during
/// setup.  The benchmark loop creates a fresh `Cursor<&[u8]>` (O(1), no copy)
/// rather than cloning the archive bytes on every iteration.
use criterion::{criterion_group, criterion_main, Criterion};
use dari::{
    archive_builder::ArchiveBuilder,
    file_reader::PreparedFile,
    i18n::detect_locale,
    pipeline::{CompressionPipeline, PipelineConfig},
    reader::load_archive,
};
use std::hint::black_box;
use std::io::Cursor;


rust_i18n::i18n!("locales", fallback = "en");

/// Build a 1 000-entry archive entirely in memory and return the raw bytes.
///
/// Uses `CompressionPipeline::process_file` with a synthetic path so that no
/// real files are read from disk during setup.
fn build_large_archive() -> Vec<u8> {
    let config = PipelineConfig {
        compress_images: false,
        encryption_passphrase: None,
        chunked_encryption: false,
        preserve_xattrs: false,
    };
    let pipeline = CompressionPipeline::new(config.clone());
    let cursor = Cursor::new(Vec::<u8>::new());
    let mut builder = ArchiveBuilder::with_config(cursor, config);
    builder.write_header().unwrap();

    let content = b"benchmark entry content for reader test".to_vec();
    for i in 0..1_000u32 {
        let archive_name = format!("dir_{}/file_{}.txt", i / 100, i);
        let pipeline_result = pipeline
            .process_file(std::path::Path::new("entry.txt"), content.clone())
            .unwrap();
        let prepared = PreparedFile {
            archive_path: archive_name,
            pipeline_result,
            timestamp: 0,
            uid: 1000,
            gid: 1000,
            perm: 644,
            xattrs: vec![],
            device_inode: None,
        };
        builder.commit_prepared(prepared).unwrap();
    }

    builder.build().unwrap();
    builder.into_inner().into_inner()
}

fn bench_load_archive(c: &mut Criterion) {
    let archive_bytes = build_large_archive();
    let locale = detect_locale();

    c.bench_function("reader/load_archive 1000 entries", |b| {
        // Cursor<&[u8]> is O(1) to create — we borrow archive_bytes, no clone.
        b.iter(|| {
            let mut cursor = Cursor::new(black_box(archive_bytes.as_slice()));
            let state = load_archive(&mut cursor, "bench.dar", &locale).unwrap();
            black_box(state);
        })
    });
}

criterion_group!(benches, bench_load_archive);
criterion_main!(benches);
