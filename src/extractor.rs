use crate::constants::crypto;
use crate::constants::flags;
use crate::encryption::nonce_from_checksum;
use crate::models::archive::ArchiveIndexEntryWrapper;
use crate::traits::decompress_bytes;
use crate::utils::sanitize_path;
use chacha20poly1305::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce, Tag};
use eyre::{Result, eyre};
use rust_i18n::t;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Extract a single entry from an archive on disk.
///
/// Convenience wrapper around [`extract_entries`].
pub fn extract_entry(
    archive_path: &Path,
    entry: &ArchiveIndexEntryWrapper,
    all_entries: &[ArchiveIndexEntryWrapper],
    dest_dir: &Path,
    passphrase: Option<&str>,
) -> Result<()> {
    let mut file = File::open(archive_path).map_err(|_| {
        eyre!(t!(
            "cli.extractor.errors.open_failed",
            file = archive_path.display()
        ))
    })?;
    extract_one(&mut file, entry, all_entries, dest_dir, passphrase)
}

/// Extract multiple entries from an archive on disk.
///
/// Opens the archive once and processes every entry in `entries_to_extract` in
/// order.  If `flags::LINKED_DATA` is set on an entry the primary entry
/// (same checksum, no linked flag) is looked up in `all_entries` to resolve
/// the real data offset.
pub fn extract_entries(
    archive_path: &Path,
    entries_to_extract: &[&ArchiveIndexEntryWrapper],
    all_entries: &[ArchiveIndexEntryWrapper],
    dest_dir: &Path,
    passphrase: Option<&str>,
) -> Result<()> {
    let mut file = File::open(archive_path).map_err(|_| {
        eyre!(t!(
            "cli.extractor.errors.open_failed",
            file = archive_path.display()
        ))
    })?;
    for entry in entries_to_extract {
        extract_one(&mut file, entry, all_entries, dest_dir, passphrase)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn extract_one(
    file: &mut File,
    entry: &ArchiveIndexEntryWrapper,
    all_entries: &[ArchiveIndexEntryWrapper],
    dest_dir: &Path,
    passphrase: Option<&str>,
) -> Result<()> {
    // Copy packed fields to local variables before using them.
    let bitflags = entry.entry.bitflags;
    let checksum = entry.entry.checksum;
    let compression_method = entry.entry.compression_method;
    let compressed_size = entry.entry.compressed_size;

    // Resolve the real data offset: linked entries share the primary's offset.
    let data_offset = if bitflags & flags::LINKED_DATA != 0 {
        resolve_primary_offset(&checksum, all_entries).ok_or_else(|| {
            eyre!(t!(
                "cli.extractor.errors.no_primary_for_linked",
                path = entry.path.as_str()
            ))
        })?
    } else {
        entry.entry.offset
    };

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
        decrypt_data(&raw, &checksum, pass).map_err(|e| {
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
    fs::write(&dest_path, &plain).map_err(|_| {
        eyre!(t!(
            "cli.extractor.errors.write_failed",
            path = dest_path.display()
        ))
    })?;

    Ok(())
}

/// Find the data offset of the first non-linked entry that has `checksum`.
pub fn resolve_primary_offset(
    checksum: &[u8; 32],
    all_entries: &[ArchiveIndexEntryWrapper],
) -> Option<u64> {
    all_entries
        .iter()
        .find(|e| e.entry.checksum == *checksum && (e.entry.bitflags & flags::LINKED_DATA) == 0)
        .map(|e| e.entry.offset)
}

/// Read the raw (compressed / possibly encrypted) bytes for `entry` from the archive on disk.
///
/// Follows `flags::LINKED_DATA` to resolve the actual data offset.  Returns
/// `None` on any I/O error so callers that only need a best-effort read (e.g.
/// the inspect preview) can handle the failure gracefully.
pub fn read_raw_entry_bytes(
    archive_path: &Path,
    entry: &ArchiveIndexEntryWrapper,
    all_entries: &[ArchiveIndexEntryWrapper],
) -> Option<Vec<u8>> {
    let offset = if entry.entry.bitflags & flags::LINKED_DATA != 0 {
        resolve_primary_offset(&entry.entry.checksum, all_entries)?
    } else {
        entry.entry.offset
    };
    let mut file = File::open(archive_path).ok()?;
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = vec![0u8; entry.entry.compressed_size as usize];
    file.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// Attempt to decrypt a ChaCha20-Poly1305 ciphertext.
///
/// Returns `None` if `data` is too short or the AEAD tag does not verify (wrong
/// passphrase).  Use this when only a success/failure answer is needed; `decrypt_data`
/// wraps this for the extractor where a proper `Result` with a user-facing message
/// is expected.
pub fn try_decrypt_bytes(data: &[u8], checksum: &[u8; 32], passphrase: &str) -> Option<Vec<u8>> {
    if data.len() < crypto::TAG_LEN {
        return None;
    }
    let tag_bytes = &data[data.len() - crypto::TAG_LEN..];
    let mut ciphertext = data[..data.len() - crypto::TAG_LEN].to_vec();
    let nonce = nonce_from_checksum(checksum);
    let key = blake3::derive_key("dari.v1.chacha20poly1305.key", passphrase.as_bytes());
    let cipher = ChaCha20Poly1305::new((&key).into());
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
fn decrypt_data(data: &[u8], checksum: &[u8; 32], passphrase: &str) -> Result<Vec<u8>> {
    if data.len() < crypto::TAG_LEN {
        return Err(eyre!(t!("cli.extractor.errors.data_too_short")));
    }
    try_decrypt_bytes(data, checksum, passphrase)
        .ok_or_else(|| eyre!(t!("cli.extractor.errors.decrypt_invalid")))
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

    // --- decrypt_data unit tests ---

    #[test]
    fn test_decrypt_data_rejects_too_short_input() {
        let result = decrypt_data(&[0u8; 10], &[0u8; 32], "pass");
        assert!(result.is_err());
    }
}
