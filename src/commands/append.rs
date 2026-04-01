use crate::archive_builder::ArchiveBuilder;
use crate::encryption::resolve_encryption_passphrase;
use crate::extractor::try_decrypt_bytes;
use crate::i18n::Locale;
use crate::pipeline::PipelineConfig;
use crate::reader::{ArchiveState, EncryptedEntryProbe, load_archive};
use crate::walker::scan_files;
use super::shared::{format_size, prepare_files_parallel, print_dry_run_prepared, print_verbose_outcome};
use clap::ArgMatches;
use eyre::{Context, Result, eyre};
use rust_i18n::t;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Instant;

pub fn call(matches: &ArgMatches, locale: &Locale) -> Result<()> {
    let file = matches.get_one::<String>("file").ok_or_else(|| {
        eyre!(t!(
            "cli.common.errors.file_required",
            locale = locale.as_str()
        ))
    })?;

    if !Path::new(file).exists() {
        return Err(eyre!(t!(
            "cli.append.errors.append_file_missing",
            locale = locale.as_str(),
            file = file
        )));
    }

    let verbose = matches.get_flag("verbose");
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
                "cli.append.messages.dry_run_header",
                locale = locale.as_str(),
                file = file
            )
        );
        for p in &prepared {
            print_dry_run_prepared(p, locale.as_str());
        }
        println!(
            "{}",
            t!("cli.append.messages.dry_run_footer", locale = locale.as_str())
        );
        return Ok(());
    }

    println!(
        "{}",
        t!(
            "cli.append.messages.appending_archive",
            locale = locale.as_str(),
            file = file
        )
    );

    let mut file_handle = OpenOptions::new()
        .read(true)
        .write(true)
        .open(file)
        .wrap_err(
            t!(
                "cli.append.errors.append_open_failed",
                locale = locale.as_str(),
                file = file
            )
            .to_string(),
        )?;

    let existing_archive = load_archive(&mut file_handle, file, locale)?;

    ensure_encryption_mode(
        existing_archive.encryption_mode,
        config.encryption_passphrase.is_some(),
        locale,
    )?;

    if let Some(true) = existing_archive.encryption_mode {
        let passphrase = config.encryption_passphrase.as_deref().ok_or_else(|| {
            eyre!(t!(
                "cli.append.errors.append_requires_encryption",
                locale = locale.as_str()
            ))
        })?;
        let probe = existing_archive.encryption_probe.as_ref().ok_or_else(|| {
            eyre!(t!(
                "cli.append.errors.append_encryption_probe_missing",
                locale = locale.as_str()
            ))
        })?;
        verify_passphrase_matches(&mut file_handle, probe, passphrase, file, locale)?;
    }

    let ArchiveState {
        entries,
        index_offset,
        ..
    } = existing_archive;

    file_handle.set_len(index_offset).wrap_err(
        t!(
            "cli.append.errors.append_truncate_failed",
            locale = locale.as_str(),
            file = file
        )
        .to_string(),
    )?;
    file_handle.seek(SeekFrom::Start(index_offset)).wrap_err(
        t!(
            "cli.append.errors.append_seek_failed",
            locale = locale.as_str(),
            file = file
        )
        .to_string(),
    )?;

    let mut builder = ArchiveBuilder::with_config(BufWriter::new(file_handle), config);
    builder.import_existing_entries(entries);

    let start = Instant::now();
    let mut outcomes = Vec::new();

    for prepared_file in prepared {
        let outcome = builder.commit_prepared(prepared_file)?;
        if verbose {
            print_verbose_outcome(&outcome, locale.as_str());
        }
        outcomes.push(outcome);
    }

    builder.build()?;

    let elapsed = start.elapsed();
    let elapsed_str = format!("{:.2}s", elapsed.as_secs_f64());
    let count = outcomes.len();
    let total_original: u64 = outcomes.iter().map(|o| o.original_size).sum();
    let total_stored: u64 = outcomes
        .iter()
        .filter(|o| !o.is_dedup)
        .map(|o| o.stored_size)
        .sum();
    let ratio = if total_original > 0 {
        format!("{:.1}", total_stored as f64 / total_original as f64 * 100.0)
    } else {
        "100.0".to_string()
    };
    println!(
        "{}",
        t!(
            "cli.append.messages.summary",
            locale = locale.as_str(),
            count = count.to_string().as_str(),
            original = format_size(total_original).as_str(),
            stored = format_size(total_stored).as_str(),
            ratio = ratio.as_str(),
            elapsed = elapsed_str.as_str(),
        )
    );

    Ok(())
}

fn ensure_encryption_mode(
    existing_mode: Option<bool>,
    encrypting_now: bool,
    locale: &Locale,
) -> Result<()> {
    match (existing_mode, encrypting_now) {
        (Some(true), false) => Err(eyre!(t!(
            "cli.append.errors.append_requires_encryption",
            locale = locale.as_str()
        ))),
        (Some(false), true) => Err(eyre!(t!(
            "cli.append.errors.append_disallows_encryption",
            locale = locale.as_str()
        ))),
        _ => Ok(()),
    }
}

fn verify_passphrase_matches(
    file: &mut File,
    probe: &EncryptedEntryProbe,
    passphrase: &str,
    file_path: &str,
    locale: &Locale,
) -> Result<()> {
    file.seek(SeekFrom::Start(probe.offset)).wrap_err(
        t!(
            "cli.append.errors.append_seek_failed",
            locale = locale.as_str(),
            file = file_path
        )
        .to_string(),
    )?;

    let mut data = vec![0u8; probe.size as usize];
    file.read_exact(&mut data).wrap_err(
        t!(
            "cli.append.errors.append_read_failed",
            locale = locale.as_str(),
            file = file_path
        )
        .to_string(),
    )?;

    try_decrypt_bytes(&data, &probe.checksum, passphrase).ok_or_else(|| {
        eyre!(t!(
            "cli.append.errors.append_passphrase_invalid",
            locale = locale.as_str()
        ))
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_encryption_mode, verify_passphrase_matches};
    use crate::i18n::Locale;
    use crate::reader::load_archive;
    use crate::test_utils::build_archive;
    use std::fs::OpenOptions;

    #[test]
    fn test_ensure_encryption_mode_requires_passphrase_when_expected() {
        let locale = Locale::new("en");
        let result = ensure_encryption_mode(Some(true), false, &locale);
        assert!(result.is_err());
    }

    #[test]
    fn test_ensure_encryption_mode_disallows_new_encryption_when_plain() {
        let locale = Locale::new("en");
        let result = ensure_encryption_mode(Some(false), true, &locale);
        assert!(result.is_err());
    }

    #[test]
    fn test_ensure_encryption_mode_allows_matching_state() {
        let locale = Locale::new("en");
        assert!(ensure_encryption_mode(Some(true), true, &locale).is_ok());
        assert!(ensure_encryption_mode(Some(false), false, &locale).is_ok());
        assert!(ensure_encryption_mode(None, true, &locale).is_ok());
        assert!(ensure_encryption_mode(None, false, &locale).is_ok());
    }

    #[test]
    fn test_verify_passphrase_matches_accepts_correct_passphrase() {
        let locale = Locale::new("en");
        let dir = tempfile::tempdir().unwrap();
        let archive_path = build_archive(
            &dir,
            "enc.dar",
            &[("file.txt", b"secret data")],
            Some("secret"),
        );

        let mut archive_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&archive_path)
            .unwrap();
        let state =
            load_archive(&mut archive_file, archive_path.to_str().unwrap(), &locale).unwrap();
        let probe = state.encryption_probe.unwrap();

        verify_passphrase_matches(
            &mut archive_file,
            &probe,
            "secret",
            archive_path.to_str().unwrap(),
            &locale,
        )
        .unwrap();
    }

    #[test]
    fn test_verify_passphrase_matches_rejects_invalid_passphrase() {
        let locale = Locale::new("en");
        let dir = tempfile::tempdir().unwrap();
        let archive_path = build_archive(
            &dir,
            "enc_fail.dar",
            &[("file.txt", b"secret data")],
            Some("secret"),
        );

        let mut archive_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&archive_path)
            .unwrap();
        let state =
            load_archive(&mut archive_file, archive_path.to_str().unwrap(), &locale).unwrap();
        let probe = state.encryption_probe.unwrap();

        assert!(
            verify_passphrase_matches(
                &mut archive_file,
                &probe,
                "wrong",
                archive_path.to_str().unwrap(),
                &locale,
            )
            .is_err()
        );
    }
}
