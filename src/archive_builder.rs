// Re-export so that callers importing from archive_builder keep working.
pub use crate::conflict::{ConflictMode, make_renamed_path};
pub use crate::file_reader::{PreparedFile, prepare_file_from_disk};

use crate::constants::flags;
use crate::format_version::FormatVersion;
use crate::models::archive::{
    ArchiveFooter, ArchiveFooterV6, ArchiveHeader, ArchiveHeaderV6, ArchiveIndexEntry,
    ArchiveIndexEntryV6, ArchiveIndexEntryWrapper, CompressionMethod,
};
use crate::pipeline::{CompressionPipeline, PipelineConfig};
use eyre::{Context, Result, eyre};
use rust_i18n::t;
use std::collections::{HashMap, HashSet};
use std::io::{Seek, Write};
use std::path::PathBuf;

/// Returned by [`ArchiveBuilder::commit_prepared`] with metadata suitable for
/// verbose progress reporting.
pub struct FileAddOutcome {
    pub archive_path: String,
    pub original_size: u64,
    /// Bytes actually stored in the archive (compressed / encrypted size).
    pub stored_size: u64,
    pub compression_method: CompressionMethod,
    pub is_dedup: bool,
}

pub struct ArchiveBuilder<W: Write + Seek> {
    writer: W,
    entries: Vec<ArchiveIndexEntryWrapper>,
    pipeline: CompressionPipeline,
    dedup_index: HashMap<[u8; 32], ExistingFileData>,
    /// All archive-relative paths currently in `entries`.
    path_set: HashSet<String>,
    /// Conflict resolution strategy applied by [`Self::commit_prepared`].
    conflict_mode: ConflictMode,
    /// Target on-disk format version.  Defaults to [`FormatVersion::V5`].
    target_version: FormatVersion,
    /// Unix timestamp written into the archive header by [`Self::write_header`].
    /// Zero before `write_header` is called.
    header_timestamp: u64,
    /// Optional external index writer attached for v6 archives.
    /// When set, [`Self::build_v6`] writes all entries to this writer and calls
    /// [`crate::index_writer::IndexWriter::finish`] before returning.
    index_writer: Option<crate::index_writer::IndexWriter>,
}

#[derive(Clone, Copy)]
struct ExistingFileData {
    offset: u64,
    compression_method: crate::models::archive::CompressionMethod,
    compressed_size: u64,
    bitflags: u16,
    /// BLAKE3 of bytes as stored on disk; zero for entries imported from v5 archives.
    stored_checksum: [u8; 32],
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Construct an [`ArchiveIndexEntry`] from its constituent parts.
///
/// Extracted to eliminate the two nearly-identical construction sites in
/// [`ArchiveBuilder::commit_prepared`].
#[allow(clippy::too_many_arguments)]
fn make_index_entry(
    offset: u64,
    bitflags: u16,
    compression_method: CompressionMethod,
    timestamp: u64,
    uid: u32,
    gid: u32,
    perm: u16,
    checksum: [u8; 32],
    original_size: u64,
    compressed_size: u64,
    path_len: u32,
    extra_len: u32,
) -> ArchiveIndexEntry {
    ArchiveIndexEntry {
        offset,
        bitflags,
        compression_method,
        modification_timestamp: timestamp,
        uid,
        gid,
        perm,
        checksum,
        original_size,
        compressed_size,
        path_length: path_len,
        extra_length: extra_len,
    }
}

impl<W: Write + Seek> ArchiveBuilder<W> {
    #[must_use]
    pub fn with_config(writer: W, config: PipelineConfig) -> Self {
        Self {
            writer,
            entries: Vec::new(),
            pipeline: CompressionPipeline::new(config),
            dedup_index: HashMap::new(),
            path_set: HashSet::new(),
            conflict_mode: ConflictMode::default(),
            target_version: FormatVersion::default(),
            header_timestamp: 0,
            index_writer: None,
        }
    }

    /// Create a new builder targeting a specific format version.
    ///
    /// Use this constructor when the caller needs to write an archive in a
    /// version other than the default (v5).  All behaviour is identical to
    /// [`Self::with_config`] for v5; v6 write paths are added in Phase 1.
    #[must_use]
    pub fn with_version(writer: W, config: PipelineConfig, version: FormatVersion) -> Self {
        Self {
            target_version: version,
            ..Self::with_config(writer, config)
        }
    }

    /// Override the conflict-resolution strategy used by [`Self::commit_prepared`].
    pub fn set_conflict_mode(&mut self, mode: ConflictMode) {
        self.conflict_mode = mode;
    }

    /// Attach an external index writer for v6 archives.
    ///
    /// When set, [`Self::build`] writes all committed entries to `iw` and calls
    /// [`crate::index_writer::IndexWriter::finish`] before returning.  Must be
    /// called **after** [`Self::write_header`] so that the index writer is
    /// created with the correct `archive_timestamp` (see [`Self::header_timestamp`]).
    pub fn set_index_writer(&mut self, iw: crate::index_writer::IndexWriter) {
        self.index_writer = Some(iw);
    }

    /// Returns the Unix timestamp written into the archive header by the most
    /// recent call to [`Self::write_header`].
    ///
    /// Use this value when constructing a [`crate::index_writer::IndexWriter`]
    /// so the timestamps in the `.dari` and `.dar` files match.
    pub fn header_timestamp(&self) -> u64 {
        self.header_timestamp
    }

    /// Returns `true` if `path` is already present in the archive (either from
    /// imported existing entries or previously committed files in this session).
    #[allow(dead_code)]
    pub fn path_exists(&self, path: &str) -> bool {
        self.path_set.contains(path)
    }

    pub fn import_existing_entries(&mut self, existing_entries: Vec<ArchiveIndexEntryWrapper>) {
        for wrapper in existing_entries {
            if wrapper.entry.bitflags & flags::LINKED_DATA == 0 {
                self.dedup_index.insert(
                    wrapper.entry.checksum,
                    ExistingFileData {
                        offset: wrapper.entry.offset,
                        compression_method: wrapper.entry.compression_method,
                        compressed_size: wrapper.entry.compressed_size,
                        bitflags: wrapper.entry.bitflags,
                        stored_checksum: wrapper.stored_checksum,
                    },
                );
            }

            self.path_set.insert(wrapper.path.clone());
            self.entries.push(wrapper);
        }
    }

    pub fn write_header(&mut self) -> Result<()> {
        match self.target_version {
            FormatVersion::V5 => {
                let h = ArchiveHeader::new()?;
                self.header_timestamp = h.timestamp;
                h.write(&mut self.writer)
                    .wrap_err(t!("cli.common.errors.header_write_failed"))?;
                Ok(())
            }
            FormatVersion::V6 => {
                let h = ArchiveHeaderV6::new()?;
                self.header_timestamp = h.timestamp;
                h.write(&mut self.writer)
                    .wrap_err(t!("cli.common.errors.header_write_failed"))?;
                Ok(())
            }
        }
    }

    /// Write a [`PreparedFile`] to the archive. Must be called from a single thread.
    /// Returns a [`FileAddOutcome`] suitable for verbose progress reporting.
    ///
    /// When the resolved archive path already exists in the archive the configured
    /// [`ConflictMode`] is applied:
    /// - [`ConflictMode::Error`]     — returns `Err` (callers should pre-check via
    ///   [`Self::path_exists`] before the write phase to avoid partial archives).
    /// - [`ConflictMode::Rename`]    — the path is suffixed with `-1`, `-2`, … until free.
    /// - [`ConflictMode::Overwrite`] — the existing entry is dropped from the index (its data
    ///   block becomes dead bytes since the format has no compaction step).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `archive_path` already exists and [`ConflictMode::Error`] is active,
    /// - writing the file data block or the index entry fails due to an I/O error.
    pub fn commit_prepared(&mut self, prepared: PreparedFile) -> Result<FileAddOutcome> {
        let mut archive_path = prepared.archive_path;
        let pipeline_result = prepared.pipeline_result;
        let mut bitflags = pipeline_result.bitflags;
        let original_size = pipeline_result.original_size;

        // ── Conflict resolution ──────────────────────────────────────────────
        if self.path_set.contains(&archive_path) {
            match self.conflict_mode {
                ConflictMode::Error => {
                    return Err(eyre!(t!(
                        "cli.append.errors.append_conflict_error",
                        paths = archive_path
                    )));
                }
                ConflictMode::Rename => {
                    archive_path = make_renamed_path(&archive_path, &self.path_set);
                }
                ConflictMode::Overwrite => {
                    // Remove the existing entry and evict its checksum from the
                    // dedup index (if it was a primary block) so that future files
                    // with the same checksum don't get linked to now-dead bytes.
                    if let Some(removed) = self.entries.iter().find(|e| e.path == archive_path) {
                        let entry_bitflags = removed.entry.bitflags;
                        if entry_bitflags & flags::LINKED_DATA == 0 {
                            self.dedup_index.remove(&removed.entry.checksum);
                        }
                    }
                    self.entries.retain(|e| e.path != archive_path);
                    self.path_set.remove(&archive_path);
                }
            }
        }

        if let Some(existing) = self.dedup_index.get(&pipeline_result.checksum).copied() {
            bitflags |= flags::LINKED_DATA;

            self.entries.push(ArchiveIndexEntryWrapper::new_v6(
                make_index_entry(
                    existing.offset,
                    bitflags | existing.bitflags,
                    existing.compression_method,
                    prepared.timestamp,
                    prepared.uid,
                    prepared.gid,
                    prepared.perm,
                    pipeline_result.checksum,
                    pipeline_result.original_size,
                    existing.compressed_size,
                    archive_path.len() as u32,
                    pipeline_result.extra.len() as u32,
                ),
                archive_path.clone(),
                pipeline_result.extra,
                existing.stored_checksum,
                0, // xattr_length — Phase 6
                0, // volume_number — Phase 3
            ));
            self.path_set.insert(archive_path.clone());

            return Ok(FileAddOutcome {
                archive_path,
                original_size,
                stored_size: existing.compressed_size,
                compression_method: existing.compression_method,
                is_dedup: true,
            });
        }

        // Record byte offset where this file's data block begins
        let data_offset = self
            .writer
            .stream_position()
            .wrap_err(t!("cli.common.errors.get_write_position_failed"))?;

        // Write file data: compressed bytes if compression ran, otherwise original bytes
        let (bytes_to_write, compressed_size) = match &pipeline_result.compressed_content {
            Some(compressed) => (compressed.as_slice(), compressed.len() as u64),
            None => (
                pipeline_result.original_content.as_slice(),
                pipeline_result.original_size,
            ),
        };

        self.writer
            .write_all(bytes_to_write)
            .wrap_err_with(|| t!("cli.common.errors.file_write_failed", file = archive_path))?;

        // Compute BLAKE3 of the bytes exactly as stored on disk (post-compression,
        // post-encryption).  Used as `stored_checksum` in v6 index entries.
        let stored_checksum = *blake3::hash(bytes_to_write).as_bytes();

        self.path_set.insert(archive_path.clone());
        self.entries.push(ArchiveIndexEntryWrapper::new_v6(
            make_index_entry(
                data_offset,
                bitflags,
                pipeline_result.compression_method,
                prepared.timestamp,
                prepared.uid,
                prepared.gid,
                prepared.perm,
                pipeline_result.checksum,
                pipeline_result.original_size,
                compressed_size,
                archive_path.len() as u32,
                pipeline_result.extra.len() as u32,
            ),
            archive_path.clone(),
            pipeline_result.extra,
            stored_checksum,
            0, // xattr_length — Phase 6
            0, // volume_number — Phase 3
        ));

        self.dedup_index.insert(
            pipeline_result.checksum,
            ExistingFileData {
                offset: data_offset,
                compression_method: pipeline_result.compression_method,
                compressed_size,
                bitflags,
                stored_checksum,
            },
        );

        Ok(FileAddOutcome {
            archive_path,
            original_size,
            stored_size: compressed_size,
            compression_method: pipeline_result.compression_method,
            is_dedup: false,
        })
    }

    /// Convenience wrapper: prepare and commit a single file.
    /// Use [`prepare_file_from_disk`] + [`Self::commit_prepared`] for parallel workflows.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read from disk, or if committing it fails
    /// (see [`Self::commit_prepared`]).
    pub fn add_file(&mut self, file_path: &PathBuf, archive_path: &str) -> Result<FileAddOutcome> {
        let prepared = prepare_file_from_disk(&self.pipeline, file_path, archive_path)?;
        self.commit_prepared(prepared)
    }

    /// Finalise the archive by writing all index entries and the footer, then flushing.
    ///
    /// Must be called exactly once after all files have been added.
    ///
    /// # Errors
    ///
    /// Returns an error if any index entry, the footer, or the flush fails due to an I/O error.
    pub fn build(&mut self) -> Result<()> {
        match self.target_version {
            FormatVersion::V5 => self.build_v5(),
            FormatVersion::V6 => self.build_v6(),
        }
    }

    fn build_v5(&mut self) -> Result<()> {
        // Record where the index section begins
        let index_offset =
            self.writer
                .stream_position()
                .wrap_err(t!("cli.common.errors.get_index_offset_failed"))? as u32;

        // Write all index entries: fixed-size struct + path bytes + extra bytes
        for wrapper in &self.entries {
            wrapper
                .entry
                .write(&mut self.writer)
                .wrap_err(t!("cli.common.errors.index_entry_write_failed"))?;
            self.writer
                .write_all(wrapper.path.as_bytes())
                .wrap_err(t!("cli.common.errors.entry_path_write_failed"))?;
            self.writer
                .write_all(wrapper.extra.as_bytes())
                .wrap_err(t!("cli.common.errors.entry_extra_write_failed"))?;
        }

        // Write footer pointing at the index
        ArchiveFooter::new(index_offset, self.entries.len() as u32)
            .write(&mut self.writer)
            .wrap_err(t!("cli.common.errors.footer_write_failed"))?;

        self.writer
            .flush()
            .wrap_err(t!("cli.common.errors.flush_archive_failed"))?;

        Ok(())
    }

    /// Finalise a v6 archive: write `ArchiveIndexEntryV6` structs for all entries,
    /// then an `ArchiveFooterV6` with a u64 index offset, then flush.
    /// If an external [`crate::index_writer::IndexWriter`] was attached via
    /// [`Self::set_index_writer`], all entries are also written there and the
    /// writer is finalised.
    fn build_v6(&mut self) -> Result<()> {
        // Record where the index section begins (u64 — no 4 GiB ceiling).
        let index_offset = self
            .writer
            .stream_position()
            .wrap_err(t!("cli.common.errors.get_index_offset_failed"))?;

        for wrapper in &self.entries {
            // Copy packed fields to locals so we can safely read them.
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
            v6_entry
                .write(&mut self.writer)
                .wrap_err(t!("cli.common.errors.index_entry_write_failed"))?;
            self.writer
                .write_all(wrapper.path.as_bytes())
                .wrap_err(t!("cli.common.errors.entry_path_write_failed"))?;
            self.writer
                .write_all(wrapper.extra.as_bytes())
                .wrap_err(t!("cli.common.errors.entry_extra_write_failed"))?;
            // xattr_length == 0 for all current entries (Phase 6 adds xattr support).
        }

        ArchiveFooterV6::new(index_offset, self.entries.len() as u32)
            .write(&mut self.writer)
            .wrap_err(t!("cli.common.errors.footer_write_failed"))?;

        self.writer
            .flush()
            .wrap_err(t!("cli.common.errors.flush_archive_failed"))?;

        // Write external index file if one was attached.
        let iw_opt = self.index_writer.take();
        if let Some(mut iw) = iw_opt {
            for wrapper in &self.entries {
                iw.write_entry(wrapper)
                    .wrap_err("Failed to write entry to external index file")?;
            }
            iw.finish()
                .wrap_err("Failed to finalise external index file")?;
        }

        Ok(())
    }

    /// Consume the builder and return the underlying writer.
    ///
    /// Useful when the writer is a `Cursor<Vec<u8>>` and the caller needs to
    /// inspect or reuse the raw bytes after [`Self::build`] has been called.
    #[allow(dead_code)]
    pub fn into_inner(self) -> W {
        self.writer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::archive::{ArchiveFooter, ArchiveHeader, CompressionMethod};
    use crate::utils::read_bytes_as;
    use std::io::Cursor;
    use std::mem::size_of;
    use std::ptr;

    /// Write a minimal archive (header + one real file + footer) and return the bytes.
    fn build_archive_with_file(path: &std::path::Path) -> Vec<u8> {
        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::with_config(buffer, PipelineConfig::default());
        builder.write_header().unwrap();
        builder
            .add_file(&path.to_path_buf(), &path.display().to_string())
            .unwrap();
        builder.build().unwrap();
        builder.writer.into_inner()
    }

    #[test]
    fn test_header_signature_and_version() {
        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::with_config(buffer, PipelineConfig::default());
        builder.write_header().unwrap();
        let data = builder.writer.into_inner();

        assert_eq!(&data[0..4], b"DARI", "signature mismatch");
        assert_eq!(data[4], 5, "version mismatch");
        assert!(data.len() >= size_of::<ArchiveHeader>());
    }

    #[test]
    fn test_footer_signature_written() {
        // Create a temp file with known content
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("hello.txt");
        std::fs::write(&file_path, b"hello world").unwrap();

        let data = build_archive_with_file(&file_path);

        // Footer is the last `size_of::<ArchiveFooter>()` bytes
        let footer_offset = data.len() - size_of::<ArchiveFooter>();
        assert_eq!(
            &data[footer_offset..footer_offset + 7],
            b"DARIEND",
            "footer signature mismatch"
        );
    }

    #[test]
    fn test_footer_file_count_matches() {
        let dir = tempfile::tempdir().unwrap();
        let f1 = dir.path().join("a.txt");
        let f2 = dir.path().join("b.rs");
        std::fs::write(&f1, b"aaa").unwrap();
        std::fs::write(&f2, b"fn main() {}").unwrap();

        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::with_config(buffer, PipelineConfig::default());
        builder.write_header().unwrap();
        builder.add_file(&f1, &f1.display().to_string()).unwrap();
        builder.add_file(&f2, &f2.display().to_string()).unwrap();
        builder.build().unwrap();
        let data = builder.writer.into_inner();

        // file count is at footer_offset + 7 (signature) + 4 (index_offset) = +11
        let footer_base = data.len() - size_of::<ArchiveFooter>();
        let file_count = read_bytes_as::<u32>(&data, footer_base + 11).unwrap();
        assert_eq!(file_count, 2, "footer file count should be 2");
    }

    #[test]
    fn test_index_offset_points_past_file_data() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("data.txt");
        std::fs::write(&file_path, b"some data").unwrap();

        let data = build_archive_with_file(&file_path);

        let footer_base = data.len() - size_of::<ArchiveFooter>();
        // index_offset is at footer_base + 7 (after signature)
        let index_offset = read_bytes_as::<u32>(&data, footer_base + 7).unwrap();

        // Index must come after the header (13 bytes) and the file data
        assert!(
            index_offset > size_of::<ArchiveHeader>() as u32,
            "index_offset ({index_offset}) must be past the header"
        );
    }

    #[test]
    fn test_file_data_is_written_between_header_and_index() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("words.txt");
        let content = b"the quick brown fox";
        std::fs::write(&file_path, content).unwrap();

        let data = build_archive_with_file(&file_path);

        let header_end = size_of::<ArchiveHeader>();
        let footer_base = data.len() - size_of::<ArchiveFooter>();
        let index_offset = read_bytes_as::<u32>(&data, footer_base + 7).unwrap() as usize;

        // There must be at least some bytes between header and index (the compressed file data)
        assert!(
            index_offset > header_end,
            "file data section should be non-empty between header and index"
        );
    }

    #[test]
    fn test_txt_entry_uses_brotli() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("readme.txt");
        // Payload must be large enough that Brotli actually shrinks it
        let content = b"Hello from dari, this is a repetitive line.\n".repeat(20);
        std::fs::write(&file_path, &content).unwrap();

        let data = build_archive_with_file(&file_path);

        let footer_base = data.len() - size_of::<ArchiveFooter>();
        let index_offset = read_bytes_as::<u32>(&data, footer_base + 7).unwrap() as usize;

        // compression_method is at byte 10 of ArchiveIndexEntry (after offset u64 + bitflags u16)
        let cm_byte = data[index_offset + 10];
        assert_eq!(
            cm_byte,
            CompressionMethod::Brotli as u8,
            ".txt should use Brotli"
        );
    }

    #[test]
    fn test_rs_entry_uses_zstandard() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("main.rs");
        // Payload must be large enough that ZStd actually shrinks it
        let content = b"fn main() { println!(\"hello from dari\"); }\n".repeat(20);
        std::fs::write(&file_path, &content).unwrap();

        let data = build_archive_with_file(&file_path);

        let footer_base = data.len() - size_of::<ArchiveFooter>();
        let index_offset = read_bytes_as::<u32>(&data, footer_base + 7).unwrap() as usize;

        let cm_byte = data[index_offset + 10];
        assert_eq!(
            cm_byte,
            CompressionMethod::Zstandard as u8,
            ".rs should use Zstandard"
        );
    }

    #[test]
    fn test_jpg_entry_skips_compression_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("photo.jpg");
        std::fs::write(&file_path, b"fake jpeg data").unwrap();

        let data = build_archive_with_file(&file_path);

        let footer_base = data.len() - size_of::<ArchiveFooter>();
        let index_offset = read_bytes_as::<u32>(&data, footer_base + 7).unwrap() as usize;

        let cm_byte = data[index_offset + 10];
        assert_eq!(
            cm_byte,
            CompressionMethod::None as u8,
            ".jpg should use None by default"
        );
    }

    #[test]
    fn test_dedup_links_second_file_to_first_offset() {
        let dir = tempfile::tempdir().unwrap();
        let f1 = dir.path().join("same1.txt");
        let f2 = dir.path().join("same2.txt");
        std::fs::write(&f1, b"same-content").unwrap();
        std::fs::write(&f2, b"same-content").unwrap();

        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::with_config(buffer, PipelineConfig::default());
        builder.write_header().unwrap();
        builder.add_file(&f1, &f1.display().to_string()).unwrap();
        builder.add_file(&f2, &f2.display().to_string()).unwrap();
        builder.build().unwrap();
        let data = builder.writer.into_inner();

        let footer_base = data.len() - size_of::<ArchiveFooter>();
        let index_offset = read_bytes_as::<u32>(&data, footer_base + 7).unwrap() as usize;
        let entry_size = size_of::<crate::models::archive::ArchiveIndexEntry>();

        let first_offset = read_bytes_as::<u64>(&data, index_offset).unwrap();
        let first_path_len = read_bytes_as::<u32>(&data, index_offset + 77).unwrap() as usize;
        let first_extra_len = read_bytes_as::<u32>(&data, index_offset + 81).unwrap() as usize;

        let second_entry_offset = index_offset + entry_size + first_path_len + first_extra_len;
        let second_offset = read_bytes_as::<u64>(&data, second_entry_offset).unwrap();
        let second_flags = read_bytes_as::<u16>(&data, second_entry_offset + 8).unwrap();

        assert_eq!(
            first_offset, second_offset,
            "deduplicated file should link to first offset"
        );
        assert_eq!(
            second_flags & flags::LINKED_DATA,
            flags::LINKED_DATA,
            "deduplicated entry should set linked bitflag"
        );
    }

    #[test]
    fn test_dedup_does_not_link_different_checksums() {
        let dir = tempfile::tempdir().unwrap();
        let f1 = dir.path().join("a.txt");
        let f2 = dir.path().join("b.txt");
        std::fs::write(&f1, b"content-a").unwrap();
        std::fs::write(&f2, b"content-b").unwrap();

        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::with_config(buffer, PipelineConfig::default());
        builder.write_header().unwrap();
        builder.add_file(&f1, &f1.display().to_string()).unwrap();
        builder.add_file(&f2, &f2.display().to_string()).unwrap();
        builder.build().unwrap();
        let data = builder.writer.into_inner();

        let footer_base = data.len() - size_of::<ArchiveFooter>();
        let index_offset = read_bytes_as::<u32>(&data, footer_base + 7).unwrap() as usize;
        let entry_size = size_of::<crate::models::archive::ArchiveIndexEntry>();

        let first_offset = read_bytes_as::<u64>(&data, index_offset).unwrap();
        let first_path_len = read_bytes_as::<u32>(&data, index_offset + 77).unwrap() as usize;
        let first_extra_len = read_bytes_as::<u32>(&data, index_offset + 81).unwrap() as usize;

        let second_entry_offset = index_offset + entry_size + first_path_len + first_extra_len;
        let second_offset = read_bytes_as::<u64>(&data, second_entry_offset).unwrap();
        let second_flags = read_bytes_as::<u16>(&data, second_entry_offset + 8).unwrap();

        assert_ne!(
            first_offset, second_offset,
            "different files should keep distinct data offsets"
        );
        assert_eq!(
            second_flags & flags::LINKED_DATA,
            0,
            "non-duplicate entry must not set linked bitflag"
        );
    }

    #[test]
    fn test_import_existing_entries_seeds_dedup_map() {
        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::with_config(buffer, PipelineConfig::default());

        let checksum = blake3::hash(b"hello world");
        let mut checksum_bytes = [0u8; 32];
        checksum_bytes.copy_from_slice(checksum.as_bytes());
        let path = "existing/hello.txt".to_string();

        let entry = ArchiveIndexEntry {
            offset: 128,
            bitflags: 0,
            compression_method: CompressionMethod::None,
            modification_timestamp: 0,
            uid: 0,
            gid: 0,
            perm: 0,
            checksum: checksum_bytes,
            original_size: 11,
            compressed_size: 11,
            path_length: path.len() as u32,
            extra_length: 0,
        };

        builder.import_existing_entries(vec![ArchiveIndexEntryWrapper::new(
            entry,
            path,
            String::new(),
        )]);

        let dir = tempfile::tempdir().unwrap();
        let new_file = dir.path().join("hello.txt");
        std::fs::write(&new_file, b"hello world").unwrap();
        builder
            .add_file(&new_file, "new/hello.txt")
            .expect("append add_file should succeed");

        assert_eq!(builder.entries.len(), 2);
        let imported = &builder.entries[0];
        let appended = &builder.entries[1];
        let imported_offset = unsafe { ptr::read_unaligned(ptr::addr_of!(imported.entry.offset)) };
        let appended_offset = unsafe { ptr::read_unaligned(ptr::addr_of!(appended.entry.offset)) };
        assert_eq!(imported_offset, appended_offset);

        let appended_flags = unsafe { ptr::read_unaligned(ptr::addr_of!(appended.entry.bitflags)) };
        assert_eq!(
            appended_flags & flags::LINKED_DATA,
            flags::LINKED_DATA,
            "appended entry should reuse imported data block"
        );
    }

    #[test]
    fn test_small_rs_entry_stored_uncompressed() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("tiny.rs");
        // 12 bytes — ZStd frame overhead alone exceeds this, so the pipeline
        // must fall back to storing the original bytes with method=None.
        std::fs::write(&file_path, b"fn main() {}").unwrap();

        let data = build_archive_with_file(&file_path);

        let footer_base = data.len() - size_of::<ArchiveFooter>();
        let index_offset = read_bytes_as::<u32>(&data, footer_base + 7).unwrap() as usize;

        let cm_byte = data[index_offset + 10];
        assert_eq!(
            cm_byte,
            CompressionMethod::None as u8,
            "tiny .rs file should be stored uncompressed when ZStd cannot shrink it"
        );

        // Verify the stored size equals the original size (no compression applied)
        let original_size = read_bytes_as::<u64>(&data, index_offset + 61).unwrap();
        let compressed_size = read_bytes_as::<u64>(&data, index_offset + 69).unwrap();
        assert_eq!(
            original_size, compressed_size,
            "stored size must equal original size for an uncompressed entry"
        );
    }

    #[test]
    fn test_conflict_error_mode_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let f1 = dir.path().join("file.txt");
        std::fs::write(&f1, b"first").unwrap();

        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::with_config(buffer, PipelineConfig::default());
        builder.set_conflict_mode(ConflictMode::Error);
        builder.write_header().unwrap();
        builder.add_file(&f1, "file.txt").unwrap();

        let f2 = dir.path().join("file2.txt");
        std::fs::write(&f2, b"second").unwrap();
        let result = builder.add_file(&f2, "file.txt");
        assert!(result.is_err(), "error mode should reject a path conflict");
    }

    #[test]
    fn test_conflict_rename_mode_suffixes_path() {
        let dir = tempfile::tempdir().unwrap();
        let f1 = dir.path().join("orig.txt");
        std::fs::write(&f1, b"first").unwrap();
        let f2 = dir.path().join("dup.txt");
        std::fs::write(&f2, b"second").unwrap();

        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::with_config(buffer, PipelineConfig::default());
        builder.set_conflict_mode(ConflictMode::Rename);
        builder.write_header().unwrap();
        builder.add_file(&f1, "assets/logo.png").unwrap();
        let outcome = builder.add_file(&f2, "assets/logo.png").unwrap();
        assert_eq!(
            outcome.archive_path, "assets/logo-1.png",
            "rename mode should suffix conflicting path with -1"
        );

        // A third file with the same original path should become -2
        let f3 = dir.path().join("dup2.txt");
        std::fs::write(&f3, b"third").unwrap();
        let outcome3 = builder.add_file(&f3, "assets/logo.png").unwrap();
        assert_eq!(outcome3.archive_path, "assets/logo-2.png");
    }

    #[test]
    fn test_conflict_overwrite_mode_replaces_entry() {
        let dir = tempfile::tempdir().unwrap();
        let f1 = dir.path().join("orig.txt");
        std::fs::write(&f1, b"first").unwrap();
        let f2 = dir.path().join("new.txt");
        std::fs::write(&f2, b"second").unwrap();

        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::with_config(buffer, PipelineConfig::default());
        builder.set_conflict_mode(ConflictMode::Overwrite);
        builder.write_header().unwrap();
        builder.add_file(&f1, "config.toml").unwrap();
        builder.add_file(&f2, "config.toml").unwrap();

        // Only one entry with path "config.toml" should remain
        let count = builder
            .entries
            .iter()
            .filter(|e| e.path == "config.toml")
            .count();
        assert_eq!(
            count, 1,
            "overwrite mode should leave exactly one entry for the path"
        );
    }

    #[test]
    fn test_path_exists_reflects_committed_entries() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("data.txt");
        std::fs::write(&f, b"hello").unwrap();

        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::with_config(buffer, PipelineConfig::default());
        builder.write_header().unwrap();
        assert!(!builder.path_exists("data.txt"));
        builder.add_file(&f, "data.txt").unwrap();
        assert!(builder.path_exists("data.txt"));
        assert!(!builder.path_exists("other.txt"));
    }

    #[test]
    fn test_conflict_overwrite_evicts_stale_dedup_entry() {
        let dir = tempfile::tempdir().unwrap();
        let f_orig = dir.path().join("config.toml");
        std::fs::write(&f_orig, b"original = true").unwrap();
        let f_same = dir.path().join("same.toml");
        std::fs::write(&f_same, b"original = true").unwrap(); // same content → same checksum
        let f_new = dir.path().join("new.toml");
        std::fs::write(&f_new, b"replaced = true").unwrap();

        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::with_config(buffer, PipelineConfig::default());
        builder.set_conflict_mode(ConflictMode::Overwrite);
        builder.write_header().unwrap();
        // Add original "config.toml"
        builder.add_file(&f_orig, "config.toml").unwrap();
        // Overwrite it with different content
        builder.add_file(&f_new, "config.toml").unwrap();
        // Now add a file with the same content as the original; it must NOT dedup to the dead offset
        let outcome = builder.add_file(&f_same, "same.toml").unwrap();
        assert!(
            !outcome.is_dedup,
            "overwrite should evict dedup entry so same-checksum files are written fresh"
        );
    }

    #[test]
    fn test_make_renamed_path_no_extension() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert("README".to_string());
        let renamed = make_renamed_path("README", &set);
        assert_eq!(renamed, "README-1");
    }

    #[test]
    fn test_make_renamed_path_with_extension() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert("assets/logo.png".to_string());
        set.insert("assets/logo-1.png".to_string());
        let renamed = make_renamed_path("assets/logo.png", &set);
        assert_eq!(renamed, "assets/logo-2.png");
    }

    // 5.3 — Deduplication with three or more identical files

    #[test]
    fn test_dedup_three_identical_files_share_one_data_block() {
        let dir = tempfile::tempdir().unwrap();
        let f1 = dir.path().join("copy1.bin");
        let f2 = dir.path().join("copy2.bin");
        let f3 = dir.path().join("copy3.bin");
        let content = b"deduplicated content shared three times";
        std::fs::write(&f1, content).unwrap();
        std::fs::write(&f2, content).unwrap();
        std::fs::write(&f3, content).unwrap();

        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::with_config(buffer, PipelineConfig::default());
        builder.write_header().unwrap();
        builder.add_file(&f1, "copy1.bin").unwrap();
        builder.add_file(&f2, "copy2.bin").unwrap();
        builder.add_file(&f3, "copy3.bin").unwrap();
        builder.build().unwrap();
        let data = builder.writer.into_inner();

        // Parse footer to find index_offset
        let footer_base = data.len() - size_of::<ArchiveFooter>();
        let index_offset = read_bytes_as::<u32>(&data, footer_base + 7).unwrap() as usize;
        let entry_size = size_of::<crate::models::archive::ArchiveIndexEntry>();

        // Parse first entry: offset + path/extra lengths
        let first_offset = read_bytes_as::<u64>(&data, index_offset).unwrap();
        let first_flags = read_bytes_as::<u16>(&data, index_offset + 8).unwrap();
        let first_path_len = read_bytes_as::<u32>(&data, index_offset + 77).unwrap() as usize;
        let first_extra_len = read_bytes_as::<u32>(&data, index_offset + 81).unwrap() as usize;

        // Parse second entry
        let second_base = index_offset + entry_size + first_path_len + first_extra_len;
        let second_offset = read_bytes_as::<u64>(&data, second_base).unwrap();
        let second_flags = read_bytes_as::<u16>(&data, second_base + 8).unwrap();
        let second_path_len = read_bytes_as::<u32>(&data, second_base + 77).unwrap() as usize;
        let second_extra_len = read_bytes_as::<u32>(&data, second_base + 81).unwrap() as usize;

        // Parse third entry
        let third_base = second_base + entry_size + second_path_len + second_extra_len;
        let third_offset = read_bytes_as::<u64>(&data, third_base).unwrap();
        let third_flags = read_bytes_as::<u16>(&data, third_base + 8).unwrap();

        // First entry must NOT carry LINKED_DATA
        assert_eq!(
            first_flags & flags::LINKED_DATA,
            0,
            "first entry must not be linked"
        );
        // Second and third must be linked to the first data block
        assert_eq!(
            second_flags & flags::LINKED_DATA,
            flags::LINKED_DATA,
            "second entry must be linked"
        );
        assert_eq!(
            third_flags & flags::LINKED_DATA,
            flags::LINKED_DATA,
            "third entry must be linked"
        );
        // All three share the same data offset
        assert_eq!(
            first_offset, second_offset,
            "second entry must reuse first data offset"
        );
        assert_eq!(
            first_offset, third_offset,
            "third entry must reuse first data offset"
        );
    }

    #[test]
    fn test_dedup_three_identical_files_all_extract_correctly() {
        use crate::extractor::extract_entries;
        use crate::i18n::Locale;
        use crate::reader::load_archive;
        use std::fs::File;

        let dir = tempfile::tempdir().unwrap();
        let content = b"shared content across three copies";
        let f1 = dir.path().join("a.txt");
        let f2 = dir.path().join("b.txt");
        let f3 = dir.path().join("c.txt");
        std::fs::write(&f1, content).unwrap();
        std::fs::write(&f2, content).unwrap();
        std::fs::write(&f3, content).unwrap();

        let archive_path = dir.path().join("triple_dedup.dar");
        {
            let file_handle = File::create(&archive_path).unwrap();
            let mut builder = ArchiveBuilder::with_config(file_handle, PipelineConfig::default());
            builder.write_header().unwrap();
            builder.add_file(&f1, "a.txt").unwrap();
            builder.add_file(&f2, "b.txt").unwrap();
            builder.add_file(&f3, "c.txt").unwrap();
            builder.build().unwrap();
        }

        let locale = Locale::new("en");
        let mut f = File::open(&archive_path).unwrap();
        let state = load_archive(&mut f, archive_path.to_str().unwrap(), &locale).unwrap();
        assert_eq!(state.entries.len(), 3);

        let dest = dir.path().join("out");
        let refs: Vec<_> = state.entries.iter().collect();
        extract_entries(&archive_path, &refs, &state.entries, &dest, None).unwrap();

        assert_eq!(std::fs::read(dest.join("a.txt")).unwrap(), content);
        assert_eq!(std::fs::read(dest.join("b.txt")).unwrap(), content);
        assert_eq!(std::fs::read(dest.join("c.txt")).unwrap(), content);
    }

    // ── Phase 1 — v6 format tests ────────────────────────────────────────────

    /// Build a minimal v6 archive (header + one file + footer) and return the bytes.
    fn build_v6_archive_with_file(path: &std::path::Path) -> Vec<u8> {
        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::with_version(buffer, PipelineConfig::default(), crate::format_version::FormatVersion::V6);
        builder.write_header().unwrap();
        builder.add_file(&path.to_path_buf(), &path.display().to_string()).unwrap();
        builder.build().unwrap();
        builder.writer.into_inner()
    }

    #[test]
    fn test_v6_header_signature_and_version() {
        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::with_version(buffer, PipelineConfig::default(), crate::format_version::FormatVersion::V6);
        builder.write_header().unwrap();
        let data = builder.writer.into_inner();

        assert_eq!(&data[0..4], b"DARI", "v6 signature mismatch");
        assert_eq!(data[4], 6, "v6 version byte mismatch");
        // volume_number and total_volumes at bytes 13..17
        let volume_number = u16::from_le_bytes(data[13..15].try_into().unwrap());
        let total_volumes = u16::from_le_bytes(data[15..17].try_into().unwrap());
        assert_eq!(volume_number, 0, "volume_number should default to 0");
        assert_eq!(total_volumes, 1, "total_volumes should default to 1");
        assert!(data.len() >= std::mem::size_of::<crate::models::archive::ArchiveHeaderV6>());
    }

    #[test]
    fn test_v6_footer_signature_written() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("hello.txt");
        std::fs::write(&file_path, b"hello world").unwrap();

        let data = build_v6_archive_with_file(&file_path);

        let footer_offset = data.len() - std::mem::size_of::<crate::models::archive::ArchiveFooterV6>();
        assert_eq!(
            &data[footer_offset..footer_offset + 7],
            b"DARIEND",
            "v6 footer signature mismatch"
        );
    }

    #[test]
    fn test_v6_footer_uses_u64_index_offset() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("data.txt");
        std::fs::write(&file_path, b"some data").unwrap();

        let data = build_v6_archive_with_file(&file_path);

        let footer_base = data.len() - std::mem::size_of::<crate::models::archive::ArchiveFooterV6>();
        // In v6 footer: sig(7) + index_offset(8 as u64) + amount_of_files(4)
        let index_offset = read_bytes_as::<u64>(&data, footer_base + 7).unwrap();
        assert!(
            index_offset > 17, // must be past the 17-byte v6 header
            "v6 index_offset ({index_offset}) must be past the v6 header"
        );
    }

    #[test]
    fn test_v6_footer_file_count_matches() {
        let dir = tempfile::tempdir().unwrap();
        let f1 = dir.path().join("a.txt");
        let f2 = dir.path().join("b.rs");
        std::fs::write(&f1, b"aaa").unwrap();
        std::fs::write(&f2, b"fn main() {}").unwrap();

        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::with_version(buffer, PipelineConfig::default(), crate::format_version::FormatVersion::V6);
        builder.write_header().unwrap();
        builder.add_file(&f1, &f1.display().to_string()).unwrap();
        builder.add_file(&f2, &f2.display().to_string()).unwrap();
        builder.build().unwrap();
        let data = builder.writer.into_inner();

        let footer_base = data.len() - std::mem::size_of::<crate::models::archive::ArchiveFooterV6>();
        // v6 footer: sig(7) + index_offset(8) = 15, then amount_of_files(4)
        let file_count = read_bytes_as::<u32>(&data, footer_base + 15).unwrap();
        assert_eq!(file_count, 2, "v6 footer file count should be 2");
    }

    #[test]
    fn test_v6_round_trip_extract() {
        use crate::extractor::extract_entries;
        use crate::i18n::Locale;
        use crate::reader::load_archive;
        use std::fs::File;

        let dir = tempfile::tempdir().unwrap();
        let content = b"hello from v6 format";
        let f1 = dir.path().join("a.txt");
        let f2 = dir.path().join("b.rs");
        std::fs::write(&f1, content).unwrap();
        std::fs::write(&f2, content).unwrap();

        let archive_path = dir.path().join("v6_test.dar");
        {
            let file_handle = File::create(&archive_path).unwrap();
            let mut builder = ArchiveBuilder::with_version(file_handle, PipelineConfig::default(), crate::format_version::FormatVersion::V6);
            builder.write_header().unwrap();
            builder.add_file(&f1, "a.txt").unwrap();
            builder.add_file(&f2, "b.rs").unwrap();
            builder.build().unwrap();
        }

        let locale = Locale::new("en");
        let mut f = File::open(&archive_path).unwrap();
        let state = load_archive(&mut f, archive_path.to_str().unwrap(), &locale).unwrap();
        assert_eq!(state.entries.len(), 2);
        assert_eq!(state.header.version, 6, "loaded archive should report version 6");

        let dest = dir.path().join("out_v6");
        let refs: Vec<_> = state.entries.iter().collect();
        extract_entries(&archive_path, &refs, &state.entries, &dest, None).unwrap();

        assert_eq!(std::fs::read(dest.join("a.txt")).unwrap(), content);
        assert_eq!(std::fs::read(dest.join("b.rs")).unwrap(), content);
    }

    #[test]
    fn test_v6_stored_checksum_is_populated() {
        use crate::i18n::Locale;
        use crate::reader::load_archive;
        use std::fs::File;

        let dir = tempfile::tempdir().unwrap();
        let content = b"content for stored checksum test";
        let f = dir.path().join("data.txt");
        std::fs::write(&f, content).unwrap();

        let archive_path = dir.path().join("stored_ck.dar");
        {
            let file_handle = File::create(&archive_path).unwrap();
            let mut builder = ArchiveBuilder::with_version(file_handle, PipelineConfig::default(), crate::format_version::FormatVersion::V6);
            builder.write_header().unwrap();
            builder.add_file(&f, "data.txt").unwrap();
            builder.build().unwrap();
        }

        let locale = Locale::new("en");
        let mut fh = File::open(&archive_path).unwrap();
        let state = load_archive(&mut fh, archive_path.to_str().unwrap(), &locale).unwrap();
        assert_eq!(state.entries.len(), 1);

        let wrapper = &state.entries[0];
        // stored_checksum_v6 must be Some (non-zero) for a v6 entry with actual content.
        assert!(
            wrapper.stored_checksum_v6().is_some(),
            "v6 entry must carry a non-zero stored_checksum"
        );
    }

    #[test]
    fn test_v6_dedup_stored_checksum_propagated_to_linked_entry() {
        use crate::i18n::Locale;
        use crate::reader::load_archive;
        use std::fs::File;

        let dir = tempfile::tempdir().unwrap();
        let content = b"deduped content";
        let f1 = dir.path().join("copy1.txt");
        let f2 = dir.path().join("copy2.txt");
        std::fs::write(&f1, content).unwrap();
        std::fs::write(&f2, content).unwrap();

        let archive_path = dir.path().join("dedup_v6.dar");
        {
            let file_handle = File::create(&archive_path).unwrap();
            let mut builder = ArchiveBuilder::with_version(file_handle, PipelineConfig::default(), crate::format_version::FormatVersion::V6);
            builder.write_header().unwrap();
            builder.add_file(&f1, "copy1.txt").unwrap();
            builder.add_file(&f2, "copy2.txt").unwrap();
            builder.build().unwrap();
        }

        let locale = Locale::new("en");
        let mut fh = File::open(&archive_path).unwrap();
        let state = load_archive(&mut fh, archive_path.to_str().unwrap(), &locale).unwrap();
        assert_eq!(state.entries.len(), 2);

        let primary = &state.entries[0];
        let linked = &state.entries[1];
        // Both entries should carry the same stored_checksum.
        assert_eq!(
            primary.stored_checksum, linked.stored_checksum,
            "linked v6 entry must inherit primary's stored_checksum"
        );
    }
}
