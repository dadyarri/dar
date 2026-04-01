use crate::archive_builder::ArchiveBuilder;
use crate::encryption::resolve_encryption_passphrase;
use crate::extractor::extract_entries;
use crate::i18n::Locale;
use crate::pipeline::PipelineConfig;
use crate::reader::load_archive;
use clap::ArgMatches;
use eyre::{Context, Result, eyre};
use rust_i18n::t;
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

    if !Path::new(file).exists() {
        return Err(eyre!(t!(
            "cli.encrypt.errors.archive_missing",
            locale = locale.as_str(),
            file = file
        )));
    }

    let passphrase_opt = resolve_encryption_passphrase(matches, locale)?;
    let passphrase = passphrase_opt.ok_or_else(|| {
        eyre!(t!(
            "cli.encrypt.errors.passphrase_required",
            locale = locale.as_str()
        ))
    })?;

    let mut file_handle = File::open(file).wrap_err_with(|| {
        t!(
            "cli.encrypt.errors.open_failed",
            locale = locale.as_str(),
            file = file
        )
        .to_string()
    })?;

    let existing_archive = load_archive(&mut file_handle, file, locale)?;

    if existing_archive.encryption_mode == Some(true) {
        return Err(eyre!(t!(
            "cli.encrypt.errors.already_encrypted",
            locale = locale.as_str(),
            file = file
        )));
    }

    drop(file_handle);

    println!(
        "{}",
        t!(
            "cli.encrypt.messages.encrypting_archive",
            locale = locale.as_str(),
            file = file
        )
    );

    let archive_path = Path::new(file);
    let parent = archive_path
        .parent()
        .unwrap_or_else(|| Path::new("."));

    // Extract all entries from the unencrypted archive to a temp directory.
    let temp_extract_dir = tempfile::tempdir().wrap_err(
        t!("cli.encrypt.errors.encrypt_failed", locale = locale.as_str()).to_string(),
    )?;
    let all_refs: Vec<&crate::models::archive::ArchiveIndexEntryWrapper> =
        existing_archive.entries.iter().collect();

    if !existing_archive.entries.is_empty() {
        extract_entries(
            archive_path,
            &all_refs,
            &existing_archive.entries,
            temp_extract_dir.path(),
            None,
        )
        .wrap_err(
            t!("cli.encrypt.errors.encrypt_failed", locale = locale.as_str()).to_string(),
        )?;
    }

    // Build a new encrypted archive in a named temp file in the same directory.
    // Note: this re-runs the full pipeline (including compression) on the extracted
    // data. Files are recompressed with the default settings (compress_images: false)
    // since the original pipeline settings are not stored in the archive format.
    let temp_out = tempfile::NamedTempFile::new_in(parent).wrap_err(
        t!("cli.encrypt.errors.encrypt_failed", locale = locale.as_str()).to_string(),
    )?;
    let (temp_file, temp_path) = temp_out.keep().wrap_err(
        t!("cli.encrypt.errors.encrypt_failed", locale = locale.as_str()).to_string(),
    )?;

    let config = PipelineConfig {
        compress_images: false,
        encryption_passphrase: Some(passphrase),
    };

    let mut builder = ArchiveBuilder::with_config(BufWriter::new(temp_file), config);
    builder.write_header()?;

    for entry in &existing_archive.entries {
        let source = temp_extract_dir.path().join(&entry.path);
        builder
            .add_file(&source, &entry.path)
            .wrap_err_with(|| {
                t!(
                    "cli.encrypt.errors.encrypt_failed",
                    locale = locale.as_str()
                )
                .to_string()
            })?;
    }

    builder.build()?;

    // Atomically replace the original archive.
    std::fs::rename(&temp_path, archive_path).wrap_err(
        t!("cli.encrypt.errors.encrypt_failed", locale = locale.as_str()).to_string(),
    )?;

    println!(
        "{}",
        t!("cli.encrypt.messages.done", locale = locale.as_str())
    );

    Ok(())
}
