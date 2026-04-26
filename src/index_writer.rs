//! External index file (`.dari`) writer and path-resolution helpers.
//!
//! The `.dari` file stores the same v6 index records as are embedded in v6 `.dar`
//! archives, but in a separate file that allows metadata operations (`inspect`,
//! `list`, selective `extract`) to avoid opening large data volumes.
//!
//! ## File layout
//!
//! ```text
//! IndexFileHeader  : 17 bytes  (DARIDX + version[1] + archive_timestamp[8] + total_volumes[2])
//! IndexEntry*      : N × (ArchiveIndexEntryV6 + path_bytes + extra_bytes + xattr_bytes)
//! IndexFileFooter  : 45 bytes  (DARIDXEND + entry_count[4] + BLAKE3[32])
//! ```
//!
//! The BLAKE3 checksum in `IndexFileFooter` covers **all bytes from offset 0 up to
//! (but not including) the footer itself**, giving the `.dari` file self-contained
//! integrity.

use crate::models::archive::{ArchiveIndexEntryV6, ArchiveIndexEntryWrapper};
use crate::xattrs::encode_xattr_blob;
use bytemuck::{Pod, Zeroable, bytes_of};
use eyre::{Context, Result};
use rust_i18n::t;
use std::fs::File;
use std::io::{BufWriter, Seek, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotEntry {
    pub path: String,
    pub checksum: [u8; 32],
    pub modification_timestamp: u64,
}

// ── Constants ─────────────────────────────────────────────────────────────────

/// Magic bytes at the start of a `.dari` index file.
pub const INDEX_SIGNATURE: &[u8; 6] = b"DARIDX";

/// Index format version stored in [`IndexFileHeader::version`].
pub const INDEX_VERSION: u8 = 1;

/// Magic bytes at the start of [`IndexFileFooter`].
pub const INDEX_FOOTER_SIGNATURE: &[u8; 9] = b"DARIDXEND";

// ── On-disk structs ───────────────────────────────────────────────────────────

/// On-disk header for the `.dari` external index file — **17 bytes**.
///
/// | Field               | Type      | Bytes |
/// |---------------------|-----------|-------|
/// | `signature`         | `[u8; 6]` | 6     |
/// | `version`           | `u8`      | 1     |
/// | `archive_timestamp` | `u64`     | 8     |
/// | `total_volumes`     | `u16`     | 2     |
#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct IndexFileHeader {
    pub signature: [u8; 6],
    /// Always [`INDEX_VERSION`] (`1`) for the current format.
    pub version: u8,
    /// Copied from `ArchiveHeader::timestamp` (or `ArchiveHeaderV6::timestamp`)
    /// of the first archive volume.  Readers compare this against the live
    /// archive header to detect a stale index.
    pub archive_timestamp: u64,
    /// Number of data volumes this index covers (`1` for single-file archives).
    pub total_volumes: u16,
}

unsafe impl Pod for IndexFileHeader {}
unsafe impl Zeroable for IndexFileHeader {}

/// On-disk footer for the `.dari` external index file — **45 bytes**.
///
/// | Field         | Type       | Bytes |
/// |---------------|------------|-------|
/// | `signature`   | `[u8; 9]`  | 9     |
/// | `entry_count` | `u32`      | 4     |
/// | `checksum`    | `[u8; 32]` | 32    |
#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct IndexFileFooter {
    pub signature: [u8; 9],
    /// Total number of index entries written to this file.
    pub entry_count: u32,
    /// BLAKE3 of all bytes from file offset 0 up to (not including) this footer.
    pub checksum: [u8; 32],
}

unsafe impl Pod for IndexFileFooter {}
unsafe impl Zeroable for IndexFileFooter {}

// ── Path helper ───────────────────────────────────────────────────────────────

/// Compute the candidate external index path for `archive_path`.
///
/// Strips any trailing `.NNN` (three-digit numeric) volume suffix, then changes
/// the file extension to `.dari`:
///
/// - `archive.dar`     → `archive.dari`
/// - `archive.dar.001` → `archive.dari`
/// - `archive.dar.002` → `archive.dari`
pub fn index_path_for_archive(archive_path: &Path) -> PathBuf {
    // Strip a three-digit-numeric extension (volume suffix) if present.
    let base = match archive_path.extension() {
        Some(ext) => {
            let s = ext.to_string_lossy();
            if s.len() == 3 && s.chars().all(|c| c.is_ascii_digit()) {
                // e.g. "archive.dar.001" → "archive.dar"
                archive_path.with_extension("")
            } else {
                archive_path.to_path_buf()
            }
        }
        None => archive_path.to_path_buf(),
    };
    // Replace extension with "dari" (e.g. "archive.dar" → "archive.dari").
    base.with_extension("dari")
}

// ── IndexWriter ───────────────────────────────────────────────────────────────

/// Streaming writer for the `.dari` external index file.
///
/// Write the header immediately on construction via [`IndexWriter::new`], append
/// entries one-by-one with [`IndexWriter::write_entry`], then call
/// [`IndexWriter::finish`] to flush and close with the self-integrity footer.
pub struct IndexWriter {
    path: PathBuf,
    writer: BufWriter<File>,
    entry_count: u32,
    total_volumes: u16,
    snapshots: Vec<SnapshotEntry>,
}

impl IndexWriter {
    /// Create a new index file at `path` and write [`IndexFileHeader`].
    ///
    /// `archive_timestamp` must match `ArchiveHeader::timestamp` (or
    /// `ArchiveHeaderV6::timestamp`) of the corresponding archive so that
    /// readers can detect a stale index.
    pub fn new(path: &Path, archive_timestamp: u64, total_volumes: u16) -> Result<Self> {
        let file = File::create(path).wrap_err_with(|| {
            t!(
                "cli.common.errors.index_file_create_failed",
                file = path.display().to_string()
            )
            .to_string()
        })?;
        let mut writer = BufWriter::new(file);
        let header = IndexFileHeader {
            signature: *INDEX_SIGNATURE,
            version: INDEX_VERSION,
            archive_timestamp,
            total_volumes,
        };
        let header_bytes = bytes_of(&header);
        writer
            .write_all(header_bytes)
            .wrap_err(t!("cli.common.errors.index_file_header_write_failed"))?;

        Ok(Self {
            path: path.to_path_buf(),
            writer,
            entry_count: 0,
            total_volumes,
            snapshots: Vec::new(),
        })
    }

    /// Append one archive entry (serialised as a v6 index record) to the file.
    pub fn write_entry(&mut self, wrapper: &ArchiveIndexEntryWrapper) -> Result<()> {
        let e = wrapper.entry;
        let v6_entry = ArchiveIndexEntryV6 {
            offset: e.offset,
            bitflags: e.bitflags,
            compression_method: e.compression_method,
            modification_timestamp: e.modification_timestamp,
            uid: e.uid,
            gid: e.gid,
            perm: e.perm,
            checksum: e.checksum,
            stored_checksum: wrapper.stored_checksum,
            original_size: e.original_size,
            compressed_size: e.compressed_size,
            path_length: e.path_length,
            extra_length: e.extra_length,
            xattr_length: wrapper.xattr_length,
            volume_number: wrapper.volume_number,
        };

        let entry_bytes = bytes_of(&v6_entry);
        self.writer
            .write_all(entry_bytes)
            .wrap_err(t!("cli.common.errors.index_file_entry_write_failed"))?;

        let path_bytes = wrapper.path.as_bytes();
        self.writer
            .write_all(path_bytes)
            .wrap_err(t!("cli.common.errors.index_file_entry_path_write_failed"))?;

        let extra_bytes = wrapper.extra.as_bytes();
        self.writer
            .write_all(extra_bytes)
            .wrap_err(t!("cli.common.errors.index_file_entry_extra_write_failed"))?;
        let xattr_bytes = encode_xattr_blob(&wrapper.xattrs)?;
        self.writer
            .write_all(&xattr_bytes)
            .wrap_err(t!("cli.common.errors.index_file_entry_extra_write_failed"))?;

        self.snapshots.push(SnapshotEntry {
            path: wrapper.path.clone(),
            checksum: e.checksum,
            modification_timestamp: e.modification_timestamp,
        });

        self.entry_count += 1;
        Ok(())
    }

    /// Update the `total_volumes` field in the already-written header.
    pub fn set_total_volumes(&mut self, total_volumes: u16) -> Result<()> {
        self.total_volumes = total_volumes;
        Ok(())
    }

    /// Finalise the index file: write [`IndexFileFooter`] (BLAKE3 of all preceding bytes) and flush.
    pub fn finish(mut self) -> Result<()> {
        self.writer
            .flush()
            .wrap_err(t!("cli.common.errors.index_file_flush_before_patch_failed"))?;
        self.writer
            .seek(std::io::SeekFrom::Start(15))
            .wrap_err(t!("cli.common.errors.index_file_total_volumes_seek_failed"))?;
        self.writer
            .write_all(&self.total_volumes.to_le_bytes())
            .wrap_err(t!(
                "cli.common.errors.index_file_total_volumes_patch_failed"
            ))?;
        self.writer.flush().wrap_err(t!(
            "cli.common.errors.index_file_flush_patched_header_failed"
        ))?;
        self.writer
            .seek(std::io::SeekFrom::End(0))
            .wrap_err(t!("cli.common.errors.index_file_seek_end_failed"))?;
        self.writer
            .write_all(&[1u8])
            .wrap_err(t!("cli.common.errors.index_file_entry_write_failed"))?;
        for snapshot in &self.snapshots {
            let path_bytes = snapshot.path.as_bytes();
            self.writer
                .write_all(&(path_bytes.len() as u32).to_le_bytes())
                .wrap_err(t!("cli.common.errors.index_file_entry_write_failed"))?;
            self.writer
                .write_all(path_bytes)
                .wrap_err(t!("cli.common.errors.index_file_entry_path_write_failed"))?;
            self.writer
                .write_all(&snapshot.checksum)
                .wrap_err(t!("cli.common.errors.index_file_entry_write_failed"))?;
            self.writer
                .write_all(&snapshot.modification_timestamp.to_le_bytes())
                .wrap_err(t!("cli.common.errors.index_file_entry_write_failed"))?;
        }
        self.writer
            .flush()
            .wrap_err(t!("cli.common.errors.index_file_flush_before_patch_failed"))?;

        let checksum = *blake3::hash(
            &std::fs::read(&self.path)
                .wrap_err(t!("cli.common.errors.index_file_read_for_checksum_failed"))?,
        )
        .as_bytes();
        let footer = IndexFileFooter {
            signature: *INDEX_FOOTER_SIGNATURE,
            entry_count: self.entry_count,
            checksum,
        };
        self.writer
            .write_all(bytes_of(&footer))
            .wrap_err(t!("cli.common.errors.index_file_footer_write_failed"))?;
        self.writer
            .flush()
            .wrap_err(t!("cli.common.errors.index_file_flush_failed"))?;
        Ok(())
    }

    /// Returns the filesystem path of the index file being written.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Locale;
    use crate::reader::load_index;
    use std::mem::size_of;

    #[test]
    fn test_index_file_header_size() {
        assert_eq!(
            size_of::<IndexFileHeader>(),
            17,
            "IndexFileHeader must be 17 bytes"
        );
    }

    #[test]
    fn test_index_file_footer_size() {
        assert_eq!(
            size_of::<IndexFileFooter>(),
            45,
            "IndexFileFooter must be 45 bytes"
        );
    }

    #[test]
    fn test_index_path_for_archive_plain_dar() {
        let idx = index_path_for_archive(Path::new("/tmp/archive.dar"));
        assert_eq!(idx, Path::new("/tmp/archive.dari"));
    }

    #[test]
    fn test_index_path_for_archive_volume_001() {
        let idx = index_path_for_archive(Path::new("/tmp/archive.dar.001"));
        assert_eq!(idx, Path::new("/tmp/archive.dari"));
    }

    #[test]
    fn test_index_path_for_archive_volume_002() {
        let idx = index_path_for_archive(Path::new("backup.dar.002"));
        assert_eq!(idx, Path::new("backup.dari"));
    }

    #[test]
    fn test_index_path_for_archive_no_extension() {
        let idx = index_path_for_archive(Path::new("archive"));
        assert_eq!(idx, Path::new("archive.dari"));
    }

    #[test]
    fn test_write_empty_index_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let idx_path = dir.path().join("empty.dari");

        let archive_ts = 1_700_000_000u64;
        let iw = IndexWriter::new(&idx_path, archive_ts, 1).unwrap();
        iw.finish().unwrap();

        let mut f = std::fs::File::open(&idx_path).unwrap();
        let locale = Locale::new("en");
        let state = load_index(&mut f, idx_path.to_str().unwrap(), &locale).unwrap();

        assert_eq!(state.entries.len(), 0);
        let ts = state.header.timestamp;
        assert_eq!(ts, archive_ts);
        assert_eq!(state.header.version, 6);
    }

    #[test]
    fn test_write_one_entry_and_read_back() {
        use crate::i18n::Locale;
        use crate::models::archive::{
            ArchiveIndexEntry, ArchiveIndexEntryWrapper, CompressionMethod,
        };
        use crate::reader::load_index;

        let dir = tempfile::tempdir().unwrap();
        let idx_path = dir.path().join("single.dari");
        let archive_ts = 1_700_000_001u64;

        let mut iw = IndexWriter::new(&idx_path, archive_ts, 1).unwrap();

        let entry = ArchiveIndexEntry {
            offset: 1024,
            bitflags: 0,
            compression_method: CompressionMethod::None,
            modification_timestamp: 100,
            uid: 1000,
            gid: 1000,
            perm: 0o644,
            checksum: [0xab; 32],
            original_size: 42,
            compressed_size: 42,
            path_length: 8, // "test.txt"
            extra_length: 0,
        };
        let wrapper = ArchiveIndexEntryWrapper::new_v6(
            entry,
            "test.txt".to_string(),
            String::new(),
            vec![("user.dari.test".to_string(), b"value".to_vec())],
            [0xcd; 32],
            25,
            0,
        );
        iw.write_entry(&wrapper).unwrap();
        iw.finish().unwrap();

        let mut f = std::fs::File::open(&idx_path).unwrap();
        let locale = Locale::new("en");
        let state = load_index(&mut f, idx_path.to_str().unwrap(), &locale).unwrap();

        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.snapshots.len(), 1);
        assert_eq!(state.entries[0].path, "test.txt");
        assert_eq!(state.snapshots[0].path, "test.txt");
        assert_eq!(
            state.entries[0].xattrs,
            vec![("user.dari.test".to_string(), b"value".to_vec())]
        );
        let ts = state.header.timestamp;
        assert_eq!(ts, archive_ts);
    }

    #[test]
    fn test_corrupted_checksum_is_rejected() {
        use crate::i18n::Locale;
        use crate::reader::load_index;

        let dir = tempfile::tempdir().unwrap();
        let idx_path = dir.path().join("corrupt.dari");

        let iw = IndexWriter::new(&idx_path, 1_700_000_002u64, 1).unwrap();
        iw.finish().unwrap();

        // Flip a byte in the BLAKE3 checksum within the footer.
        let mut bytes = std::fs::read(&idx_path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        std::fs::write(&idx_path, &bytes).unwrap();

        let mut f = std::fs::File::open(&idx_path).unwrap();
        let locale = Locale::new("en");
        let result = load_index(&mut f, idx_path.to_str().unwrap(), &locale);
        assert!(result.is_err(), "corrupted index must be rejected");
    }

    // ── Phase 2 — additional load_index / IndexWriter tests ─────────────────

    #[test]
    fn test_write_multiple_entries_and_read_back_verifies_all_fields() {
        use crate::i18n::Locale;
        use crate::models::archive::{
            ArchiveIndexEntry, ArchiveIndexEntryWrapper, CompressionMethod,
        };
        use crate::reader::load_index;

        let dir = tempfile::tempdir().unwrap();
        let idx_path = dir.path().join("multi3.dari");
        let archive_ts = 1_700_000_100u64;

        let mut iw = IndexWriter::new(&idx_path, archive_ts, 1).unwrap();

        let names = ["alpha.txt", "beta.rs", "gamma.html"];
        let sizes = [(100u64, 80u64), (200, 150), (300, 250)];

        for (name, (orig, comp)) in names.iter().zip(sizes.iter()) {
            let entry = ArchiveIndexEntry {
                offset: 1024,
                bitflags: 0,
                compression_method: CompressionMethod::None,
                modification_timestamp: 42,
                uid: 1000,
                gid: 1000,
                perm: 0o644,
                checksum: [0x11; 32],
                original_size: *orig,
                compressed_size: *comp,
                path_length: name.len() as u32,
                extra_length: 0,
            };
            let wrapper = ArchiveIndexEntryWrapper::new_v6(
                entry,
                name.to_string(),
                String::new(),
                Vec::new(),
                [0x22; 32],
                0,
                0,
            );
            iw.write_entry(&wrapper).unwrap();
        }
        iw.finish().unwrap();

        let mut f = std::fs::File::open(&idx_path).unwrap();
        let locale = Locale::new("en");
        let state = load_index(&mut f, idx_path.to_str().unwrap(), &locale).unwrap();

        assert_eq!(state.entries.len(), 3);
        let paths: Vec<&str> = state.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"alpha.txt"));
        assert!(paths.contains(&"beta.rs"));
        assert!(paths.contains(&"gamma.html"));
        let state_ts = state.header.timestamp;
        assert_eq!(state_ts, archive_ts, "timestamp must round-trip");
        assert_eq!(
            state.index_offset, 0,
            "external index always reports index_offset 0"
        );
    }

    #[test]
    fn test_load_index_entry_count_mismatch_is_rejected() {
        use crate::i18n::Locale;
        use crate::reader::load_index;

        let dir = tempfile::tempdir().unwrap();
        let idx_path = dir.path().join("mismatch.dari");

        // Write a valid empty .dari (0 entries).
        let iw = IndexWriter::new(&idx_path, 1_700_000_200u64, 1).unwrap();
        iw.finish().unwrap();

        // Corrupt the entry_count in the footer: change it from 0 to 5.
        // Footer layout: sig(9) + entry_count(4 LE) + checksum(32) = 45 bytes at end.
        // The BLAKE3 only covers bytes before the footer, so changing entry_count
        // without touching the body keeps the checksum valid — but parsing will fail
        // because the body is too short to hold 5 entries.
        let mut bytes = std::fs::read(&idx_path).unwrap();
        let footer_start = bytes.len() - std::mem::size_of::<IndexFileFooter>();
        let ec_offset = footer_start + 9; // after 9-byte signature
        bytes[ec_offset..ec_offset + 4].copy_from_slice(&5u32.to_le_bytes());
        std::fs::write(&idx_path, &bytes).unwrap();

        let mut f = std::fs::File::open(&idx_path).unwrap();
        let locale = Locale::new("en");
        let result = load_index(&mut f, idx_path.to_str().unwrap(), &locale);
        assert!(
            result.is_err(),
            "entry_count mismatch (body too short) must be rejected"
        );
    }

    #[test]
    fn test_load_index_wrong_header_signature_is_rejected() {
        use blake3::Hasher;

        let dir = tempfile::tempdir().unwrap();
        let idx_path = dir.path().join("badsig.dari");

        // Build a .dari-like file with a wrong header signature ("NOTDIX" instead of "DARIDX").
        // We compute the correct BLAKE3 so the integrity check passes, but the header
        // signature check must then reject it.
        let mut wrong_hdr = [0u8; 17];
        wrong_hdr[0..6].copy_from_slice(b"NOTDIX"); // wrong signature
        wrong_hdr[6] = 1; // version
        wrong_hdr[7..15].copy_from_slice(&1_700_000_300u64.to_le_bytes()); // timestamp
        wrong_hdr[15..17].copy_from_slice(&1u16.to_le_bytes()); // total_volumes

        let mut hasher = Hasher::new();
        hasher.update(&wrong_hdr);
        let checksum = *hasher.finalize().as_bytes();

        let mut footer = [0u8; 45];
        footer[0..9].copy_from_slice(INDEX_FOOTER_SIGNATURE);
        footer[9..13].copy_from_slice(&0u32.to_le_bytes()); // entry_count = 0
        footer[13..45].copy_from_slice(&checksum);

        let mut content = wrong_hdr.to_vec();
        content.extend_from_slice(&footer);
        std::fs::write(&idx_path, &content).unwrap();

        let mut f = std::fs::File::open(&idx_path).unwrap();
        let locale = Locale::new("en");
        let result = load_index(&mut f, idx_path.to_str().unwrap(), &locale);
        assert!(result.is_err(), "wrong header signature must be rejected");
    }

    #[test]
    fn test_index_writer_path_accessor_returns_correct_path() {
        let dir = tempfile::tempdir().unwrap();
        let idx_path = dir.path().join("accessor_test.dari");
        let iw = IndexWriter::new(&idx_path, 0, 1).unwrap();
        assert_eq!(
            iw.path(),
            idx_path.as_path(),
            "path() must return the file path passed to new()"
        );
        iw.finish().unwrap();
    }

    #[test]
    fn test_load_index_total_volumes_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let idx_path = dir.path().join("volumes.dari");

        // Write an index claiming to cover 3 volumes.
        let iw = IndexWriter::new(&idx_path, 1_700_000_400u64, 3).unwrap();
        iw.finish().unwrap();

        // Re-read the raw bytes and inspect the IndexFileHeader.
        let bytes = std::fs::read(&idx_path).unwrap();
        let total_volumes = u16::from_le_bytes(bytes[15..17].try_into().unwrap());
        assert_eq!(total_volumes, 3, "total_volumes must be stored correctly");
    }
}
