use crate::archive_builder::{ArchiveBuilder, FileAddOutcome, prepare_file_from_disk};
use crate::encryption::resolve_encryption_passphrase;
use crate::i18n::Locale;
use crate::models::archive::CompressionMethod;
use crate::pipeline::{CompressionPipeline, PipelineConfig};
use crate::walker::scan_files;
use clap::ArgMatches;
use eyre::{Context, Result, eyre};
use rayon::prelude::*;
use rust_i18n::t;
use std::fs;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

pub fn call(matches: &ArgMatches, locale: &Locale) -> Result<()> {
    let file = matches.get_one::<String>("file").ok_or_else(|| {
        eyre!(t!(
            "cli.common.errors.file_required",
            locale = locale.as_str()
        ))
    })?;

    let verbose = matches.get_flag("verbose");
    let overwrite = matches.get_flag("overwrite");
    let compress_images = matches.get_flag("compress-images");
    let encryption_passphrase = resolve_encryption_passphrase(matches, locale)?;
    let content = matches.get_many::<String>("content").ok_or_else(|| {
        eyre!(t!(
            "cli.common.errors.content_required",
            locale = locale.as_str()
        ))
    })?;

    if Path::new(file).exists() && !overwrite {
        return Err(eyre!(t!(
            "cli.create.errors.file_exists",
            locale = locale.as_str(),
            file = file
        )));
    }

    if Path::new(file).exists() && overwrite {
        fs::remove_file(file).wrap_err(
            t!("cli.create.errors.delete_failed", locale = locale.as_str()).to_string(),
        )?;
    }

    println!(
        "{}",
        t!(
            "cli.create.messages.creating_archive",
            locale = locale.as_str(),
            file = file
        )
    );

    // Collect all files first so we can process them in parallel.
    let file_entries = scan_files(content, locale)?;

    let config = PipelineConfig {
        compress_images,
        encryption_passphrase,
    };

    // Build a shared pipeline used only for the parallel read+compress phase.
    // `CompressionPipeline` is Sync, so `&pipeline` is safe to share across threads.
    let pipeline = CompressionPipeline::new(config.clone());

    // ── Parallel phase: read + checksum + compress ──────────────────────────
    let prepared: Vec<_> = file_entries
        .par_iter()
        .map(|entry| prepare_file_from_disk(&pipeline, &entry.source_path, &entry.archive_path))
        .collect::<Result<_>>()?;

    // ── Serial phase: dedup-check + write to archive ─────────────────────────
    let file_handle = File::create(file).wrap_err(
        t!(
            "cli.create.errors.create_file_failed",
            locale = locale.as_str()
        )
        .to_string(),
    )?;
    let writer = BufWriter::new(file_handle);

    let mut builder = ArchiveBuilder::with_config(writer, config);
    builder.write_header()?;

    for prepared_file in prepared {
        let outcome = builder.commit_prepared(prepared_file)?;

        if verbose {
            print_verbose_outcome(&outcome);
        }
    }

    builder.build()?;

    Ok(())
}

fn format_size(bytes: u64) -> String {
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

fn compression_method_label(method: CompressionMethod) -> &'static str {
    match method {
        CompressionMethod::None => "stored",
        CompressionMethod::Brotli => "brotli",
        CompressionMethod::Zstandard => "zstd",
        CompressionMethod::Lzma => "lzma",
        CompressionMethod::LeptonJpeg => "lepton",
    }
}

fn print_verbose_outcome(outcome: &FileAddOutcome) {
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

#[cfg(test)]
mod tests {
    use crate::archive_builder::ArchiveBuilder;
    use crate::pipeline::PipelineConfig;
    use crate::utils::{get_unix_timestamp, read_bytes_as, read_string};
    use std::io::Cursor;

    #[test]
    fn test_archive_header_writing() {
        // Arrange
        let mut buffer = Cursor::new(Vec::new());

        // Act
        let mut builder = ArchiveBuilder::with_config(&mut buffer, PipelineConfig::default());
        builder.write_header().unwrap();

        // Assert
        let data = buffer.into_inner();
        assert!(!data.is_empty(), "Archive data should not be empty");
        assert!(
            read_string(&data, 0, 4).is_ok(),
            "Failed to read archive signature"
        );
        assert_eq!(
            read_string(&data, 0, 4).unwrap(),
            "DARI".to_string(),
            "Invalid archive signature"
        );
        assert!(
            read_bytes_as::<u8>(&data, 4).is_ok(),
            "Failed to read archive version"
        );
        assert_eq!(
            read_bytes_as::<u8>(&data, 4).unwrap(),
            5,
            "Invalid archive version"
        );
        assert!(
            read_bytes_as::<u64>(&data, 5).is_ok(),
            "Failed to read archive creation timestamp"
        );
        assert!(
            read_bytes_as::<u64>(&data, 5).unwrap() <= get_unix_timestamp().unwrap(),
            "Invalid archive creation timestamp"
        );
    }
}
