use super::shared::{
    prepare_files_parallel, print_dry_run_prepared, print_summary, print_verbose_outcome,
};
use crate::archive_builder::{ArchiveBuilder, ConflictMode, PreparedFile, make_renamed_path};
use crate::encryption::resolve_encryption_passphrase;
use crate::extractor::try_decrypt_bytes;
use crate::format_version::FormatVersion;
use crate::i18n::Locale;
use crate::pipeline::PipelineConfig;
use crate::reader::{ArchiveState, EncryptedEntryProbe, load_archive};
use crate::walker::scan_files_incremental;
use clap::ArgMatches;
use clap::parser::ValueSource;
use eyre::{Context, Result, eyre};
use rust_i18n::t;
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Instant;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

pub fn call(matches: &ArgMatches, locale: &Locale) -> Result<()> {
    let file = matches.get_one::<String>("file").ok_or_else(|| {
        eyre!(t!(
            "cli.common.errors.file_required",
            locale = locale.as_str()
        ))
    })?;

    if !Path::new(file).exists() {
        return Err(eyre!(t!(
            "cli.incremental.errors.archive_missing",
            locale = locale.as_str(),
            file = file
        )));
    }

    let verbose = matches.get_flag("verbose");
    let dry_run = matches.get_flag("dry-run");
    let compress_images = matches.get_flag("compress-images");
    let encryption_passphrase = resolve_encryption_passphrase(matches, locale)?;
    let chunked_encryption = matches.get_flag("chunked-encryption");
    let preserve_xattrs = matches.get_flag("preserve-xattrs");
    if chunked_encryption && encryption_passphrase.is_none() {
        return Err(eyre!(t!(
            "cli.common.errors.chunked_encryption_requires_encrypt",
            locale = locale.as_str()
        )));
    }

    let cli_format_version = match matches
        .get_one::<String>("format-version")
        .map(String::as_str)
    {
        Some("6") => FormatVersion::V6,
        _ => FormatVersion::V5,
    };
    let version_explicitly_set =
        matches.value_source("format-version") == Some(ValueSource::CommandLine);
    let content = matches.get_many::<String>("content").ok_or_else(|| {
        eyre!(t!(
            "cli.common.errors.content_required",
            locale = locale.as_str()
        ))
    })?;
    let conflict_mode = parse_conflict_mode(
        matches
            .get_one::<String>("on-conflict")
            .map(String::as_str)
            .unwrap_or("error"),
        locale,
    )?;

    let config = PipelineConfig {
        compress_images,
        encryption_passphrase,
        chunked_encryption,
        preserve_xattrs,
    };

    let mut file_handle = OpenOptions::new()
        .read(true)
        .write(true)
        .open(file)
        .wrap_err(
            t!(
                "cli.incremental.errors.open_failed",
                locale = locale.as_str(),
                file = file
            )
            .to_string(),
        )?;

    let existing_archive = load_archive(&mut file_handle, file, locale)?;
    let since = match matches.get_one::<String>("since") {
        Some(value) => parse_since_arg(value, locale)?,
        None => existing_archive.header.timestamp,
    };

    let file_entries = scan_files_incremental(content, locale, since)?;
    let prepared = prepare_files_parallel(&file_entries, &config)?;

    if prepared.is_empty() {
        println!(
            "{}",
            t!(
                "cli.incremental.messages.no_changes",
                locale = locale.as_str(),
                file = file
            )
        );
        return Ok(());
    }

    let archive_format_version =
        FormatVersion::try_from(existing_archive.header.version).map_err(eyre::Report::new)?;
    if version_explicitly_set && cli_format_version != archive_format_version {
        return Err(eyre!(t!(
            "cli.append.errors.version_mismatch",
            locale = locale.as_str(),
            found = existing_archive.header.version as u32,
            requested = u8::from(cli_format_version) as u32
        )));
    }
    if chunked_encryption && archive_format_version != FormatVersion::V6 {
        return Err(eyre!(t!(
            "cli.append.errors.append_chunked_requires_v6",
            locale = locale.as_str()
        )));
    }
    if preserve_xattrs && archive_format_version != FormatVersion::V6 {
        return Err(eyre!(t!(
            "cli.append.errors.append_preserve_xattrs_requires_v6",
            locale = locale.as_str()
        )));
    }

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
        header,
        index_offset,
        ..
    } = existing_archive;
    let archive_timestamp = header.timestamp;

    if dry_run {
        run_dry_run(file, &entries, &prepared, conflict_mode, locale);
        return Ok(());
    }

    preflight_conflict_check(&entries, &prepared, conflict_mode, locale)?;

    println!(
        "{}",
        t!(
            "cli.incremental.messages.incrementing_archive",
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

    let mut builder =
        ArchiveBuilder::with_version(BufWriter::new(file_handle), config, archive_format_version);
    builder.set_archive_output_path(file);
    builder.set_conflict_mode(conflict_mode);
    builder.import_existing_entries(entries);

    if archive_format_version == FormatVersion::V6 {
        let idx_path = crate::index_writer::index_path_for_archive(std::path::Path::new(file));
        let iw = crate::index_writer::IndexWriter::new(&idx_path, archive_timestamp, 1)
            .wrap_err_with(|| {
                t!(
                    "cli.create.errors.index_write_failed",
                    locale = locale.as_str()
                )
                .to_string()
            })?;
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

fn parse_since_arg(value: &str, locale: &Locale) -> Result<u64> {
    if let Some(raw) = value.strip_prefix('@') {
        return raw.parse::<u64>().wrap_err(
            t!(
                "cli.incremental.errors.since_invalid",
                locale = locale.as_str(),
                value = value
            )
            .to_string(),
        );
    }

    OffsetDateTime::parse(value, &Rfc3339)
        .map(|dt| dt.unix_timestamp().max(0) as u64)
        .wrap_err(
            t!(
                "cli.incremental.errors.since_invalid",
                locale = locale.as_str(),
                value = value
            )
            .to_string(),
        )
}

fn run_dry_run(
    file: &str,
    existing_entries: &[crate::models::archive::ArchiveIndexEntryWrapper],
    prepared: &[PreparedFile],
    conflict_mode: ConflictMode,
    locale: &Locale,
) {
    println!(
        "{}",
        t!(
            "cli.incremental.messages.dry_run_header",
            locale = locale.as_str(),
            file = file
        )
    );

    let mut dry_run_path_set: HashSet<String> =
        existing_entries.iter().map(|e| e.path.clone()).collect();
    for p in prepared {
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
        t!(
            "cli.incremental.messages.dry_run_footer",
            locale = locale.as_str()
        )
    );
}

fn preflight_conflict_check(
    existing_entries: &[crate::models::archive::ArchiveIndexEntryWrapper],
    prepared: &[PreparedFile],
    conflict_mode: ConflictMode,
    locale: &Locale,
) -> Result<()> {
    if conflict_mode != ConflictMode::Error {
        return Ok(());
    }
    let existing_path_set: HashSet<&str> =
        existing_entries.iter().map(|e| e.path.as_str()).collect();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut conflicts: Vec<&str> = Vec::new();
    for p in prepared {
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

    try_decrypt_bytes(
        &data,
        &probe.checksum,
        probe.bitflags,
        &probe.extra,
        passphrase,
    )
    .ok_or_else(|| {
        eyre!(t!(
            "cli.append.errors.append_passphrase_invalid",
            locale = locale.as_str()
        ))
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_since_arg;
    use crate::i18n::Locale;

    #[test]
    fn parse_since_unix_prefixed() {
        let locale = Locale::new("en");
        assert_eq!(parse_since_arg("@42", &locale).unwrap(), 42);
    }

    #[test]
    fn parse_since_rfc3339() {
        let locale = Locale::new("en");
        assert_eq!(
            parse_since_arg("1970-01-01T00:00:42Z", &locale).unwrap(),
            42
        );
    }
}
