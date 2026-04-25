use crate::constants::crypto;
use crate::constants::flags;
use crate::constants::extra_keys;
use crate::encryption::nonce_for_segment;
use crate::encryption::nonce_from_checksum;
use crate::extra::parse_extra_pairs;
use crate::models::archive::ArchiveIndexEntryWrapper;
use crate::traits::decompress_bytes;
use crate::utils::sanitize_path;
use crate::xattrs::{hardlink_target, restore_xattrs};
use chacha20poly1305::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce, Tag};
use eyre::{Result, eyre};
use rust_i18n::t;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Extract a single entry from an archive on disk.
///
/// Convenience wrapper around [`extract_entries`].
///
/// # Errors
///
/// Returns an error if the archive cannot be opened, the entry's data cannot be read,
/// decryption fails (wrong passphrase, passphrase missing), or decompression fails.
pub fn extract_entry(
    archive_path: &Path,
    entry: &ArchiveIndexEntryWrapper,
    all_entries: &[ArchiveIndexEntryWrapper],
    dest_dir: &Path,
    passphrase: Option<&str>,
) -> Result<()> {
    extract_one(archive_path, entry, all_entries, dest_dir, passphrase)
}

/// Extract multiple entries from an archive on disk.
///
/// Opens the archive once and processes every entry in `entries_to_extract` in
/// order.  If `flags::LINKED_DATA` is set on an entry the primary entry
/// (same checksum, no linked flag) is looked up in `all_entries` to resolve
/// the real data offset.
///
/// # Errors
///
/// Returns an error if the archive cannot be opened, or if any individual entry
/// fails to be extracted (see [`extract_entry`]).
pub fn extract_entries(
    archive_path: &Path,
    entries_to_extract: &[&ArchiveIndexEntryWrapper],
    all_entries: &[ArchiveIndexEntryWrapper],
    dest_dir: &Path,
    passphrase: Option<&str>,
) -> Result<()> {
    for entry in entries_to_extract {
        if hardlink_target(&entry.xattrs).is_none() {
            extract_one(archive_path, entry, all_entries, dest_dir, passphrase)?;
        }
    }
    for entry in entries_to_extract {
        if hardlink_target(&entry.xattrs).is_some() {
            extract_one(archive_path, entry, all_entries, dest_dir, passphrase)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn extract_one(
    archive_path: &Path,
    entry: &ArchiveIndexEntryWrapper,
    all_entries: &[ArchiveIndexEntryWrapper],
    dest_dir: &Path,
    passphrase: Option<&str>,
) -> Result<()> {
    let safe_path = sanitize_path(&entry.path);
    let dest_path = dest_dir.join(&safe_path);
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent).map_err(|_| {
            eyre!(t!(
                "cli.extractor.errors.create_dir_failed",
                path = parent.display()
            ))
        })?;
    }

    if let Some(target) = hardlink_target(&entry.xattrs) {
        let target_path = dest_dir.join(sanitize_path(target));
        fs::hard_link(&target_path, &dest_path).map_err(|_| {
            eyre!(t!(
                "cli.extractor.errors.write_failed",
                path = dest_path.display()
            ))
        })?;
        restore_xattrs(&dest_path, &entry.xattrs)?;
        return Ok(());
    }

    // Copy packed fields to local variables before using them.
    let bitflags = entry.entry.bitflags;
    let checksum = entry.entry.checksum;
    let compression_method = entry.entry.compression_method;
    let compressed_size = entry.entry.compressed_size;

    // Resolve the real data offset: linked entries share the primary's offset.
    let (data_volume, data_offset) = if bitflags & flags::LINKED_DATA != 0 {
        resolve_primary_location(&checksum, all_entries).ok_or_else(|| {
            eyre!(t!(
                "cli.extractor.errors.no_primary_for_linked",
                path = entry.path.as_str()
            ))
        })?
    } else {
        (entry.volume_number, entry.entry.offset)
    };

    let volume_path = resolve_volume_path(archive_path, data_volume);
    let mut file = std::fs::File::open(&volume_path).map_err(|_| {
        eyre!(t!(
            "cli.extractor.errors.open_failed",
            file = volume_path.display()
        ))
    })?;

    // Seek to the data block and read all compressed bytes.
    file.seek(SeekFrom::Start(data_offset))
        .map_err(|_| eyre!(t!("cli.extractor.errors.seek_failed")))?;
    let mut raw = vec![0u8; compressed_size as usize];
    file.read_exact(&mut raw)
        .map_err(|_| eyre!(t!("cli.extractor.errors.read_failed")))?;

    // Decrypt when the entry carries the encrypted flag.
    let decrypted = if bitflags & flags::ENCRYPTED_DATA != 0 {
        let pass = passphrase.ok_or_else(|| {
            eyre!(t!(
                "cli.extractor.errors.passphrase_required",
                path = entry.path.as_str()
            ))
        })?;
        decrypt_data(&raw, &checksum, bitflags, &entry.extra, pass).map_err(|e| {
            eyre!(t!(
                "cli.extractor.errors.decrypt_failed",
                path = entry.path.as_str(),
                error = e
            ))
        })?
    } else {
        raw
    };

    // Decompress according to the stored compression method.
    let plain = decompress_bytes(compression_method, &decrypted).map_err(|e| {
        eyre!(t!(
            "cli.extractor.errors.decompress_failed",
            path = entry.path.as_str(),
            error = e
        ))
    })?;

    // Write the recovered bytes to dest_dir / entry.path, creating dirs as needed.
    // Sanitize the stored path to prevent path traversal (strips `..`, `/`, Windows prefixes).
    fs::write(&dest_path, &plain).map_err(|_| {
        eyre!(t!(
            "cli.extractor.errors.write_failed",
            path = dest_path.display()
        ))
    })?;
    restore_xattrs(&dest_path, &entry.xattrs)?;

    Ok(())
}

/// Find the data offset of the first non-linked entry that has `checksum`.
pub fn resolve_primary_offset(
    checksum: &[u8; 32],
    all_entries: &[ArchiveIndexEntryWrapper],
) -> Option<u64> {
    resolve_primary_location(checksum, all_entries).map(|(_, offset)| offset)
}

pub fn resolve_primary_location(
    checksum: &[u8; 32],
    all_entries: &[ArchiveIndexEntryWrapper],
) -> Option<(u16, u64)> {
    all_entries
        .iter()
        .find(|e| e.entry.checksum == *checksum && (e.entry.bitflags & flags::LINKED_DATA) == 0)
        .map(|e| (e.volume_number, e.entry.offset))
}

pub fn resolve_volume_path(base: &Path, volume: u16) -> PathBuf {
    let has_numeric_suffix = base
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.len() == 3 && ext.chars().all(|c| c.is_ascii_digit()));

    if volume == 0 && !has_numeric_suffix {
        return base.to_path_buf();
    }

    let root = if has_numeric_suffix {
        base.with_extension("")
    } else {
        base.to_path_buf()
    };
    PathBuf::from(format!("{}.{:03}", root.display(), u32::from(volume) + 1))
}

/// Read the raw (compressed / possibly encrypted) bytes for `entry` from the archive on disk.
///
/// Follows `flags::LINKED_DATA` to resolve the actual data offset.
///
/// # Note
///
/// This function intentionally returns `Option<Vec<u8>>` instead of `eyre::Result` because
/// it is used for **best-effort preview reads** in the TUI inspector (`tui/preview.rs`).
/// Any I/O failure (e.g. archive truncated, permission error, corrupted offset) is silently
/// converted to `None` so the caller can display a graceful "cannot preview" message instead
/// of crashing the TUI.  For extraction that must succeed, use [`extract_entry`] or
/// [`extract_entries`] which return proper `Result` values with user-facing error messages.
///
/// Set the `DARI_DEBUG=1` environment variable to have I/O failures logged to stderr, which
/// helps diagnose unexpected `None` returns during development.
pub fn read_raw_entry_bytes(
    archive_path: &Path,
    entry: &ArchiveIndexEntryWrapper,
    all_entries: &[ArchiveIndexEntryWrapper],
) -> Option<Vec<u8>> {
    let debug = std::env::var("DARI_DEBUG").is_ok();

    let (volume, offset) = if entry.entry.bitflags & flags::LINKED_DATA != 0 {
        let primary = resolve_primary_location(&entry.entry.checksum, all_entries);
        if primary.is_none() && debug {
            eprintln!(
                "[dari debug] read_raw_entry_bytes: no primary entry found for linked entry '{}'",
                entry.path
            );
        }
        primary?
    } else {
        (entry.volume_number, entry.entry.offset)
    };

    let volume_path = resolve_volume_path(archive_path, volume);
    let mut file = match std::fs::File::open(&volume_path) {
        Ok(f) => f,
        Err(e) => {
            if debug {
                eprintln!(
                    "[dari debug] read_raw_entry_bytes: failed to open '{}': {e}",
                    volume_path.display()
                );
            }
            return None;
        }
    };

    if let Err(e) = file.seek(SeekFrom::Start(offset)) {
        if debug {
            eprintln!(
                "[dari debug] read_raw_entry_bytes: seek to offset {offset} failed for '{}': {e}",
                entry.path
            );
        }
        return None;
    }

    let mut buf = vec![0u8; entry.entry.compressed_size as usize];
    if let Err(e) = file.read_exact(&mut buf) {
        if debug {
            eprintln!(
                "[dari debug] read_raw_entry_bytes: read_exact failed for '{}': {e}",
                entry.path
            );
        }
        return None;
    }

    Some(buf)
}

/// Attempt to decrypt a ChaCha20-Poly1305 ciphertext.
///
/// Returns `None` if `data` is too short or the AEAD tag does not verify (wrong
/// passphrase).  Use this when only a success/failure answer is needed; `decrypt_data`
/// wraps this for the extractor where a proper `Result` with a user-facing message
/// is expected.
pub fn try_decrypt_bytes(
    data: &[u8],
    checksum: &[u8; 32],
    bitflags: u16,
    extra: &str,
    passphrase: &str,
) -> Option<Vec<u8>> {
    let nonce = nonce_from_checksum(checksum);
    let key = blake3::derive_key("dari.v1.chacha20poly1305.key", passphrase.as_bytes());
    let cipher = ChaCha20Poly1305::new((&key).into());

    if bitflags & flags::CHUNKED_ENCRYPTION != 0 {
        let segments = parse_chunked_segment_count(extra)?;
        let total_tag_bytes = segments.checked_mul(crypto::TAG_LEN)?;
        if data.len() < total_tag_bytes {
            return None;
        }
        let total_plain_len = data.len() - total_tag_bytes;
        let mut remaining_plain = total_plain_len;
        let mut cursor = 0usize;
        let mut plaintext = Vec::with_capacity(total_plain_len);

        for segment_idx in 0..segments {
            let segment_plain_len = if segment_idx + 1 == segments {
                remaining_plain
            } else {
                remaining_plain.min(crypto::SEGMENT_SIZE)
            };
            let next = cursor.checked_add(segment_plain_len + crypto::TAG_LEN)?;
            if next > data.len() {
                return None;
            }

            let mut ciphertext = data[cursor..cursor + segment_plain_len].to_vec();
            let tag_bytes = &data[cursor + segment_plain_len..next];
            cipher
                .decrypt_in_place_detached(
                    Nonce::from_slice(&nonce_for_segment(&nonce, segment_idx as u64)),
                    b"",
                    &mut ciphertext,
                    Tag::from_slice(tag_bytes),
                )
                .ok()?;
            plaintext.extend_from_slice(&ciphertext);
            cursor = next;
            remaining_plain = remaining_plain.saturating_sub(segment_plain_len);
        }

        if cursor != data.len() || remaining_plain != 0 {
            return None;
        }

        return Some(plaintext);
    }

    if data.len() < crypto::TAG_LEN {
        return None;
    }
    let tag_bytes = &data[data.len() - crypto::TAG_LEN..];
    let mut ciphertext = data[..data.len() - crypto::TAG_LEN].to_vec();
    cipher
        .decrypt_in_place_detached(
            Nonce::from_slice(&nonce),
            b"",
            &mut ciphertext,
            Tag::from_slice(tag_bytes),
        )
        .ok()?;
    Some(ciphertext)
}

/// Decrypt ChaCha20-Poly1305 ciphertext, returning a user-facing `Result`.
///
/// The nonce is the first `crypto::NONCE_LEN` bytes of `checksum` (matching the
/// encoding in `pipeline.rs`).  The authentication tag occupies the last
/// `crypto::TAG_LEN` bytes of `data`; the rest is the actual ciphertext.
fn decrypt_data(
    data: &[u8],
    checksum: &[u8; 32],
    bitflags: u16,
    extra: &str,
    passphrase: &str,
) -> Result<Vec<u8>> {
    if bitflags & flags::CHUNKED_ENCRYPTION == 0 && data.len() < crypto::TAG_LEN {
        return Err(eyre!(t!("cli.extractor.errors.data_too_short")));
    }
    if bitflags & flags::CHUNKED_ENCRYPTION != 0 && parse_chunked_segment_count(extra).is_none() {
        return Err(eyre!(t!(
            "cli.extractor.errors.chunked_segments_missing"
        )));
    }
    try_decrypt_bytes(data, checksum, bitflags, extra, passphrase)
        .ok_or_else(|| eyre!(t!("cli.extractor.errors.decrypt_invalid")))
}

fn parse_chunked_segment_count(extra: &str) -> Option<usize> {
    parse_extra_pairs(extra)
        .into_iter()
        .find(|(key, _)| key == extra_keys::ENC_SEGMENTS)
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .filter(|count| *count > 0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive_builder::ArchiveBuilder;
    use crate::i18n::Locale;
    use crate::pipeline::PipelineConfig;
    use crate::reader::load_archive;
    use crate::test_utils::build_archive;
    use std::fs::File;

    fn load(path: &std::path::Path) -> Vec<ArchiveIndexEntryWrapper> {
        let locale = Locale::new("en");
        let mut f = File::open(path).unwrap();
        load_archive(&mut f, path.to_str().unwrap(), &locale)
            .unwrap()
            .entries
    }

    // --- plain extraction ---

    #[test]
    fn test_extract_single_plain_file() {
        let dir = tempfile::tempdir().unwrap();
        let archive = build_archive(&dir, "a.dar", &[("hello.txt", b"hello world")], None);
        let entries = load(&archive);
        let dest = dir.path().join("out");

        extract_entry(&archive, &entries[0], &entries, &dest, None).unwrap();

        let result = std::fs::read(dest.join("hello.txt")).unwrap();
        assert_eq!(result, b"hello world");
    }

    #[test]
    fn test_extract_creates_nested_directories() {
        let dir = tempfile::tempdir().unwrap();
        // The archive stores a path with a sub-directory component; the source
        // file on disk must live at a flat path (the archive_path is logical).
        let archive_path = dir.path().join("nested.dar");
        let src_file = dir.path().join("file.txt");
        std::fs::write(&src_file, b"nested content").unwrap();

        {
            let file_handle = File::create(&archive_path).unwrap();
            let mut builder = ArchiveBuilder::with_config(
                file_handle,
                PipelineConfig {
                    compress_images: false,
                    encryption_passphrase: None,
                    chunked_encryption: false,
                    preserve_xattrs: false,
                },
            );
            builder.write_header().unwrap();
            // store under a path that includes a sub-directory
            builder.add_file(&src_file, "sub/file.txt").unwrap();
            builder.build().unwrap();
        }

        let entries = load(&archive_path);
        let dest = dir.path().join("out_nested");

        extract_entry(&archive_path, &entries[0], &entries, &dest, None).unwrap();

        let result = std::fs::read(dest.join("sub/file.txt")).unwrap();
        assert_eq!(result, b"nested content");
    }

    #[test]
    fn test_extract_multiple_files() {
        let dir = tempfile::tempdir().unwrap();
        let archive = build_archive(
            &dir,
            "multi.dar",
            &[("a.txt", b"aaa"), ("b.txt", b"bbb"), ("c.txt", b"ccc")],
            None,
        );
        let entries = load(&archive);
        let dest = dir.path().join("out_multi");

        let refs: Vec<&ArchiveIndexEntryWrapper> = entries.iter().collect();
        extract_entries(&archive, &refs, &entries, &dest, None).unwrap();

        assert_eq!(std::fs::read(dest.join("a.txt")).unwrap(), b"aaa");
        assert_eq!(std::fs::read(dest.join("b.txt")).unwrap(), b"bbb");
        assert_eq!(std::fs::read(dest.join("c.txt")).unwrap(), b"ccc");
    }

    // --- encrypted extraction ---

    #[test]
    fn test_extract_encrypted_file_with_correct_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let archive = build_archive(
            &dir,
            "enc.dar",
            &[("secret.txt", b"secret data")],
            Some("hunter2"),
        );
        let entries = load(&archive);
        let dest = dir.path().join("out_enc");

        extract_entry(&archive, &entries[0], &entries, &dest, Some("hunter2")).unwrap();

        let result = std::fs::read(dest.join("secret.txt")).unwrap();
        assert_eq!(result, b"secret data");
    }

    #[test]
    fn test_extract_encrypted_file_fails_with_wrong_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let archive = build_archive(
            &dir,
            "enc_wrong.dar",
            &[("secret.txt", b"secret data")],
            Some("correct"),
        );
        let entries = load(&archive);
        let dest = dir.path().join("out_enc_wrong");

        let result = extract_entry(&archive, &entries[0], &entries, &dest, Some("wrong"));
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_encrypted_file_fails_without_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let archive = build_archive(
            &dir,
            "enc_no_pass.dar",
            &[("secret.txt", b"secret data")],
            Some("pass"),
        );
        let entries = load(&archive);
        let dest = dir.path().join("out_no_pass");

        let result = extract_entry(&archive, &entries[0], &entries, &dest, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_chunked_encrypted_v6_file_with_correct_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("chunked.dar");
        let source = dir.path().join("chunked.txt");
        let content = b"chunked payload ".repeat(100_000);
        std::fs::write(&source, &content).unwrap();

        {
            let file_handle = File::create(&archive_path).unwrap();
            let mut builder = ArchiveBuilder::with_version(
                file_handle,
                PipelineConfig {
                    compress_images: false,
                    encryption_passphrase: Some("secret".to_string()),
                    chunked_encryption: true,
                    preserve_xattrs: false,
                },
                crate::format_version::FormatVersion::V6,
            );
            builder.write_header().unwrap();
            builder.add_file(&source, "chunked.txt").unwrap();
            builder.build().unwrap();
        }

        let entries = load(&archive_path);
        let dest = dir.path().join("out_chunked");

        extract_entry(&archive_path, &entries[0], &entries, &dest, Some("secret")).unwrap();

        let result = std::fs::read(dest.join("chunked.txt")).unwrap();
        assert_eq!(result, content);
    }

    // --- deduplication / linked data ---

    #[test]
    fn test_extract_linked_entry_recovers_correct_content() {
        let dir = tempfile::tempdir().unwrap();
        let content = b"duplicate content that will be deduplicated";

        // Write identical content under two names; the second becomes linked.
        let archive_path = build_archive(
            &dir,
            "dedup.dar",
            &[
                ("copy1.txt", content.as_ref()),
                ("copy2.txt", content.as_ref()),
            ],
            None,
        );

        let entries = load(&archive_path);
        assert_eq!(entries.len(), 2);

        // At least one entry should carry the linked flag.
        let has_linked = entries
            .iter()
            .any(|e| e.entry.bitflags & flags::LINKED_DATA != 0);
        assert!(has_linked, "expected a linked entry after deduplication");

        let dest = dir.path().join("out_dedup");
        let refs: Vec<&ArchiveIndexEntryWrapper> = entries.iter().collect();
        extract_entries(&archive_path, &refs, &entries, &dest, None).unwrap();

        assert_eq!(std::fs::read(dest.join("copy1.txt")).unwrap(), content);
        assert_eq!(std::fs::read(dest.join("copy2.txt")).unwrap(), content);
    }

    // --- resolve_primary_offset unit tests ---

    #[test]
    fn test_resolve_primary_offset_returns_none_when_empty() {
        assert!(resolve_primary_offset(&[0u8; 32], &[]).is_none());
    }

    #[test]
    fn test_resolve_volume_path_single_file_returns_base() {
        let path = resolve_volume_path(Path::new("/tmp/archive.dar"), 0);
        assert_eq!(path, Path::new("/tmp/archive.dar"));
    }

    #[test]
    fn test_resolve_volume_path_split_volume_advances_suffix() {
        let path = resolve_volume_path(Path::new("/tmp/archive.dar.001"), 1);
        assert_eq!(path, Path::new("/tmp/archive.dar.002"));
    }

    // --- decrypt_data unit tests ---

    #[test]
    fn test_decrypt_data_rejects_too_short_input() {
        let result = decrypt_data(&[0u8; 10], &[0u8; 32], 0, "", "pass");
        assert!(result.is_err());
    }

    // --- 5.4 path sanitisation adversarial cases ---

    /// Build an archive where the index entry records an arbitrary logical `archive_path`
    /// (bypassing `calculate_archive_path`) so we can test the extractor's sanitisation.
    fn build_archive_with_path(
        dir: &tempfile::TempDir,
        archive_name: &str,
        archive_path: &str,
        content: &[u8],
    ) -> std::path::PathBuf {
        let archive_file = dir.path().join(archive_name);
        let src = dir.path().join("_src_tmp");
        std::fs::write(&src, content).unwrap();
        let fh = File::create(&archive_file).unwrap();
        let mut builder = ArchiveBuilder::with_config(
            fh,
            PipelineConfig {
                compress_images: false,
                encryption_passphrase: None,
                chunked_encryption: false,
                preserve_xattrs: false,
            },
        );
        builder.write_header().unwrap();
        builder.add_file(&src, archive_path).unwrap();
        builder.build().unwrap();
        archive_file
    }

    #[test]
    fn test_path_traversal_dotdot_is_sanitised() {
        let dir = tempfile::tempdir().unwrap();
        let archive = build_archive_with_path(&dir, "trav.dar", "../../etc/passwd", b"evil");
        let entries = load(&archive);
        let dest = dir.path().join("out_trav");
        extract_entry(&archive, &entries[0], &entries, &dest, None).unwrap();

        // The extracted file must be inside `dest`, not at ../../etc/passwd
        let extracted_path = dest.join("etc/passwd");
        assert!(
            extracted_path.exists(),
            "sanitised path should land at dest/etc/passwd"
        );
        let evil_path = std::path::Path::new("/etc/passwd");
        assert!(
            !evil_path.exists() || std::fs::read(evil_path).unwrap_or_default() != b"evil",
            "must not have overwritten /etc/passwd"
        );
    }

    #[test]
    fn test_absolute_path_is_sanitised() {
        let dir = tempfile::tempdir().unwrap();
        let archive = build_archive_with_path(&dir, "abs.dar", "/tmp/evil_absolute", b"abs");
        let entries = load(&archive);
        let dest = dir.path().join("out_abs");
        extract_entry(&archive, &entries[0], &entries, &dest, None).unwrap();

        // Absolute leading slash must be stripped; file lands inside dest
        let expected = dest.join("tmp/evil_absolute");
        assert!(
            expected.exists(),
            "absolute path must be sanitised to a relative path under dest"
        );
    }

    #[test]
    fn test_windows_drive_prefix_is_sanitised() {
        let dir = tempfile::tempdir().unwrap();
        // On Unix this just looks like a path component with a colon, but sanitize_path
        // must still produce a safe relative path.
        let archive =
            build_archive_with_path(&dir, "win.dar", "C:\\Windows\\System32\\file.txt", b"win");
        let entries = load(&archive);
        let dest = dir.path().join("out_win");
        extract_entry(&archive, &entries[0], &entries, &dest, None).unwrap();

        // dest must contain exactly the sanitised file; nothing escaped outside dest
        fn any_file_under(dir: &std::path::Path) -> bool {
            if let Ok(rd) = std::fs::read_dir(dir) {
                for entry in rd.flatten() {
                    let p = entry.path();
                    if p.is_file() {
                        return true;
                    } else if p.is_dir() && any_file_under(&p) {
                        return true;
                    }
                }
            }
            false
        }
        assert!(
            any_file_under(&dest),
            "extracted file should land somewhere inside dest"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_extract_preserves_hardlinks_and_xattrs_for_v6() {
        use crate::format_version::FormatVersion;
        use crate::xattrs::hardlink_target;
        use std::os::unix::fs::MetadataExt;

        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("preserve.dar");
        let source_a = dir.path().join("source-a.txt");
        let source_b = dir.path().join("source-b.txt");
        std::fs::write(&source_a, b"preserved payload").unwrap();
        std::fs::hard_link(&source_a, &source_b).unwrap();
        xattr::set(&source_a, "user.dari.test", b"roundtrip").unwrap();

        {
            let file_handle = File::create(&archive_path).unwrap();
            let mut builder = ArchiveBuilder::with_version(
                file_handle,
                PipelineConfig {
                    compress_images: false,
                    encryption_passphrase: None,
                    chunked_encryption: false,
                    preserve_xattrs: true,
                },
                FormatVersion::V6,
            );
            builder.write_header().unwrap();
            builder.add_file(&source_a, "a.txt").unwrap();
            builder.add_file(&source_b, "b.txt").unwrap();
            builder.build().unwrap();
        }

        let entries = load(&archive_path);
        assert_eq!(
            xattr::get(dir.path().join("source-a.txt"), "user.dari.test").unwrap(),
            Some(b"roundtrip".to_vec())
        );
        assert!(
            entries
                .iter()
                .find(|entry| entry.path == "a.txt")
                .is_some_and(|entry| !entry.xattrs.is_empty())
        );
        assert_eq!(
            entries
                .iter()
                .find(|entry| entry.path == "b.txt")
                .and_then(|entry| hardlink_target(&entry.xattrs)),
            Some("a.txt")
        );

        let dest = dir.path().join("out_preserve");
        let refs: Vec<&ArchiveIndexEntryWrapper> = entries.iter().collect();
        extract_entries(&archive_path, &refs, &entries, &dest, None).unwrap();

        assert_eq!(std::fs::read(dest.join("a.txt")).unwrap(), b"preserved payload");
        assert_eq!(std::fs::read(dest.join("b.txt")).unwrap(), b"preserved payload");
        assert_eq!(
            xattr::get(dest.join("a.txt"), "user.dari.test").unwrap(),
            Some(b"roundtrip".to_vec())
        );
        let a_meta = std::fs::metadata(dest.join("a.txt")).unwrap();
        let b_meta = std::fs::metadata(dest.join("b.txt")).unwrap();
        assert_eq!(a_meta.ino(), b_meta.ino(), "extracted files should be hard-linked");
    }
}
