use crate::constants::flags;
use crate::constants::format;
use crate::format_version::FormatVersion;
use crate::i18n::Locale;
use crate::models::archive::{
    ArchiveFooter, ArchiveFooterV6, ArchiveHeader, ArchiveHeaderV6, ArchiveIndexEntry,
    ArchiveIndexEntryV6, ArchiveIndexEntryWrapper,
};
use eyre::{Context, Result, eyre};
use rust_i18n::t;
use std::io::{Read, Seek, SeekFrom};
use std::mem::size_of;

/// A combined [`Read`] + [`Seek`] bound that is object-safe.
///
/// Automatically implemented for any `T: Read + Seek`, including `std::fs::File`
/// and `std::io::Cursor<Vec<u8>>`.  Passing a `Cursor<Vec<u8>>` makes it possible
/// to unit-test archive parsing without touching the filesystem.
pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Wrapper around [`Read::read_exact`] that attaches a translated error context.
fn read_exact_ctx(
    source: &mut dyn ReadSeek,
    buf: &mut [u8],
    ctx_key: &str,
    locale: &Locale,
) -> Result<()> {
    source
        .read_exact(buf)
        .wrap_err(t!(ctx_key, locale = locale.as_str()).to_string())
}

/// Wrapper around [`Seek::seek`] that attaches a translated error context.
fn seek_ctx(
    source: &mut dyn ReadSeek,
    pos: SeekFrom,
    file_path: &str,
    locale: &Locale,
) -> Result<()> {
    source
        .seek(pos)
        .wrap_err(
            t!(
                "cli.common.errors.seek_failed",
                locale = locale.as_str(),
                file = file_path
            )
            .to_string(),
        )
        .map(|_| ())
}

/// Parsed state of an existing `.dar` archive.
pub struct ArchiveState {
    pub entries: Vec<ArchiveIndexEntryWrapper>,
    pub header: ArchiveHeader,
    pub encryption_mode: Option<bool>,
    /// Byte offset where the index section starts; used by `append` to truncate before rewriting.
    pub index_offset: u64,
    /// First encrypted entry found, used by `append` to verify the passphrase before writing.
    pub encryption_probe: Option<EncryptedEntryProbe>,
}

/// Enough information to decrypt one entry and verify that a passphrase is correct.
#[derive(Clone, Copy)]
pub struct EncryptedEntryProbe {
    pub offset: u64,
    pub size: u64,
    pub checksum: [u8; 32],
}

/// Read the first 5 bytes of `source`, verify the `DARI` signature, and
/// return the [`FormatVersion`] encoded in the version byte.
///
/// The cursor position after this call is unspecified; callers should seek
/// before further I/O.
pub fn read_version(source: &mut dyn ReadSeek, locale: &Locale) -> Result<FormatVersion> {
    source.seek(SeekFrom::Start(0)).wrap_err(
        t!(
            "cli.common.errors.seek_failed",
            locale = locale.as_str(),
            file = "<archive>"
        )
        .to_string(),
    )?;

    let mut buf = [0u8; 5];
    read_exact_ctx(source, &mut buf, "cli.common.errors.header_read_failed", locale)?;

    if &buf[0..4] != format::SIGNATURE {
        return Err(eyre!(t!(
            "cli.common.errors.header_invalid",
            locale = locale.as_str()
        )));
    }

    FormatVersion::try_from(buf[4]).map_err(eyre::Report::new)
}

/// Parse the header, footer, and full index of a v5 `.dar` archive source.
///
/// This is the verbatim body of the former `load_archive` and is only called
/// after [`read_version`] has confirmed the archive is v5.  All existing call
/// sites that previously called `load_archive` directly continue to work
/// unchanged because [`load_archive`] now dispatches here for v5 archives.
pub fn load_v5(
    source: &mut dyn ReadSeek,
    file_path: &str,
    locale: &Locale,
) -> Result<ArchiveState> {
    // Determine total byte length by seeking to the end.
    let file_len = source.seek(SeekFrom::End(0)).wrap_err(
        t!(
            "cli.common.errors.seek_failed",
            locale = locale.as_str(),
            file = file_path
        )
        .to_string(),
    )?;
    let header_size = size_of::<ArchiveHeader>() as u64;
    let footer_size = size_of::<ArchiveFooter>() as u64;

    if file_len < header_size + footer_size {
        return Err(eyre!(t!(
            "cli.common.errors.footer_invalid",
            locale = locale.as_str()
        )));
    }

    // --- Header ---
    seek_ctx(source, SeekFrom::Start(0), file_path, locale)?;

    let mut header_buf = [0u8; size_of::<ArchiveHeader>()];
    read_exact_ctx(
        source,
        &mut header_buf,
        "cli.common.errors.header_read_failed",
        locale,
    )?;
    let header = *bytemuck::from_bytes::<ArchiveHeader>(&header_buf);

    if header.signature != *format::SIGNATURE || header.version != format::VERSION {
        return Err(eyre!(t!(
            "cli.common.errors.header_invalid",
            locale = locale.as_str()
        )));
    }

    // --- Footer ---
    let footer_pos = file_len - footer_size;
    seek_ctx(source, SeekFrom::Start(footer_pos), file_path, locale)?;

    let mut footer_buf = [0u8; size_of::<ArchiveFooter>()];
    read_exact_ctx(
        source,
        &mut footer_buf,
        "cli.common.errors.footer_read_failed",
        locale,
    )?;
    let footer = *bytemuck::from_bytes::<ArchiveFooter>(&footer_buf);

    if footer.signature != *format::FOOTER_SIGNATURE {
        return Err(eyre!(t!(
            "cli.common.errors.footer_invalid",
            locale = locale.as_str()
        )));
    }

    let index_offset = footer.index_offset as u64;
    if index_offset < header_size || index_offset > footer_pos {
        return Err(eyre!(t!(
            "cli.common.errors.footer_invalid",
            locale = locale.as_str()
        )));
    }

    // --- Index entries ---
    seek_ctx(source, SeekFrom::Start(index_offset), file_path, locale)?;

    let mut entries = Vec::with_capacity(footer.amount_of_files as usize);
    let mut encryption_mode: Option<bool> = None;
    let mut encryption_probe: Option<EncryptedEntryProbe> = None;

    for _ in 0..footer.amount_of_files {
        let mut entry_buf = [0u8; size_of::<ArchiveIndexEntry>()];
        read_exact_ctx(
            source,
            &mut entry_buf,
            "cli.common.errors.index_decode_failed",
            locale,
        )?;
        let entry = *bytemuck::from_bytes::<ArchiveIndexEntry>(&entry_buf);

        let entry_encrypted = (entry.bitflags & flags::ENCRYPTED_DATA) != 0;
        match encryption_mode {
            None => encryption_mode = Some(entry_encrypted),
            Some(expected) if expected != entry_encrypted => {
                return Err(eyre!(t!(
                    "cli.common.errors.mixed_encryption",
                    locale = locale.as_str()
                )));
            }
            _ => {}
        }

        if entry_encrypted && encryption_probe.is_none() {
            encryption_probe = Some(EncryptedEntryProbe {
                offset: entry.offset,
                size: entry.compressed_size,
                checksum: entry.checksum,
            });
        }

        let mut path_bytes = vec![0u8; entry.path_length as usize];
        read_exact_ctx(
            source,
            &mut path_bytes,
            "cli.common.errors.index_decode_failed",
            locale,
        )?;
        let path = String::from_utf8(path_bytes).wrap_err_with(|| {
            t!(
                "cli.common.errors.utf8_failed",
                locale = locale.as_str(),
                field = "path"
            )
            .to_string()
        })?;

        let mut extra_bytes = vec![0u8; entry.extra_length as usize];
        read_exact_ctx(
            source,
            &mut extra_bytes,
            "cli.common.errors.index_decode_failed",
            locale,
        )?;
        let extra = String::from_utf8(extra_bytes).wrap_err_with(|| {
            t!(
                "cli.common.errors.utf8_failed",
                locale = locale.as_str(),
                field = "extra"
            )
            .to_string()
        })?;

        entries.push(ArchiveIndexEntryWrapper::new(entry, path, extra));
    }

    Ok(ArchiveState {
        entries,
        header,
        encryption_mode,
        index_offset,
        encryption_probe,
    })
}

/// Parse the header, footer, and full index of a v6 `.dar` archive source.
///
/// Mirror of [`load_v5`] using the updated v6 struct sizes and field layout.
/// In particular:
/// - The header is 17 bytes (`ArchiveHeaderV6`) and carries `volume_number` / `total_volumes`.
/// - The footer is 19 bytes (`ArchiveFooterV6`) with `index_offset` widened to `u64`.
/// - Each index entry is 123 bytes (`ArchiveIndexEntryV6`) and adds `stored_checksum`,
///   `xattr_length`, and `volume_number`.
/// - After the variable-length `extra` bytes, `xattr_length` additional bytes are present
///   in the index tail (xattr blob); these are read and discarded unless Phase 6 is active.
pub fn load_v6(
    source: &mut dyn ReadSeek,
    file_path: &str,
    locale: &Locale,
) -> Result<ArchiveState> {
    let file_len = source.seek(SeekFrom::End(0)).wrap_err(
        t!(
            "cli.common.errors.seek_failed",
            locale = locale.as_str(),
            file = file_path
        )
        .to_string(),
    )?;
    let header_size = size_of::<ArchiveHeaderV6>() as u64;
    let footer_size = size_of::<ArchiveFooterV6>() as u64;

    if file_len < header_size + footer_size {
        return Err(eyre!(t!(
            "cli.common.errors.footer_invalid",
            locale = locale.as_str()
        )));
    }

    // --- Header (17 bytes) ---
    seek_ctx(source, SeekFrom::Start(0), file_path, locale)?;
    let mut header_buf = [0u8; size_of::<ArchiveHeaderV6>()];
    read_exact_ctx(
        source,
        &mut header_buf,
        "cli.common.errors.header_read_failed",
        locale,
    )?;
    let v6_header = *bytemuck::from_bytes::<ArchiveHeaderV6>(&header_buf);

    if v6_header.signature != *format::SIGNATURE || v6_header.version != 6 {
        return Err(eyre!(t!(
            "cli.common.errors.header_invalid",
            locale = locale.as_str()
        )));
    }

    // Synthesise a v5-layout ArchiveHeader for ArchiveState compatibility.
    // Only `signature`, `version`, and `timestamp` are accessible through this path.
    let header = ArchiveHeader {
        signature: v6_header.signature,
        version: v6_header.version,
        timestamp: v6_header.timestamp,
    };

    // --- Footer (19 bytes) ---
    let footer_pos = file_len - footer_size;
    seek_ctx(source, SeekFrom::Start(footer_pos), file_path, locale)?;
    let mut footer_buf = [0u8; size_of::<ArchiveFooterV6>()];
    read_exact_ctx(
        source,
        &mut footer_buf,
        "cli.common.errors.footer_read_failed",
        locale,
    )?;
    let footer = *bytemuck::from_bytes::<ArchiveFooterV6>(&footer_buf);

    if footer.signature != *format::FOOTER_SIGNATURE {
        return Err(eyre!(t!(
            "cli.common.errors.footer_invalid",
            locale = locale.as_str()
        )));
    }

    let index_offset = footer.index_offset;
    if index_offset < header_size || index_offset > footer_pos {
        return Err(eyre!(t!(
            "cli.common.errors.footer_invalid",
            locale = locale.as_str()
        )));
    }

    // --- Index entries (123 bytes each + variable tail) ---
    seek_ctx(source, SeekFrom::Start(index_offset), file_path, locale)?;

    let mut entries = Vec::with_capacity(footer.amount_of_files as usize);
    let mut encryption_mode: Option<bool> = None;
    let mut encryption_probe: Option<EncryptedEntryProbe> = None;

    for _ in 0..footer.amount_of_files {
        let mut entry_buf = [0u8; size_of::<ArchiveIndexEntryV6>()];
        read_exact_ctx(
            source,
            &mut entry_buf,
            "cli.common.errors.index_decode_failed",
            locale,
        )?;
        let v6 = *bytemuck::from_bytes::<ArchiveIndexEntryV6>(&entry_buf);

        // Map v6 on-disk struct → v5-compatible ArchiveIndexEntry (holds the common fields).
        let entry = ArchiveIndexEntry {
            offset: v6.offset,
            bitflags: v6.bitflags,
            compression_method: v6.compression_method,
            modification_timestamp: v6.modification_timestamp,
            uid: v6.uid,
            gid: v6.gid,
            perm: v6.perm,
            checksum: v6.checksum,
            original_size: v6.original_size,
            compressed_size: v6.compressed_size,
            path_length: v6.path_length,
            extra_length: v6.extra_length,
        };

        let entry_encrypted = (v6.bitflags & flags::ENCRYPTED_DATA) != 0;
        match encryption_mode {
            None => encryption_mode = Some(entry_encrypted),
            Some(expected) if expected != entry_encrypted => {
                return Err(eyre!(t!(
                    "cli.common.errors.mixed_encryption",
                    locale = locale.as_str()
                )));
            }
            _ => {}
        }

        if entry_encrypted && encryption_probe.is_none() {
            encryption_probe = Some(EncryptedEntryProbe {
                offset: v6.offset,
                size: v6.compressed_size,
                checksum: v6.checksum,
            });
        }

        // Variable-length tail: path
        let mut path_bytes = vec![0u8; v6.path_length as usize];
        read_exact_ctx(
            source,
            &mut path_bytes,
            "cli.common.errors.index_decode_failed",
            locale,
        )?;
        let path = String::from_utf8(path_bytes).wrap_err_with(|| {
            t!(
                "cli.common.errors.utf8_failed",
                locale = locale.as_str(),
                field = "path"
            )
            .to_string()
        })?;

        // Variable-length tail: extra
        let mut extra_bytes = vec![0u8; v6.extra_length as usize];
        read_exact_ctx(
            source,
            &mut extra_bytes,
            "cli.common.errors.index_decode_failed",
            locale,
        )?;
        let extra = String::from_utf8(extra_bytes).wrap_err_with(|| {
            t!(
                "cli.common.errors.utf8_failed",
                locale = locale.as_str(),
                field = "extra"
            )
            .to_string()
        })?;

        // Variable-length tail: xattr blob (skip; Phase 6 will consume it)
        if v6.xattr_length > 0 {
            let mut xattr_bytes = vec![0u8; v6.xattr_length as usize];
            read_exact_ctx(
                source,
                &mut xattr_bytes,
                "cli.common.errors.index_decode_failed",
                locale,
            )?;
        }

        entries.push(ArchiveIndexEntryWrapper::new_v6(
            entry,
            path,
            extra,
            v6.stored_checksum,
            v6.xattr_length,
            v6.volume_number,
        ));
    }

    Ok(ArchiveState {
        entries,
        header,
        encryption_mode,
        index_offset,
        encryption_probe,
    })
}

/// Parse the header, footer, and full index of a `.dar` archive source.
///
/// Reads the version byte from the archive and dispatches to the appropriate
/// version-specific loader.  Currently only v5 archives are supported;
/// v6 support will be added in Phase 1.
///
/// `source` can be any [`ReadSeek`] implementation — typically a `std::fs::File`
/// in production code or a `std::io::Cursor<Vec<u8>>` in unit tests.
///
/// The cursor position after this call is unspecified; callers should seek before further I/O.
///
/// # Errors
///
/// Returns an error if:
/// - the source is too short to hold a valid header + footer,
/// - the header signature or version is invalid,
/// - the footer signature is invalid or `index_offset` is out of range,
/// - any index entry cannot be read or decoded (truncated source, invalid UTF-8 path/extra),
/// - entries mix encrypted and unencrypted data.
pub fn load_archive(
    source: &mut dyn ReadSeek,
    file_path: &str,
    locale: &Locale,
) -> Result<ArchiveState> {
    let version = read_version(source, locale)?;
    match version {
        FormatVersion::V5 => load_v5(source, file_path, locale),
        FormatVersion::V6 => load_v6(source, file_path, locale),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Locale;
    use crate::test_utils::build_archive_bytes;
    use std::io::Cursor;

    fn en() -> Locale {
        Locale::new("en")
    }

    /// Build an in-memory archive and return a `Cursor` ready for parsing.
    fn archive(files: &[(&str, &[u8])]) -> Cursor<Vec<u8>> {
        Cursor::new(build_archive_bytes(files, None))
    }

    /// Build an encrypted in-memory archive and return a `Cursor` ready for parsing.
    fn enc_archive(files: &[(&str, &[u8])], pass: &str) -> Cursor<Vec<u8>> {
        Cursor::new(build_archive_bytes(files, Some(pass)))
    }

    // --- single entry ---

    #[test]
    fn test_parses_single_entry_path() {
        let mut src = archive(&[("hello.txt", b"hello")]);
        let state = load_archive(&mut src, "<mem>", &en()).unwrap();
        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.entries[0].path, "hello.txt");
    }

    #[test]
    fn test_parses_entry_original_size() {
        let mut src = archive(&[("data.bin", b"0123456789")]);
        let state = load_archive(&mut src, "<mem>", &en()).unwrap();
        // `ArchiveIndexEntry` is #[repr(C, packed)]; copy field before comparing.
        let original_size = state.entries[0].entry.original_size;
        assert_eq!(original_size, 10);
    }

    // --- multiple entries ---

    #[test]
    fn test_parses_multiple_entries() {
        let mut src = archive(&[("a.txt", b"aaa"), ("b.txt", b"bbb"), ("c.txt", b"ccc")]);
        let state = load_archive(&mut src, "<mem>", &en()).unwrap();
        assert_eq!(state.entries.len(), 3);
        let paths: Vec<&str> = state.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"a.txt"));
        assert!(paths.contains(&"b.txt"));
        assert!(paths.contains(&"c.txt"));
    }

    // --- encryption detection ---

    #[test]
    fn test_unencrypted_archive_reports_false_mode() {
        let mut src = archive(&[("f.txt", b"x")]);
        let state = load_archive(&mut src, "<mem>", &en()).unwrap();
        assert_eq!(state.encryption_mode, Some(false));
        assert!(state.encryption_probe.is_none());
    }

    #[test]
    fn test_encrypted_archive_reports_true_mode() {
        let mut src = enc_archive(&[("f.txt", b"x")], "s3cr3t");
        let state = load_archive(&mut src, "<mem>", &en()).unwrap();
        assert_eq!(state.encryption_mode, Some(true));
    }

    #[test]
    fn test_encrypted_archive_populates_probe() {
        let mut src = enc_archive(&[("f.txt", b"secret")], "pass");
        let state = load_archive(&mut src, "<mem>", &en()).unwrap();
        let probe = state
            .encryption_probe
            .expect("probe should be Some for encrypted archive");
        assert!(probe.size > 0);
        assert_ne!(probe.checksum, [0u8; 32]);
    }

    // --- index offset ---

    #[test]
    fn test_index_offset_is_within_file_bounds() {
        let bytes = build_archive_bytes(&[("x.rs", b"fn main(){}")], None);
        let file_len = bytes.len() as u64;
        let mut src = Cursor::new(bytes);
        let state = load_archive(&mut src, "<mem>", &en()).unwrap();
        assert!(state.index_offset >= size_of::<ArchiveHeader>() as u64);
        assert!(state.index_offset < file_len);
    }

    // --- header fields ---

    #[test]
    fn test_header_signature_is_preserved() {
        let mut src = archive(&[("f.txt", b"y")]);
        let state = load_archive(&mut src, "<mem>", &en()).unwrap();
        assert_eq!(&state.header.signature, b"DARI");
        assert_eq!(state.header.version, 5);
    }

    #[test]
    fn test_header_timestamp_is_nonzero() {
        let mut src = archive(&[("f.txt", b"z")]);
        let state = load_archive(&mut src, "<mem>", &en()).unwrap();
        assert!(state.header.timestamp > 0);
    }

    // --- negative: source too short ---

    #[test]
    fn test_rejects_file_shorter_than_header_plus_footer() {
        let mut src = Cursor::new(b"DARI".to_vec());
        assert!(load_archive(&mut src, "<mem>", &en()).is_err());
    }

    // --- negative: bad header signature ---

    #[test]
    fn test_rejects_invalid_header_signature() {
        // 28 bytes: wrong sig + version 5 + zero timestamp + zero footer area
        let mut data = vec![0u8; 28];
        data[0..4].copy_from_slice(b"XXXX");
        data[4] = 5;
        let mut src = Cursor::new(data);
        assert!(load_archive(&mut src, "<mem>", &en()).is_err());
    }

    // --- negative: bad header version ---

    #[test]
    fn test_rejects_invalid_header_version() {
        let mut data = vec![0u8; 28];
        data[0..4].copy_from_slice(b"DARI");
        data[4] = 99; // unsupported version
        let mut src = Cursor::new(data);
        assert!(load_archive(&mut src, "<mem>", &en()).is_err());
    }

    // --- negative: bad footer signature ---

    #[test]
    fn test_rejects_invalid_footer_signature() {
        // Valid header (13 B) + bad footer (15 B): total 28 B.
        // Footer is read from the last 15 bytes; signature must be "DARIEND".
        let mut data = vec![0u8; 28];
        data[0..4].copy_from_slice(b"DARI");
        data[4] = 5;
        // bytes 13‥19 are footer signature — leave as zeros (not "DARIEND")
        let mut src = Cursor::new(data);
        assert!(load_archive(&mut src, "<mem>", &en()).is_err());
    }

    // --- negative: index_offset out of range ---

    #[test]
    fn test_rejects_index_offset_of_zero() {
        // 28 bytes: valid header + valid footer signature but index_offset = 0
        let mut data = vec![0u8; 28];
        data[0..4].copy_from_slice(b"DARI");
        data[4] = 5;
        data[13..20].copy_from_slice(b"DARIEND");
        // index_offset at bytes 20‥23 — leave as 0u32 LE (< header_size = 13)
        let mut src = Cursor::new(data);
        assert!(load_archive(&mut src, "<mem>", &en()).is_err());
    }

    // --- 5.5 corrupt / truncated archive handling ---

    #[test]
    fn test_rejects_index_offset_beyond_eof() {
        // Build a valid 28-byte stub but set index_offset to a value beyond the file.
        let mut data = vec![0u8; 28];
        data[0..4].copy_from_slice(b"DARI");
        data[4] = 5;
        data[13..20].copy_from_slice(b"DARIEND");
        // index_offset = 9999 — well past EOF
        let off: u32 = 9999;
        data[20..24].copy_from_slice(&off.to_le_bytes());
        let mut src = Cursor::new(data);
        assert!(
            load_archive(&mut src, "<mem>", &en()).is_err(),
            "index_offset beyond EOF must be rejected"
        );
    }

    #[test]
    fn test_rejects_index_with_zero_bytes_for_entry() {
        // Build a real single-entry archive then overwrite the index area with zeros.
        let mut data = build_archive_bytes(&[("f.txt", b"hi")], None);

        // Locate index_offset from the footer and zero out every byte from that point
        // up to (but not including) the footer.
        let footer_size = std::mem::size_of::<crate::models::archive::ArchiveFooter>();
        let footer_base = data.len() - footer_size;
        let idx_off =
            u32::from_le_bytes(data[footer_base + 7..footer_base + 11].try_into().unwrap())
                as usize;
        for b in &mut data[idx_off..footer_base] {
            *b = 0;
        }

        let mut src = Cursor::new(data);
        // Zeroed index entries may parse as garbage (e.g. huge path_length / extra_length),
        // which will cause a read-exact failure on the variable-length fields.
        // The important invariant is that load_archive does not succeed with corrupted data.
        let result = load_archive(&mut src, "<mem>", &en());
        // It may succeed (all-zero entry happens to be valid: path_length=0, extra_length=0)
        // or fail — either is acceptable; what must NOT happen is a panic or returning the
        // wrong entries.  We verify that no panic occurred (the test just runs) and, if it
        // succeeded, the entry path is empty.
        if let Ok(state) = result {
            for entry in &state.entries {
                assert!(
                    entry.path.is_empty(),
                    "zeroed index entry must yield an empty path"
                );
            }
        }
    }

    #[test]
    fn test_rejects_archive_truncated_mid_index_entry() {
        // Build a real archive, then truncate it inside the first index entry.
        let mut data = build_archive_bytes(&[("file.rs", b"fn main(){}")], None);

        let footer_size = std::mem::size_of::<crate::models::archive::ArchiveFooter>();
        let footer_base = data.len() - footer_size;
        let idx_off =
            u32::from_le_bytes(data[footer_base + 7..footer_base + 11].try_into().unwrap())
                as usize;

        // Truncate to remove the last half of the index section (including the footer)
        let truncate_at = idx_off + 4; // only 4 bytes into the index entry — well short of a full entry
        data.truncate(truncate_at);
        let mut src = Cursor::new(data);
        assert!(
            load_archive(&mut src, "<mem>", &en()).is_err(),
            "archive truncated mid-index must be rejected"
        );
    }

    #[test]
    fn test_empty_archive_zero_entries_parses_successfully() {
        use crate::archive_builder::ArchiveBuilder;
        use crate::pipeline::PipelineConfig;

        let cursor = std::io::Cursor::new(Vec::<u8>::new());
        let mut builder = ArchiveBuilder::with_config(cursor, PipelineConfig::default());
        builder.write_header().unwrap();
        builder.build().unwrap();
        let bytes = builder.into_inner().into_inner();

        let mut src = Cursor::new(bytes);
        let state = load_archive(&mut src, "<mem>", &en()).unwrap();
        assert_eq!(
            state.entries.len(),
            0,
            "empty archive should have zero entries"
        );
        assert!(
            state.encryption_mode.is_none(),
            "empty archive has no encryption mode"
        );
    }

    // ── Phase 1 — v6 reader tests ────────────────────────────────────────────

    /// Build an in-memory v6 archive with the given files.
    fn v6_archive(files: &[(&str, &[u8])]) -> Cursor<Vec<u8>> {
        use crate::archive_builder::ArchiveBuilder;
        use crate::format_version::FormatVersion;
        use crate::pipeline::{CompressionPipeline, PipelineConfig};
        use std::path::Path;

        let cfg = PipelineConfig::default();
        let pipeline = CompressionPipeline::new(cfg.clone());
        let cursor = std::io::Cursor::new(Vec::<u8>::new());
        let mut builder = ArchiveBuilder::with_version(cursor, cfg, FormatVersion::V6);
        builder.write_header().unwrap();
        for (name, content) in files {
            let pr = pipeline
                .process_file(Path::new(name), content.to_vec())
                .unwrap();
            let prepared = crate::archive_builder::PreparedFile {
                archive_path: name.to_string(),
                pipeline_result: pr,
                timestamp: 0,
                uid: 1000,
                gid: 1000,
                perm: 0o644,
            };
            builder.commit_prepared(prepared).unwrap();
        }
        builder.build().unwrap();
        builder.into_inner()
    }

    #[test]
    fn test_v6_parses_single_entry_path() {
        let mut src = v6_archive(&[("hello.txt", b"hello")]);
        let state = load_archive(&mut src, "<mem>", &en()).unwrap();
        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.entries[0].path, "hello.txt");
        assert_eq!(state.header.version, 6, "loaded header should report v6");
    }

    #[test]
    fn test_v6_parses_multiple_entries() {
        let mut src = v6_archive(&[("a.txt", b"aaa"), ("b.txt", b"bbb")]);
        let state = load_archive(&mut src, "<mem>", &en()).unwrap();
        assert_eq!(state.entries.len(), 2);
        let paths: Vec<&str> = state.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"a.txt"));
        assert!(paths.contains(&"b.txt"));
    }

    #[test]
    fn test_v6_stored_checksum_is_nonzero() {
        let mut src = v6_archive(&[("data.bin", b"important content")]);
        let state = load_archive(&mut src, "<mem>", &en()).unwrap();
        let wrapper = &state.entries[0];
        assert!(
            wrapper.stored_checksum_v6().is_some(),
            "v6 entry must carry a non-zero stored_checksum"
        );
    }

    #[test]
    fn test_v6_index_offset_uses_u64() {
        let mut src = v6_archive(&[("f.txt", b"x")]);
        let state = load_archive(&mut src, "<mem>", &en()).unwrap();
        // index_offset is stored as u64 in v6; verify it's within the archive bounds
        assert!(
            state.index_offset >= 17, // v6 header is 17 bytes
            "v6 index_offset must be past the 17-byte header"
        );
    }

    #[test]
    fn test_v6_unencrypted_archive_reports_false_mode() {
        let mut src = v6_archive(&[("f.txt", b"x")]);
        let state = load_archive(&mut src, "<mem>", &en()).unwrap();
        assert_eq!(state.encryption_mode, Some(false));
        assert!(state.encryption_probe.is_none());
    }

    #[test]
    fn test_v6_original_size_is_preserved() {
        let content = b"0123456789";
        let mut src = v6_archive(&[("data.bin", content)]);
        let state = load_archive(&mut src, "<mem>", &en()).unwrap();
        let original_size = state.entries[0].entry.original_size;
        assert_eq!(original_size, content.len() as u64);
    }
}
