use super::shared::{
    prepare_files_parallel, print_dry_run_prepared, print_summary, print_verbose_outcome,
};
use crate::archive_builder::ArchiveBuilder;
use crate::encryption::resolve_encryption_passphrase;
use crate::i18n::Locale;
use crate::pipeline::PipelineConfig;
use crate::walker::scan_files;
use clap::ArgMatches;
use eyre::{eyre, Context, Result};
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
    let content = matches.get_many::<String>("content").ok_or_else(|| {
        eyre!(t!(
            "cli.common.errors.content_required",
            locale = locale.as_str()
        ))
    })?;

    let config = PipelineConfig {
        compress_images,
        encryption_passphrase,
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
