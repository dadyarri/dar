use crate::archive_builder::{ArchiveBuilder, ConflictMode, make_renamed_path};
use crate::encryption::resolve_encryption_passphrase;
use crate::extractor::try_decrypt_bytes;
use crate::i18n::Locale;
use crate::pipeline::PipelineConfig;
use crate::reader::{ArchiveState, EncryptedEntryProbe, load_archive};
use crate::walker::scan_files;
use super::shared::{prepare_files_parallel, print_dry_run_prepared, print_summary, print_verbose_outcome};
use clap::ArgMatches;
use eyre::{Context, Result, eyre};
use rust_i18n::t;
use std::collections::HashSet;
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

    let conflict_mode = parse_conflict_mode(
        matches.get_one::<String>("on-conflict").map(String::as_str).unwrap_or("error"),
        locale,
    )?;

    let config = PipelineConfig {
        compress_images,
        encryption_passphrase,
    };

    // Collect all files first so we can process them in parallel.
    let file_entries = scan_files(content, locale)?;

    // ── Parallel phase: read + checksum + compress ──────────────────────────
    let prepared = prepare_files_parallel(&file_entries, &config)?;

    // ── Open archive (needed for both dry-run conflict reporting and actual append) ──
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

        // Simulate conflict resolution to give accurate dry-run output.
        let mut dry_run_path_set: HashSet<String> =
            entries.iter().map(|e| e.path.clone()).collect();

        for p in &prepared {
            let conflict = dry_run_path_set.contains(&p.archive_path);
            if conflict {
                match conflict_mode {
                    ConflictMode::Error => {
                        print_dry_run_prepared(p, locale.as_str());
                        println!(
                            "    {}",
                            t!(
                                "cli.append.messages.dry_run_conflict_error",
                                locale = locale.as_str()
                            )
                        );
                    }
                    ConflictMode::Rename => {
                        let new_path = make_renamed_path(&p.archive_path, &dry_run_path_set);
                        dry_run_path_set.insert(new_path.clone());
                        print_dry_run_prepared(p, locale.as_str());
                        println!(
                            "    {}",
                            t!(
                                "cli.append.messages.dry_run_conflict_rename",
                                locale = locale.as_str(),
                                new_path = new_path
                            )
                        );
                    }
                    ConflictMode::Overwrite => {
                        dry_run_path_set.insert(p.archive_path.clone());
                        print_dry_run_prepared(p, locale.as_str());
                        println!(
                            "    {}",
                            t!(
                                "cli.append.messages.dry_run_conflict_overwrite",
                                locale = locale.as_str()
                            )
                        );
                    }
                }
            } else {
                dry_run_path_set.insert(p.archive_path.clone());
                print_dry_run_prepared(p, locale.as_str());
            }
        }

        println!(
            "{}",
            t!("cli.append.messages.dry_run_footer", locale = locale.as_str())
        );
        return Ok(());
    }

    // ── Pre-flight conflict check for error mode ─────────────────────────────
    // Must happen before we truncate the archive so it is never modified on error.
    if conflict_mode == ConflictMode::Error {
        let existing_path_set: HashSet<&str> =
            entries.iter().map(|e| e.path.as_str()).collect();
        let mut seen: HashSet<&str> = HashSet::new();
        let mut conflicts: Vec<&str> = Vec::new();
        for p in &prepared {
            if existing_path_set.contains(p.archive_path.as_str())
                || seen.contains(p.archive_path.as_str())
            {
                conflicts.push(p.archive_path.as_str());
            }
            seen.insert(p.archive_path.as_str());
        }
        if !conflicts.is_empty() {
            return Err(eyre!(t!(
                "cli.append.errors.append_conflict_error",
                locale = locale.as_str(),
                paths = conflicts.join("\n  ")
            )));
        }
    }

    println!(
        "{}",
        t!(
            "cli.append.messages.appending_archive",
            locale = locale.as_str(),
            file = file
        )
    );

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
    builder.set_conflict_mode(conflict_mode);
    builder.import_existing_entries(entries);

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
    print_summary(count, total_original, total_stored, elapsed.as_secs_f64(), locale.as_str());

    Ok(())
}

fn parse_conflict_mode(value: &str, locale: &Locale) -> Result<ConflictMode> {
    match value {
        "error" => Ok(ConflictMode::Error),
        "rename" => Ok(ConflictMode::Rename),
        "overwrite" => Ok(ConflictMode::Overwrite),
        other => Err(eyre!(t!(
            "cli.append.errors.append_invalid_conflict_mode",
            locale = locale.as_str(),
            value = other
        ))),
    }
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
    use super::{ensure_encryption_mode, parse_conflict_mode, verify_passphrase_matches};
    use crate::i18n::Locale;
    use crate::reader::load_archive;
    use crate::test_utils::build_archive;
    use clap::{Arg, ArgAction, Command};
    use std::fs::OpenOptions;
    use tempfile::tempdir;

    fn make_matches(args: &[&str]) -> clap::ArgMatches {
        let cmd = Command::new("dari").subcommand(
            Command::new("append")
                .arg(
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .required(true),
                )
                .arg(
                    Arg::new("compress-images")
                        .long("compress-images")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("encrypt")
                        .long("encrypt")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("encrypt-passphrase")
                        .long("encrypt-passphrase")
                        .action(ArgAction::Set),
                )
                .arg(
                    Arg::new("verbose")
                        .short('v')
                        .long("verbose")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("dry-run")
                        .long("dry-run")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("on-conflict")
                        .long("on-conflict")
                        .action(ArgAction::Set)
                        .default_value("error"),
                )
                .arg(Arg::new("content").num_args(0..).action(ArgAction::Append)),
        );
        let mut full_args = vec!["dari", "append"];
        full_args.extend_from_slice(args);
        let matches = cmd.get_matches_from(full_args);
        matches.subcommand_matches("append").unwrap().clone()
    }

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
    fn test_parse_conflict_mode_valid_values() {
        use crate::archive_builder::ConflictMode;
        let locale = Locale::new("en");
        assert_eq!(parse_conflict_mode("error", &locale).unwrap(), ConflictMode::Error);
        assert_eq!(parse_conflict_mode("rename", &locale).unwrap(), ConflictMode::Rename);
        assert_eq!(parse_conflict_mode("overwrite", &locale).unwrap(), ConflictMode::Overwrite);
    }

    #[test]
    fn test_parse_conflict_mode_invalid_value_returns_error() {
        let locale = Locale::new("en");
        assert!(parse_conflict_mode("invalid", &locale).is_err());
    }

    #[test]
    fn test_conflict_error_mode_aborts_when_path_conflicts() {
        let dir = tempdir().unwrap();
        // archive already contains "file.txt"
        let archive = build_archive(&dir, "test.dar", &[("file.txt", b"original")], None);

        // create a new file that would produce archive path "file.txt"
        let new_file = dir.path().join("file.txt");
        std::fs::write(&new_file, b"new content").unwrap();

        let sub_matches = make_matches(&[
            "-f",
            archive.to_str().unwrap(),
            "--on-conflict",
            "error",
            new_file.to_str().unwrap(),
        ]);

        let locale = Locale::new("en");
        let result = super::call(&sub_matches, &locale);
        assert!(result.is_err(), "error mode should fail on path conflict");
    }

    #[test]
    fn test_conflict_error_mode_leaves_archive_unmodified() {
        let dir = tempdir().unwrap();
        let archive = build_archive(&dir, "test.dar", &[("file.txt", b"original")], None);

        // Capture the original file size before the attempt
        let original_size = std::fs::metadata(&archive).unwrap().len();

        let new_file = dir.path().join("file.txt");
        std::fs::write(&new_file, b"new content").unwrap();

        let sub_matches = make_matches(&[
            "-f",
            archive.to_str().unwrap(),
            "--on-conflict",
            "error",
            new_file.to_str().unwrap(),
        ]);

        let locale = Locale::new("en");
        let _ = super::call(&sub_matches, &locale);

        let size_after = std::fs::metadata(&archive).unwrap().len();
        assert_eq!(
            original_size, size_after,
            "error mode must not modify the archive on conflict"
        );
    }

    #[test]
    fn test_conflict_rename_mode_creates_renamed_entry() {
        let dir = tempdir().unwrap();
        let archive = build_archive(&dir, "rename.dar", &[("file.txt", b"original")], None);

        let new_file = dir.path().join("file.txt");
        std::fs::write(&new_file, b"new content").unwrap();

        let sub_matches = make_matches(&[
            "-f",
            archive.to_str().unwrap(),
            "--on-conflict",
            "rename",
            new_file.to_str().unwrap(),
        ]);

        let locale = Locale::new("en");
        super::call(&sub_matches, &locale).unwrap();

        // After appending, the archive should contain both "file.txt" and "file-1.txt"
        let mut fh = OpenOptions::new().read(true).write(true).open(&archive).unwrap();
        let state = load_archive(&mut fh, archive.to_str().unwrap(), &locale).unwrap();
        let paths: Vec<_> = state.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"file.txt"), "original entry must be kept");
        assert!(paths.contains(&"file-1.txt"), "renamed entry must exist");
    }

    #[test]
    fn test_conflict_overwrite_mode_replaces_existing_entry() {
        let dir = tempdir().unwrap();
        let archive = build_archive(&dir, "overwrite.dar", &[("file.txt", b"original")], None);

        let new_file = dir.path().join("file.txt");
        std::fs::write(&new_file, b"replaced content").unwrap();

        let sub_matches = make_matches(&[
            "-f",
            archive.to_str().unwrap(),
            "--on-conflict",
            "overwrite",
            new_file.to_str().unwrap(),
        ]);

        let locale = Locale::new("en");
        super::call(&sub_matches, &locale).unwrap();

        // Archive should have exactly one entry named "file.txt"
        let mut fh = OpenOptions::new().read(true).write(true).open(&archive).unwrap();
        let state = load_archive(&mut fh, archive.to_str().unwrap(), &locale).unwrap();
        let count = state.entries.iter().filter(|e| e.path == "file.txt").count();
        assert_eq!(count, 1, "overwrite mode must leave exactly one entry with the path");
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
