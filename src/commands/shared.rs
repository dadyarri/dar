use crate::archive_builder::{FileAddOutcome, PreparedFile, prepare_file_from_disk};
use crate::models::archive::CompressionMethod;
use crate::pipeline::{CompressionPipeline, PipelineConfig};
use crate::walker::ScannedFile;
use eyre::Result;
use rayon::prelude::*;

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

pub fn compression_method_label(method: CompressionMethod) -> &'static str {
    match method {
        CompressionMethod::None => "stored",
        CompressionMethod::Brotli => "brotli",
        CompressionMethod::Zstandard => "zstd",
        CompressionMethod::Lzma => "lzma",
        CompressionMethod::LeptonJpeg => "lepton",
    }
}

pub fn print_verbose_outcome(outcome: &FileAddOutcome) {
    let orig = format_size(outcome.original_size);

    if outcome.is_dedup {
        println!("  {:<60} {:>10}  [dedup]", outcome.archive_path, orig);
        return;
    }

    match outcome.compression_method {
        CompressionMethod::None => {
            println!("  {:<60} {:>10}  [stored]", outcome.archive_path, orig);
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
                compression_method_label(method),
                ratio,
            );
        }
    }
}

pub fn print_dry_run_prepared(prepared: &PreparedFile) {
    let orig = format_size(prepared.pipeline_result.original_size);

    match prepared.pipeline_result.compression_method {
        CompressionMethod::None => {
            println!("  {:<60} {:>10}  [stored]", prepared.archive_path, orig);
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
                compression_method_label(method),
                ratio,
            );
        }
    }
}
