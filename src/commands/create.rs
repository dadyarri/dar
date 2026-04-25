use super::shared::{
    prepare_files_parallel, print_dry_run_prepared, print_summary, print_verbose_outcome,
};
use crate::archive_builder::ArchiveBuilder;
use crate::encryption::resolve_encryption_passphrase;
use crate::format_version::FormatVersion;
use crate::i18n::Locale;
use crate::index_writer::{IndexWriter, index_path_for_archive};
use crate::pipeline::PipelineConfig;
use crate::walker::scan_files;
use clap::ArgMatches;
use eyre::{Context, Result, eyre};
use rust_i18n::t;
use std::fs;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::Instant;

pub fn call(matches: &ArgMatches, locale: &Locale) -> Result<()> {
    let file = matches.get_one::<String>("file").ok_or_else(|| {
        eyre!(t!(
            "cli.common.errors.file_required",
            locale = locale.as_str()
        ))
    })?;

    let verbose = matches.get_flag("verbose");
    let overwrite = matches.get_flag("overwrite");
    let dry_run = matches.get_flag("dry-run");
    let compress_images = matches.get_flag("compress-images");
    let encryption_passphrase = resolve_encryption_passphrase(matches, locale)?;
    let chunked_encryption = matches.get_flag("chunked-encryption");
    if chunked_encryption && encryption_passphrase.is_none() {
        return Err(eyre!(t!(
            "cli.common.errors.chunked_encryption_requires_encrypt",
            locale = locale.as_str()
        )));
    }
    let mut format_version = match matches
        .get_one::<String>("format-version")
        .map(String::as_str)
    {
        Some("6") => FormatVersion::V6,
        _ => FormatVersion::V5,
    };
    let split_size = matches
        .get_one::<String>("split-size")
        .map(|s| parse_split_size(s, locale))
        .transpose()?;
    let auto_v6_reason = if split_size.is_some() {
        Some("--split-size")
    } else if chunked_encryption {
        Some("--chunked-encryption")
    } else {
        None
    };
    if auto_v6_reason.is_some() && format_version != FormatVersion::V6 {
        format_version = FormatVersion::V6;
        println!(
            "{}",
            t!(
                "cli.common.flags.format_version_auto",
                locale = locale.as_str(),
                v = 6,
                reason = auto_v6_reason.unwrap_or("--format-version")
            )
        );
    }
    let content = matches.get_many::<String>("content").ok_or_else(|| {
        eyre!(t!(
            "cli.common.errors.content_required",
            locale = locale.as_str()
        ))
    })?;

    let config = PipelineConfig {
        compress_images,
        encryption_passphrase,
        chunked_encryption,
    };

    // Collect all files first so we can process them in parallel.
    let file_entries = scan_files(content, locale)?;

    // ── Parallel phase: read + checksum + compress ──────────────────────────
    let prepared = prepare_files_parallel(&file_entries, &config)?;

    // ── Dry-run short-circuit ────────────────────────────────────────────────
    if dry_run {
        println!(
            "{}",
            t!(
                "cli.create.messages.dry_run_header",
                locale = locale.as_str(),
                file = file
            )
        );
        for p in &prepared {
            print_dry_run_prepared(p, locale.as_str());
        }
        println!(
            "{}",
            t!(
                "cli.create.messages.dry_run_footer",
                locale = locale.as_str()
            )
        );
        return Ok(());
    }

    let archive_path = Path::new(file);
    let first_output_path = if split_size.is_some() {
        std::path::PathBuf::from(format!("{}.001", archive_path.display()))
    } else {
        archive_path.to_path_buf()
    };

    if first_output_path.exists() && !overwrite {
        return Err(eyre!(t!(
            "cli.create.errors.file_exists",
            locale = locale.as_str(),
            file = first_output_path.display().to_string()
        )));
    }

    if first_output_path.exists() && overwrite {
        fs::remove_file(&first_output_path).wrap_err(
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

    // ── Serial phase: dedup-check + write to archive ─────────────────────────
    let file_handle = File::create(&first_output_path).wrap_err(
        t!(
            "cli.create.errors.create_file_failed",
            locale = locale.as_str()
        )
        .to_string(),
    )?;
    let writer = BufWriter::new(file_handle);

    let mut builder = ArchiveBuilder::with_version(writer, config, format_version);
    builder.set_archive_output_path(first_output_path.clone());
    if let Some(split_threshold) = split_size {
        builder.enable_split(file, split_threshold, |path| {
            Ok(BufWriter::new(File::create(path)?))
        });
    }
    builder.write_header()?;

    // For v6 archives, attach an external index writer so the `.dari` file is
    // written alongside the `.dar` file when `build()` is called.
    if format_version == FormatVersion::V6 {
        let idx_path = index_path_for_archive(Path::new(file));
        let iw = IndexWriter::new(&idx_path, builder.header_timestamp(), 1).wrap_err(
            t!(
                "cli.create.errors.index_write_failed",
                locale = locale.as_str()
            )
            .to_string(),
        )?;
        builder.set_index_writer(iw);
    }

    let start = Instant::now();
    let mut count = 0usize;
    let mut total_original = 0u64;
    let mut total_stored = 0u64;

    for prepared_file in prepared {
        let outcome = builder.commit_prepared(prepared_file)?;

        if verbose {
            print_verbose_outcome(&outcome, locale.as_str());
        }
        count += 1;
        total_original += outcome.original_size;
        if !outcome.is_dedup {
            total_stored += outcome.stored_size;
        }
    }

    builder.build()?;

    let elapsed = start.elapsed();
    print_summary(
        count,
        total_original,
        total_stored,
        elapsed.as_secs_f64(),
        locale.as_str(),
    );

    Ok(())
}

fn parse_split_size(value: &str, locale: &Locale) -> Result<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(eyre!(t!(
            "cli.common.errors.split_size_empty",
            locale = locale.as_str()
        )));
    }
    let split_at = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let (digits, suffix) = trimmed.split_at(split_at);
    let base: u64 = digits.parse().wrap_err(
        t!("cli.common.errors.split_size_invalid", locale = locale.as_str()).to_string(),
    )?;
    let multiplier = match suffix.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" => 1024,
        "m" | "mb" => 1024 * 1024,
        "g" | "gb" => 1024 * 1024 * 1024,
        other => {
            return Err(eyre!(t!(
                "cli.common.errors.split_size_suffix_invalid",
                locale = locale.as_str(),
                suffix = other
            )));
        }
    };
    Ok(base.saturating_mul(multiplier))
}

#[cfg(test)]
mod tests {
    use super::parse_split_size;
    use crate::archive_builder::ArchiveBuilder;
    use crate::pipeline::PipelineConfig;
    use crate::utils::{get_unix_timestamp, read_bytes_as, read_string};
    use std::io::Cursor;

    #[test]
    fn test_archive_header_writing() {
        // Arrange
        let buffer = Cursor::new(Vec::new());

        // Act
        let mut builder = ArchiveBuilder::with_config(buffer, PipelineConfig::default());
        builder.write_header().unwrap();

        // Assert
        let data = builder.into_inner().into_inner();
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

    #[test]
    fn test_parse_split_size_supports_suffixes() {
        let locale = crate::i18n::Locale::new("en");
        assert_eq!(parse_split_size("512", &locale).unwrap(), 512);
        assert_eq!(parse_split_size("2K", &locale).unwrap(), 2 * 1024);
        assert_eq!(parse_split_size("3m", &locale).unwrap(), 3 * 1024 * 1024);
        assert_eq!(parse_split_size("1GB", &locale).unwrap(), 1024 * 1024 * 1024);
    }

    #[test]
    fn test_parse_split_size_rejects_unknown_suffix() {
        let locale = crate::i18n::Locale::new("en");
        assert!(parse_split_size("7T", &locale).is_err());
    }
}
