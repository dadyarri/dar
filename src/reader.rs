use crate::i18n::Locale;
use crate::models::archive::{
    ArchiveFooter, ArchiveHeader, ArchiveIndexEntry, ArchiveIndexEntryWrapper,
};
use crate::pipeline::INDEX_FLAG_ENCRYPTED_DATA;
use eyre::{Context, Result, eyre};
use rust_i18n::t;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::mem::size_of;

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

/// Parse the header, footer, and full index of a `.dar` file.
///
/// The file cursor position after this call is unspecified; callers should seek before further I/O.
pub fn load_archive(file: &mut File, file_path: &str, locale: &Locale) -> Result<ArchiveState> {
    let metadata = file.metadata().wrap_err(
        t!(
            "cli.common.errors.read_failed",
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
            "cli.common.errors.footer_invalid",
            locale = locale.as_str()
        )));
    }

    // --- Header ---
    file.seek(SeekFrom::Start(0)).wrap_err(
        t!(
            "cli.common.errors.seek_failed",
            locale = locale.as_str(),
            file = file_path
        )
        .to_string(),
    )?;

    let mut header_buf = [0u8; size_of::<ArchiveHeader>()];
    file.read_exact(&mut header_buf).wrap_err(
        t!(
            "cli.common.errors.header_read_failed",
            locale = locale.as_str(),
            file = file_path
        )
        .to_string(),
    )?;
    let header = *bytemuck::from_bytes::<ArchiveHeader>(&header_buf);

    if header.signature != *b"DARI" || header.version != 5 {
        return Err(eyre!(t!(
            "cli.common.errors.header_invalid",
            locale = locale.as_str()
        )));
    }

    // --- Footer ---
    let footer_pos = file_len - footer_size;
    file.seek(SeekFrom::Start(footer_pos)).wrap_err(
        t!(
            "cli.common.errors.seek_failed",
            locale = locale.as_str(),
            file = file_path
        )
        .to_string(),
    )?;

    let mut footer_buf = [0u8; size_of::<ArchiveFooter>()];
    file.read_exact(&mut footer_buf).wrap_err(
        t!(
            "cli.common.errors.footer_read_failed",
            locale = locale.as_str(),
            file = file_path
        )
        .to_string(),
    )?;
    let footer = *bytemuck::from_bytes::<ArchiveFooter>(&footer_buf);

    if footer.signature != *b"DARIEND" {
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
    file.seek(SeekFrom::Start(index_offset)).wrap_err(
        t!(
            "cli.common.errors.seek_failed",
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
                "cli.common.errors.index_decode_failed",
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
        file.read_exact(&mut path_bytes).wrap_err(
            t!(
                "cli.common.errors.index_decode_failed",
                locale = locale.as_str()
            )
            .to_string(),
        )?;
        let path = String::from_utf8(path_bytes).map_err(|_| {
            eyre!(t!(
                "cli.common.errors.utf8_failed",
                locale = locale.as_str(),
                field = "path"
            ))
        })?;

        let mut extra_bytes = vec![0u8; entry.extra_length as usize];
        file.read_exact(&mut extra_bytes).wrap_err(
            t!(
                "cli.common.errors.index_decode_failed",
                locale = locale.as_str()
            )
            .to_string(),
        )?;
        let extra = String::from_utf8(extra_bytes).map_err(|_| {
            eyre!(t!(
                "cli.common.errors.utf8_failed",
                locale = locale.as_str(),
                field = "extra"
            ))
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Locale;
    use crate::test_utils::build_archive;
    use std::fs::File;

    fn en() -> Locale {
        Locale::new("en")
    }

    // --- single entry ---

    #[test]
    fn test_parses_single_entry_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = build_archive(&dir, "a.dar", &[("hello.txt", b"hello")], None);
        let mut f = File::open(&path).unwrap();
        let state = load_archive(&mut f, path.to_str().unwrap(), &en()).unwrap();
        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.entries[0].path, "hello.txt");
    }

    #[test]
    fn test_parses_entry_original_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = build_archive(&dir, "b.dar", &[("data.bin", b"0123456789")], None);
        let mut f = File::open(&path).unwrap();
        let state = load_archive(&mut f, path.to_str().unwrap(), &en()).unwrap();
        // `ArchiveIndexEntry` is #[repr(C, packed)]; copy field before comparing.
        let original_size = state.entries[0].entry.original_size;
        assert_eq!(original_size, 10);
    }

    // --- multiple entries ---

    #[test]
    fn test_parses_multiple_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = build_archive(
            &dir,
            "multi.dar",
            &[("a.txt", b"aaa"), ("b.txt", b"bbb"), ("c.txt", b"ccc")],
            None,
        );
        let mut f = File::open(&path).unwrap();
        let state = load_archive(&mut f, path.to_str().unwrap(), &en()).unwrap();
        assert_eq!(state.entries.len(), 3);
        let paths: Vec<&str> = state.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"a.txt"));
        assert!(paths.contains(&"b.txt"));
        assert!(paths.contains(&"c.txt"));
    }

    // --- encryption detection ---

    #[test]
    fn test_unencrypted_archive_reports_false_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = build_archive(&dir, "plain.dar", &[("f.txt", b"x")], None);
        let mut f = File::open(&path).unwrap();
        let state = load_archive(&mut f, path.to_str().unwrap(), &en()).unwrap();
        assert_eq!(state.encryption_mode, Some(false));
        assert!(state.encryption_probe.is_none());
    }

    #[test]
    fn test_encrypted_archive_reports_true_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = build_archive(&dir, "enc.dar", &[("f.txt", b"x")], Some("s3cr3t"));
        let mut f = File::open(&path).unwrap();
        let state = load_archive(&mut f, path.to_str().unwrap(), &en()).unwrap();
        assert_eq!(state.encryption_mode, Some(true));
    }

    #[test]
    fn test_encrypted_archive_populates_probe() {
        let dir = tempfile::tempdir().unwrap();
        let path = build_archive(&dir, "enc2.dar", &[("f.txt", b"secret")], Some("pass"));
        let mut f = File::open(&path).unwrap();
        let state = load_archive(&mut f, path.to_str().unwrap(), &en()).unwrap();
        let probe = state
            .encryption_probe
            .expect("probe should be Some for encrypted archive");
        assert!(probe.size > 0);
        assert_ne!(probe.checksum, [0u8; 32]);
    }

    // --- index offset ---

    #[test]
    fn test_index_offset_is_within_file_bounds() {
        let dir = tempfile::tempdir().unwrap();
        let path = build_archive(&dir, "off.dar", &[("x.rs", b"fn main(){}")], None);
        let file_len = std::fs::metadata(&path).unwrap().len();
        let mut f = File::open(&path).unwrap();
        let state = load_archive(&mut f, path.to_str().unwrap(), &en()).unwrap();
        assert!(state.index_offset >= size_of::<ArchiveHeader>() as u64);
        assert!(state.index_offset < file_len);
    }

    // --- header fields ---

    #[test]
    fn test_header_signature_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = build_archive(&dir, "hdr.dar", &[("f.txt", b"y")], None);
        let mut f = File::open(&path).unwrap();
        let state = load_archive(&mut f, path.to_str().unwrap(), &en()).unwrap();
        assert_eq!(&state.header.signature, b"DARI");
        assert_eq!(state.header.version, 5);
    }

    #[test]
    fn test_header_timestamp_is_nonzero() {
        let dir = tempfile::tempdir().unwrap();
        let path = build_archive(&dir, "ts.dar", &[("f.txt", b"z")], None);
        let mut f = File::open(&path).unwrap();
        let state = load_archive(&mut f, path.to_str().unwrap(), &en()).unwrap();
        assert!(state.header.timestamp > 0);
    }

    // --- negative: file too short ---

    #[test]
    fn test_rejects_file_shorter_than_header_plus_footer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("short.dar");
        std::fs::write(&path, b"DARI").unwrap();
        let mut f = File::open(&path).unwrap();
        assert!(load_archive(&mut f, path.to_str().unwrap(), &en()).is_err());
    }

    // --- negative: bad header signature ---

    #[test]
    fn test_rejects_invalid_header_signature() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("badsig.dar");
        // 28 bytes: wrong sig + version 5 + zero timestamp + zero footer area
        let mut data = vec![0u8; 28];
        data[0..4].copy_from_slice(b"XXXX");
        data[4] = 5;
        std::fs::write(&path, &data).unwrap();
        let mut f = File::open(&path).unwrap();
        assert!(load_archive(&mut f, path.to_str().unwrap(), &en()).is_err());
    }

    // --- negative: bad header version ---

    #[test]
    fn test_rejects_invalid_header_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("badver.dar");
        let mut data = vec![0u8; 28];
        data[0..4].copy_from_slice(b"DARI");
        data[4] = 99; // unsupported version
        std::fs::write(&path, &data).unwrap();
        let mut f = File::open(&path).unwrap();
        assert!(load_archive(&mut f, path.to_str().unwrap(), &en()).is_err());
    }

    // --- negative: bad footer signature ---

    #[test]
    fn test_rejects_invalid_footer_signature() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("badftr.dar");
        // Valid header (13 B) + bad footer (15 B): total 28 B.
        // Footer is read from the last 15 bytes; signature must be "DARIEND".
        let mut data = vec![0u8; 28];
        data[0..4].copy_from_slice(b"DARI");
        data[4] = 5;
        // bytes 13‥19 are footer signature — leave as zeros (not "DARIEND")
        std::fs::write(&path, &data).unwrap();
        let mut f = File::open(&path).unwrap();
        assert!(load_archive(&mut f, path.to_str().unwrap(), &en()).is_err());
    }

    // --- negative: index_offset out of range ---

    #[test]
    fn test_rejects_index_offset_of_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("badoff.dar");
        // 28 bytes: valid header + valid footer signature but index_offset = 0
        let mut data = vec![0u8; 28];
        data[0..4].copy_from_slice(b"DARI");
        data[4] = 5;
        data[13..20].copy_from_slice(b"DARIEND");
        // index_offset at bytes 20‥23 — leave as 0u32 LE (< header_size = 13)
        std::fs::write(&path, &data).unwrap();
        let mut f = File::open(&path).unwrap();
        assert!(load_archive(&mut f, path.to_str().unwrap(), &en()).is_err());
    }
}
