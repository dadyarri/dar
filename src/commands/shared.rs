use crate::archive_builder::{FileAddOutcome, PreparedFile, prepare_file_from_disk};
use crate::models::archive::CompressionMethod;
use crate::pipeline::{CompressionPipeline, PipelineConfig};
use crate::walker::ScannedFile;
use eyre::Result;
use rayon::prelude::*;
use rust_i18n::t;
use std::borrow::Cow;

/// Prepare all files in parallel: read + checksum + compress.
///
/// Shared by both `create` and `append` commands.
pub fn prepare_files_parallel(
    file_entries: &[ScannedFile],
    config: &PipelineConfig,
) -> Result<Vec<PreparedFile>> {
    let pipeline = CompressionPipeline::new(config.clone());
    file_entries
        .par_iter()
        .map(|entry| prepare_file_from_disk(&pipeline, &entry.source_path, &entry.archive_path))
        .collect()
}

#[must_use]
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[must_use]
pub fn compression_method_label(method: CompressionMethod, locale: &str) -> Cow<'static, str> {
    match method {
        CompressionMethod::None => t!("cli.common.methods.stored", locale = locale),
        CompressionMethod::Brotli => t!("cli.common.methods.brotli", locale = locale),
        CompressionMethod::Zstandard => t!("cli.common.methods.zstd", locale = locale),
        CompressionMethod::Lzma => t!("cli.common.methods.lzma", locale = locale),
    }
}

/// Compute the stored-vs-original ratio as a formatted percentage string.
///
/// Returns `"100.0"` when `total_original` is zero to avoid division by zero.
#[must_use]
pub fn compute_ratio(total_original: u64, total_stored: u64) -> String {
    if total_original > 0 {
        format!("{:.1}", total_stored as f64 / total_original as f64 * 100.0)
    } else {
        "100.0".to_string()
    }
}

pub fn print_summary(
    count: usize,
    total_original: u64,
    total_stored: u64,
    elapsed_secs: f64,
    locale: &str,
) {
    let ratio = compute_ratio(total_original, total_stored);
    let elapsed_str = format!("{:.2}s", elapsed_secs);
    println!(
        "{}",
        t!(
            "cli.common.messages.summary",
            locale = locale,
            count = count.to_string().as_str(),
            original = format_size(total_original).as_str(),
            stored = format_size(total_stored).as_str(),
            ratio = ratio.as_str(),
            elapsed = elapsed_str.as_str(),
        )
    );
}

pub fn print_verbose_outcome(outcome: &FileAddOutcome, locale: &str) {
    let orig = format_size(outcome.original_size);

    if outcome.is_dedup {
        let dedup_label = t!("cli.common.methods.dedup", locale = locale);
        println!(
            "  {:<60} {:>10}  [{}]",
            outcome.archive_path, orig, dedup_label
        );
        return;
    }

    match outcome.compression_method {
        CompressionMethod::None => {
            let stored_label = compression_method_label(CompressionMethod::None, locale);
            println!(
                "  {:<60} {:>10}  [{}]",
                outcome.archive_path, orig, stored_label
            );
        }
        method => {
            let ratio = if outcome.original_size > 0 {
                outcome.stored_size as f64 / outcome.original_size as f64 * 100.0
            } else {
                100.0
            };
            let stored = format_size(outcome.stored_size);
            println!(
                "  {:<60} {:>10} → {:>10}  [{}, {:.0}%]",
                outcome.archive_path,
                orig,
                stored,
                compression_method_label(method, locale),
                ratio,
            );
        }
    }
}

pub fn print_dry_run_prepared(prepared: &PreparedFile, locale: &str) {
    let orig = format_size(prepared.pipeline_result.original_size);

    match prepared.pipeline_result.compression_method {
        CompressionMethod::None => {
            let stored_label = compression_method_label(CompressionMethod::None, locale);
            println!(
                "  {:<60} {:>10}  [{}]",
                prepared.archive_path, orig, stored_label
            );
        }
        method => {
            let stored_size = if prepared.pipeline_result.compressed_content.is_some() {
                prepared.pipeline_result.compressed_size
            } else {
                prepared.pipeline_result.original_size
            };
            let ratio = if prepared.pipeline_result.original_size > 0 {
                stored_size as f64 / prepared.pipeline_result.original_size as f64 * 100.0
            } else {
                100.0
            };
            let stored = format_size(stored_size);
            println!(
                "  {:<60} {:>10} → {:>10}  [{}, {:.0}%]",
                prepared.archive_path,
                orig,
                stored,
                compression_method_label(method, locale),
                ratio,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{compute_ratio, format_size};
    use crate::archive_builder::FileAddOutcome;
    use crate::models::archive::CompressionMethod;

    #[test]
    fn test_compute_ratio_compressed() {
        assert_eq!(compute_ratio(1_000, 500), "50.0");
    }

    #[test]
    fn test_compute_ratio_no_compression() {
        assert_eq!(compute_ratio(1_000, 1_000), "100.0");
    }

    #[test]
    fn test_compute_ratio_zero_original() {
        assert_eq!(compute_ratio(0, 0), "100.0");
    }

    #[test]
    fn test_dedup_excluded_from_total_stored() {
        let outcomes = [
            FileAddOutcome {
                archive_path: "a.txt".to_string(),
                original_size: 1_000,
                stored_size: 400,
                compression_method: CompressionMethod::Zstandard,
                is_dedup: false,
            },
            FileAddOutcome {
                archive_path: "b.txt".to_string(),
                original_size: 800,
                stored_size: 800,
                compression_method: CompressionMethod::None,
                is_dedup: true,
            },
        ];

        let mut count = 0usize;
        let mut total_original = 0u64;
        let mut total_stored = 0u64;
        for o in &outcomes {
            count += 1;
            total_original += o.original_size;
            if !o.is_dedup {
                total_stored += o.stored_size;
            }
        }

        assert_eq!(count, 2);
        assert_eq!(total_original, 1_800);
        assert_eq!(
            total_stored, 400,
            "dedup entry must not contribute to total_stored"
        );
        assert_eq!(compute_ratio(total_original, total_stored), "22.2");
    }

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(512), "512 B");
    }

    #[test]
    fn test_format_size_kilobytes() {
        assert_eq!(format_size(2_048), "2.00 KB");
    }

    #[test]
    fn test_format_size_megabytes() {
        assert_eq!(format_size(1_048_576), "1.00 MB");
    }

    #[test]
    fn test_compression_method_label_english() {
        use super::compression_method_label;
        assert_eq!(
            compression_method_label(CompressionMethod::None, "en"),
            "stored"
        );
        assert_eq!(
            compression_method_label(CompressionMethod::Brotli, "en"),
            "brotli"
        );
        assert_eq!(
            compression_method_label(CompressionMethod::Zstandard, "en"),
            "zstd"
        );
        assert_eq!(
            compression_method_label(CompressionMethod::Lzma, "en"),
            "lzma"
        );
    }

    #[test]
    fn test_compression_method_label_russian() {
        use super::compression_method_label;
        assert_eq!(
            compression_method_label(CompressionMethod::None, "ru"),
            "без сжатия"
        );
        assert_eq!(
            compression_method_label(CompressionMethod::Brotli, "ru"),
            "brotli"
        );
        assert_eq!(
            compression_method_label(CompressionMethod::Zstandard, "ru"),
            "zstd"
        );
        assert_eq!(
            compression_method_label(CompressionMethod::Lzma, "ru"),
            "lzma"
        );
    }
}
