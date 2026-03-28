use super::resolve_encryption_passphrase;
use crate::archive_builder::ArchiveBuilder;
use crate::i18n::Locale;
use crate::models::archive::{
    ArchiveFooter, ArchiveHeader, ArchiveIndexEntry, ArchiveIndexEntryWrapper,
};
use crate::pipeline::{INDEX_FLAG_ENCRYPTED_DATA, PipelineConfig};
use crate::walker::scan_files;
use chacha20poly1305::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce, Tag};
use clap::ArgMatches;
use eyre::{Context, Result, eyre};
use rust_i18n::t;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom};
use std::mem::size_of;
use std::path::Path;

pub fn call(matches: &ArgMatches, locale: &Locale) -> Result<()> {
    let file = matches
        .get_one::<String>("file")
        .ok_or_else(|| eyre!(t!("cli.common.errors.file_required", locale = locale.as_str())))?;

    if !Path::new(file).exists() {
        return Err(eyre!(t!(
            "cli.append.errors.append_file_missing",
            locale = locale.as_str(),
            file = file
        )));
    }

    let verbose = matches.get_flag("verbose");
    let compress_images = matches.get_flag("compress-images");
    let encryption_passphrase = resolve_encryption_passphrase(matches, locale)?;
    let content = matches
        .get_many::<String>("content")
        .ok_or_else(|| eyre!(t!("cli.common.errors.content_required", locale = locale.as_str())))?;

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

    let existing_archive = load_existing_archive(&mut file_handle, file, locale)?;

    ensure_encryption_mode(
        existing_archive.encryption_mode,
        encryption_passphrase.is_some(),
        locale,
    )?;

    if let Some(true) = existing_archive.encryption_mode {
        let passphrase = encryption_passphrase.as_deref().ok_or_else(|| {
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

    let ExistingArchiveState {
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

    let config = PipelineConfig {
        compress_images,
        encryption_passphrase,
    };

    let mut builder = ArchiveBuilder::with_config(BufWriter::new(file_handle), config);
    builder.import_existing_entries(entries);

    for file_entry in scan_files(content, locale)? {
        if verbose {
            println!("{}", file_entry.source_path.display());
        }

        builder.add_file(&file_entry.source_path, &file_entry.archive_path)?;
    }

    builder.build()?;

    Ok(())
}

struct ExistingArchiveState {
    entries: Vec<ArchiveIndexEntryWrapper>,
    index_offset: u64,
    encryption_mode: Option<bool>,
    encryption_probe: Option<EncryptedEntryProbe>,
}

#[derive(Clone, Copy)]
struct EncryptedEntryProbe {
    offset: u64,
    size: u32,
    checksum: [u8; 32],
}

fn load_existing_archive(
    file: &mut File,
    file_path: &str,
    locale: &Locale,
) -> Result<ExistingArchiveState> {
    let metadata = file.metadata().wrap_err(
        t!(
            "cli.append.errors.append_read_failed",
            locale = locale.as_str(),
            file = file_path
        )
        .to_string(),
    )?;
    let file_len = metadata.len();
    let header_size = size_of::<ArchiveHeader>() as u64;
    let footer_size = size_of::<ArchiveFooter>() as u64;

    if file_len < header_size + footer_size {
        return Err(eyre!(t!(
            "cli.append.errors.append_footer_invalid",
            locale = locale.as_str()
        )));
    }

    file.seek(SeekFrom::Start(0)).wrap_err(
        t!(
            "cli.append.errors.append_seek_failed",
            locale = locale.as_str(),
            file = file_path
        )
        .to_string(),
    )?;

    let mut header_buf = [0u8; size_of::<ArchiveHeader>()];
    file.read_exact(&mut header_buf).wrap_err(
        t!(
            "cli.append.errors.append_header_read_failed",
            locale = locale.as_str(),
            file = file_path
        )
        .to_string(),
    )?;
    let header = *bytemuck::from_bytes::<ArchiveHeader>(&header_buf);

    if header.signature != *b"DARI" || header.version != 5 {
        return Err(eyre!(t!(
            "cli.append.errors.append_header_invalid",
            locale = locale.as_str()
        )));
    }

    let footer_pos = file_len - footer_size;
    file.seek(SeekFrom::Start(footer_pos)).wrap_err(
        t!(
            "cli.append.errors.append_seek_failed",
            locale = locale.as_str(),
            file = file_path
        )
        .to_string(),
    )?;

    let mut footer_buf = [0u8; size_of::<ArchiveFooter>()];
    file.read_exact(&mut footer_buf).wrap_err(
        t!(
            "cli.append.errors.append_footer_read_failed",
            locale = locale.as_str(),
            file = file_path
        )
        .to_string(),
    )?;
    let footer = *bytemuck::from_bytes::<ArchiveFooter>(&footer_buf);

    if footer.signature != *b"DARIEND" {
        return Err(eyre!(t!(
            "cli.append.errors.append_footer_invalid",
            locale = locale.as_str()
        )));
    }

    let index_offset = footer.index_offset as u64;
    if index_offset < header_size || index_offset > footer_pos {
        return Err(eyre!(t!(
            "cli.append.errors.append_footer_invalid",
            locale = locale.as_str()
        )));
    }

    file.seek(SeekFrom::Start(index_offset)).wrap_err(
        t!(
            "cli.append.errors.append_seek_failed",
            locale = locale.as_str(),
            file = file_path
        )
        .to_string(),
    )?;

    let mut entries = Vec::with_capacity(footer.amount_of_files as usize);
    let mut encryption_mode: Option<bool> = None;
    let mut encryption_probe: Option<EncryptedEntryProbe> = None;
    for _ in 0..footer.amount_of_files {
        let mut entry_buf = [0u8; size_of::<ArchiveIndexEntry>()];
        file.read_exact(&mut entry_buf).wrap_err(
            t!(
                "cli.append.errors.append_index_decode_failed",
                locale = locale.as_str()
            )
            .to_string(),
        )?;
        let entry = *bytemuck::from_bytes::<ArchiveIndexEntry>(&entry_buf);

        let entry_encrypted = (entry.bitflags & INDEX_FLAG_ENCRYPTED_DATA) != 0;
        match encryption_mode {
            None => encryption_mode = Some(entry_encrypted),
            Some(expected) if expected != entry_encrypted => {
                return Err(eyre!(t!(
                    "cli.append.errors.append_mixed_encryption",
                    locale = locale.as_str()
                )));
            }
            _ => {}
        }

        if entry_encrypted && encryption_probe.is_none() {
            encryption_probe = Some(EncryptedEntryProbe {
                offset: entry.offset as u64,
                size: entry.compressed_size,
                checksum: entry.checksum,
            });
        }

        let mut path_bytes = vec![0u8; entry.path_length as usize];
        file.read_exact(&mut path_bytes).wrap_err(
            t!(
                "cli.append.errors.append_index_decode_failed",
                locale = locale.as_str()
            )
            .to_string(),
        )?;
        let path = String::from_utf8(path_bytes).map_err(|_| {
            eyre!(t!(
                "cli.append.errors.append_utf8_failed",
                locale = locale.as_str(),
                field = "path"
            ))
        })?;

        let mut extra_bytes = vec![0u8; entry.extra_length as usize];
        file.read_exact(&mut extra_bytes).wrap_err(
            t!(
                "cli.append.errors.append_index_decode_failed",
                locale = locale.as_str()
            )
            .to_string(),
        )?;
        let extra = String::from_utf8(extra_bytes).map_err(|_| {
            eyre!(t!(
                "cli.append.errors.append_utf8_failed",
                locale = locale.as_str(),
                field = "extra"
            ))
        })?;

        entries.push(ArchiveIndexEntryWrapper::new(entry, path, extra));
    }

    Ok(ExistingArchiveState {
        entries,
        index_offset,
        encryption_mode,
        encryption_probe,
    })
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
    if probe.size < 16 {
        return Err(eyre!(t!(
            "cli.append.errors.append_encryption_probe_missing",
            locale = locale.as_str()
        )));
    }

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

    if data.len() < 16 {
        return Err(eyre!(t!(
            "cli.append.errors.append_encryption_probe_missing",
            locale = locale.as_str()
        )));
    }

    let tag_bytes: Vec<u8> = data[data.len() - 16..].to_vec();
    data.truncate(data.len() - 16);
    let mut ciphertext = data;

    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&probe.checksum[..12]);
    let key = blake3::derive_key("dari.v1.chacha20poly1305.key", passphrase.as_bytes());
    let cipher = ChaCha20Poly1305::new((&key).into());

    cipher
        .decrypt_in_place_detached(
            Nonce::from_slice(&nonce),
            b"",
            &mut ciphertext,
            Tag::from_slice(&tag_bytes),
        )
        .map_err(|_| {
            eyre!(t!(
                "cli.append.errors.append_passphrase_invalid",
                locale = locale.as_str()
            ))
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_encryption_mode, load_existing_archive, verify_passphrase_matches};
    use crate::archive_builder::ArchiveBuilder;
    use crate::i18n::Locale;
    use crate::pipeline::PipelineConfig;
    use std::fs::{File, OpenOptions};
    use std::path::Path;

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
        let archive_path = dir.path().join("enc.dar");
        let data_path = dir.path().join("file.txt");
        std::fs::write(&data_path, b"secret data").unwrap();

        {
            let file_handle = File::create(&archive_path).unwrap();
            let mut builder = ArchiveBuilder::with_config(
                file_handle,
                PipelineConfig {
                    compress_images: false,
                    encryption_passphrase: Some("secret".into()),
                },
            );
            builder.write_header().unwrap();
            builder
                .add_file(&data_path, data_path.file_name().unwrap().to_str().unwrap())
                .unwrap();
            builder.build().unwrap();
        }

        let mut archive_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&archive_path)
            .unwrap();
        let state =
            load_existing_archive(&mut archive_file, archive_path.to_str().unwrap(), &locale)
                .unwrap();
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
        let archive_path = dir.path().join("enc_fail.dar");
        let data_path = dir.path().join("file.txt");
        std::fs::write(&data_path, b"secret data").unwrap();

        {
            let file_handle = File::create(&archive_path).unwrap();
            let mut builder = ArchiveBuilder::with_config(
                file_handle,
                PipelineConfig {
                    compress_images: false,
                    encryption_passphrase: Some("secret".into()),
                },
            );
            builder.write_header().unwrap();
            builder
                .add_file(&data_path, Path::new("file.txt").to_str().unwrap())
                .unwrap();
            builder.build().unwrap();
        }

        let mut archive_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&archive_path)
            .unwrap();
        let state =
            load_existing_archive(&mut archive_file, archive_path.to_str().unwrap(), &locale)
                .unwrap();
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
