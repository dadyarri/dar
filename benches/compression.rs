/// Benchmarks for [`dari::pipeline::CompressionPipeline::process_file`].
///
/// Exercises Brotli, ZStd, and LZMA on a 1 MiB synthetic plaintext payload.
///
/// `iter_batched` is used so that the payload clone (needed because
/// `process_file` takes ownership) is **excluded** from the measured time and
/// only the compression step itself is captured.
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use dari::pipeline::{CompressionPipeline, PipelineConfig};
use std::hint::black_box;
use std::path::Path;


rust_i18n::i18n!("locales", fallback = "en");

/// 1 MiB of highly-compressible ASCII text.
fn synthetic_payload() -> Vec<u8> {
    let pattern: &[u8] = b"The quick brown fox jumps over the lazy dog. ABCDEFGHIJKLMNOPQRSTUV";
    let repeats = (1024 * 1024 / pattern.len()) + 1;
    pattern.repeat(repeats)[..1024 * 1024].to_vec()
}

fn config() -> PipelineConfig {
    PipelineConfig {
        compress_images: false,
        encryption_passphrase: None,
        chunked_encryption: false,
        preserve_xattrs: false,
    }
}

fn bench_brotli(c: &mut Criterion) {
    let payload = synthetic_payload();
    // `.html` routes to BrotliCompressor.
    let path = Path::new("payload.html");
    let pipeline = CompressionPipeline::new(config());

    c.bench_function("pipeline/brotli 1MiB", |b| {
        b.iter_batched(
            || payload.clone(),
            |data| {
                let result = pipeline.process_file(black_box(path), data).unwrap();
                black_box(result);
            },
            BatchSize::LargeInput,
        )
    });
}

fn bench_zstd(c: &mut Criterion) {
    let payload = synthetic_payload();
    // `.rs` routes to ZStandardCompressor.
    let path = Path::new("payload.rs");
    let pipeline = CompressionPipeline::new(config());

    c.bench_function("pipeline/zstd 1MiB", |b| {
        b.iter_batched(
            || payload.clone(),
            |data| {
                let result = pipeline.process_file(black_box(path), data).unwrap();
                black_box(result);
            },
            BatchSize::LargeInput,
        )
    });
}

fn bench_lzma(c: &mut Criterion) {
    let payload = synthetic_payload();
    // `.tex` routes to LzmaCompressor.
    let path = Path::new("payload.tex");
    let pipeline = CompressionPipeline::new(config());

    c.bench_function("pipeline/lzma 1MiB", |b| {
        b.iter_batched(
            || payload.clone(),
            |data| {
                let result = pipeline.process_file(black_box(path), data).unwrap();
                black_box(result);
            },
            BatchSize::LargeInput,
        )
    });
}

criterion_group!(benches, bench_brotli, bench_zstd, bench_lzma);
criterion_main!(benches);
