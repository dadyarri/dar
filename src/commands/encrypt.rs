use crate::archive_builder::ArchiveBuilder;
use crate::encryption::resolve_encryption_passphrase;
use crate::extractor::extract_entries;
use crate::i18n::Locale;
use crate::pipeline::PipelineConfig;
use crate::reader::load_archive;
use clap::ArgMatches;
use eyre::{Context, Result, eyre};
use rust_i18n::t;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

/// A temporary directory that is automatically deleted when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn create() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!("dari-encrypt-{}", std::process::id()));
        fs::create_dir_all(&path)?;
        Ok(TempDir(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

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
    let parent = archive_path.parent().unwrap_or_else(|| Path::new("."));

    let in_place = matches.get_flag("in-place");
    let output_path: PathBuf = if in_place {
        archive_path.to_path_buf()
    } else if let Some(output) = matches.get_one::<String>("output") {
        PathBuf::from(output)
    } else {
        // `file_name()` only returns None for paths that end in `..`, which
        // cannot exist on disk, so the fallback here is unreachable in practice.
        let file_name = archive_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("archive");
        let new_name = if let Some(stripped) = file_name.strip_suffix(".dar") {
            format!("{}.enc.dar", stripped)
        } else {
            format!("{}.enc.dar", file_name)
        };
        parent.join(new_name)
    };

    let output_dir = output_path.parent().unwrap_or_else(|| Path::new("."));

    // Extract all entries from the unencrypted archive to a temp directory.
    let temp_extract_dir = TempDir::create().wrap_err(
        t!(
            "cli.encrypt.errors.encrypt_failed",
            locale = locale.as_str()
        )
        .to_string(),
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
            t!(
                "cli.encrypt.errors.encrypt_failed",
                locale = locale.as_str()
            )
            .to_string(),
        )?;
    }

    // Build a new encrypted archive to a uniquely-named temp file in the same
    // directory as the output. Both paths are on the same filesystem so the
    // subsequent rename is effectively atomic.
    // Note: this re-runs the full pipeline (including compression) on the extracted
    // data. Files are recompressed with the default settings (compress_images: false)
    // since the original pipeline settings are not stored in the archive format.
    let temp_path = output_dir.join(format!(".dari-enc-{}.tmp", std::process::id()));
    let temp_file = File::create(&temp_path).wrap_err(
        t!(
            "cli.encrypt.errors.encrypt_failed",
            locale = locale.as_str()
        )
        .to_string(),
    )?;

    let config = PipelineConfig {
        compress_images: false,
        encryption_passphrase: Some(passphrase),
        chunked_encryption: false,
        preserve_xattrs: false,
    };

    let mut builder = ArchiveBuilder::with_config(BufWriter::new(temp_file), config);
    builder.write_header()?;

    for entry in &existing_archive.entries {
        let source = temp_extract_dir.path().join(&entry.path);
        builder.add_file(&source, &entry.path).wrap_err_with(|| {
            t!(
                "cli.encrypt.errors.encrypt_failed",
                locale = locale.as_str()
            )
            .to_string()
        })?;
    }

    builder.build()?;

    // Move the temp file to the output path. Since temp_out was created inside
    // output_dir (the same directory as output_path), both paths are on the
    // same filesystem and the rename is effectively atomic.
    std::fs::rename(&temp_path, &output_path).wrap_err(
        t!(
            "cli.encrypt.errors.encrypt_failed",
            locale = locale.as_str()
        )
        .to_string(),
    )?;

    println!(
        "{}",
        t!("cli.encrypt.messages.done", locale = locale.as_str())
    );

    Ok(())
}
