use crate::models::archive::{
    ArchiveFooter, ArchiveHeader, ArchiveIndexEntry, ArchiveIndexEntryWrapper, CompressionMethod,
};
use crate::pipeline::{
    CompressionPipeline, INDEX_FLAG_LINKED_DATA, PipelineConfig, PipelineFileData,
};
use crate::utils::get_mode;
use eyre::{Context, Result};
use rust_i18n::t;
use std::collections::HashMap;
use std::fs::{File, metadata};
use std::io::{Seek, Write};
use std::path::PathBuf;
use std::time::SystemTime;

const CHUNK_SIZE: usize = 512 * 1024; // 512KB

/// Holds all data needed to write a file into an archive, produced by
/// [`prepare_file_from_disk`] so that preparation can run in parallel.
pub struct PreparedFile {
    pub archive_path: String,
    pub pipeline_result: PipelineFileData,
    pub timestamp: u64,
    pub uid: u32,
    pub gid: u32,
    pub perm: u16,
}

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
}

#[derive(Clone, Copy)]
struct ExistingFileData {
    offset: u32,
    compression_method: crate::models::archive::CompressionMethod,
    compressed_size: u32,
    bitflags: u16,
}

// ---------------------------------------------------------------------------
// File reading helper (safe to call from multiple threads)
// ---------------------------------------------------------------------------

fn read_file_content(file_path: &PathBuf, file_size: usize) -> Result<Vec<u8>> {
    if file_size > CHUNK_SIZE {
        let mut file = File::open(file_path)?;
        let mut content = Vec::with_capacity(file_size);
        let mut buffer = vec![0u8; CHUNK_SIZE];
        loop {
            let n = std::io::Read::read(&mut file, &mut buffer)?;
            if n == 0 {
                break;
            }
            content.extend_from_slice(&buffer[..n]);
        }
        Ok(content)
    } else {
        std::fs::read(file_path).map_err(Into::into)
    }
}

/// Read, checksum, and compress a file without touching the archive writer.
/// Safe to call from multiple threads simultaneously.
pub fn prepare_file_from_disk(
    pipeline: &CompressionPipeline,
    file_path: &PathBuf,
    archive_path: &str,
) -> Result<PreparedFile> {
    let fs_meta = metadata(file_path)?;
    let file_size = fs_meta.len() as usize;
    let (uid, gid, perm) = get_mode(&fs_meta);

    let timestamp = fs_meta
        .modified()?
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();

    let file_content = read_file_content(file_path, file_size)?;
    let pipeline_result = pipeline.process_file(file_path, file_content)?;

    Ok(PreparedFile {
        archive_path: archive_path.to_string(),
        pipeline_result,
        timestamp,
        uid,
        gid,
        perm,
    })
}

impl<W: Write + Seek> ArchiveBuilder<W> {
    pub fn with_config(writer: W, config: PipelineConfig) -> Self {
        Self {
            writer,
            entries: Vec::new(),
            pipeline: CompressionPipeline::new(config),
            dedup_index: HashMap::new(),
        }
    }

    pub fn import_existing_entries(&mut self, existing_entries: Vec<ArchiveIndexEntryWrapper>) {
        for wrapper in existing_entries {
            if wrapper.entry.bitflags & INDEX_FLAG_LINKED_DATA == 0 {
                self.dedup_index.insert(
                    wrapper.entry.checksum,
                    ExistingFileData {
                        offset: wrapper.entry.offset,
                        compression_method: wrapper.entry.compression_method,
                        compressed_size: wrapper.entry.compressed_size,
                        bitflags: wrapper.entry.bitflags,
                    },
                );
            }

            self.entries.push(wrapper);
        }
    }

    pub fn write_header(&mut self) -> Result<()> {
        ArchiveHeader::new()
            .write(&mut self.writer)
            .wrap_err(t!("cli.common.errors.header_write_failed"))?;

        Ok(())
    }

    /// Write a [`PreparedFile`] to the archive. Must be called from a single thread.
    /// Returns a [`FileAddOutcome`] suitable for verbose progress reporting.
    pub fn commit_prepared(&mut self, prepared: PreparedFile) -> Result<FileAddOutcome> {
        let archive_path = prepared.archive_path;
        let pipeline_result = prepared.pipeline_result;
        let mut bitflags = pipeline_result.bitflags;
        let original_size = pipeline_result.original_size as u64;

        if let Some(existing) = self.dedup_index.get(&pipeline_result.checksum).copied() {
            bitflags |= INDEX_FLAG_LINKED_DATA;

            self.entries.push(ArchiveIndexEntryWrapper::new(
                ArchiveIndexEntry {
                    offset: existing.offset,
                    bitflags: bitflags | existing.bitflags,
                    compression_method: existing.compression_method,
                    modification_timestamp: prepared.timestamp,
                    uid: prepared.uid,
                    gid: prepared.gid,
                    perm: prepared.perm,
                    checksum: pipeline_result.checksum,
                    original_size: pipeline_result.original_size,
                    compressed_size: existing.compressed_size,
                    path_length: archive_path.len() as u32,
                    extra_length: pipeline_result.extra.len() as u32,
                },
                archive_path.clone(),
                pipeline_result.extra,
            ));

            return Ok(FileAddOutcome {
                archive_path,
                original_size,
                stored_size: existing.compressed_size as u64,
                compression_method: existing.compression_method,
                is_dedup: true,
            });
        }

        // Record byte offset where this file's data block begins
        let data_offset =
            self.writer
                .stream_position()
                .wrap_err(t!("cli.common.errors.get_write_position_failed"))? as u32;

        // Write file data: compressed bytes if compression ran, otherwise original bytes
        let (bytes_to_write, compressed_size) = match &pipeline_result.compressed_content {
            Some(compressed) => (compressed.as_slice(), compressed.len() as u32),
            None => (
                pipeline_result.original_content.as_slice(),
                pipeline_result.original_size,
            ),
        };

        self.writer
            .write_all(bytes_to_write)
            .wrap_err_with(|| t!("cli.common.errors.file_write_failed", file = archive_path))?;

        self.entries.push(ArchiveIndexEntryWrapper::new(
            ArchiveIndexEntry {
                offset: data_offset,
                bitflags,
                compression_method: pipeline_result.compression_method,
                modification_timestamp: prepared.timestamp,
                uid: prepared.uid,
                gid: prepared.gid,
                perm: prepared.perm,
                checksum: pipeline_result.checksum,
                original_size: pipeline_result.original_size,
                compressed_size,
                path_length: archive_path.len() as u32,
                extra_length: pipeline_result.extra.len() as u32,
            },
            archive_path.clone(),
            pipeline_result.extra,
        ));

        self.dedup_index.insert(
            pipeline_result.checksum,
            ExistingFileData {
                offset: data_offset,
                compression_method: pipeline_result.compression_method,
                compressed_size,
                bitflags,
            },
        );

        Ok(FileAddOutcome {
            archive_path,
            original_size,
            stored_size: compressed_size as u64,
            compression_method: pipeline_result.compression_method,
            is_dedup: false,
        })
    }

    /// Convenience wrapper: prepare and commit a single file.
    /// Use [`prepare_file_from_disk`] + [`Self::commit_prepared`] for parallel workflows.
    pub fn add_file(&mut self, file_path: &PathBuf, archive_path: &str) -> Result<FileAddOutcome> {
        let prepared = prepare_file_from_disk(&self.pipeline, file_path, archive_path)?;
        self.commit_prepared(prepared)
    }

    pub fn build(&mut self) -> Result<()> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::archive::{ArchiveFooter, ArchiveHeader, CompressionMethod};
    use crate::pipeline::INDEX_FLAG_LINKED_DATA;
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

        // compression_method is the 7th byte of ArchiveIndexEntry (after offset u32 + bitflags u16)
        let cm_byte = data[index_offset + 6];
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

        let cm_byte = data[index_offset + 6];
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

        let cm_byte = data[index_offset + 6];
        assert_eq!(
            cm_byte,
            CompressionMethod::None as u8,
            ".jpg should use None by default"
        );
    }

    #[test]
    fn test_jpg_entry_with_compress_images_falls_back_for_invalid_data() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("photo.jpg");
        std::fs::write(&file_path, b"fake jpeg data that compresses aaaaaaaaa").unwrap();

        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::with_config(
            buffer,
            PipelineConfig {
                compress_images: true,
                encryption_passphrase: None,
            },
        );
        builder.write_header().unwrap();
        builder
            .add_file(&file_path.to_path_buf(), &file_path.display().to_string())
            .unwrap();
        builder.build().unwrap();
        let data = builder.writer.into_inner();

        let footer_base = data.len() - size_of::<ArchiveFooter>();
        let index_offset = read_bytes_as::<u32>(&data, footer_base + 7).unwrap() as usize;

        let cm_byte = data[index_offset + 6];
        assert_eq!(
            cm_byte,
            CompressionMethod::None as u8,
            "invalid jpeg bytes should be stored unchanged when optimization fails"
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

        let first_offset = read_bytes_as::<u32>(&data, index_offset).unwrap();
        let first_path_len = read_bytes_as::<u32>(&data, index_offset + 65).unwrap() as usize;
        let first_extra_len = read_bytes_as::<u32>(&data, index_offset + 69).unwrap() as usize;

        let second_entry_offset = index_offset + entry_size + first_path_len + first_extra_len;
        let second_offset = read_bytes_as::<u32>(&data, second_entry_offset).unwrap();
        let second_flags = read_bytes_as::<u16>(&data, second_entry_offset + 4).unwrap();

        assert_eq!(
            first_offset, second_offset,
            "deduplicated file should link to first offset"
        );
        assert_eq!(
            second_flags & INDEX_FLAG_LINKED_DATA,
            INDEX_FLAG_LINKED_DATA,
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

        let first_offset = read_bytes_as::<u32>(&data, index_offset).unwrap();
        let first_path_len = read_bytes_as::<u32>(&data, index_offset + 65).unwrap() as usize;
        let first_extra_len = read_bytes_as::<u32>(&data, index_offset + 69).unwrap() as usize;

        let second_entry_offset = index_offset + entry_size + first_path_len + first_extra_len;
        let second_offset = read_bytes_as::<u32>(&data, second_entry_offset).unwrap();
        let second_flags = read_bytes_as::<u16>(&data, second_entry_offset + 4).unwrap();

        assert_ne!(
            first_offset, second_offset,
            "different files should keep distinct data offsets"
        );
        assert_eq!(
            second_flags & INDEX_FLAG_LINKED_DATA,
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
            appended_flags & INDEX_FLAG_LINKED_DATA,
            INDEX_FLAG_LINKED_DATA,
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

        let cm_byte = data[index_offset + 6];
        assert_eq!(
            cm_byte,
            CompressionMethod::None as u8,
            "tiny .rs file should be stored uncompressed when ZStd cannot shrink it"
        );

        // Verify the stored size equals the original size (no compression applied)
        let original_size = read_bytes_as::<u32>(&data, index_offset + 57).unwrap();
        let compressed_size = read_bytes_as::<u32>(&data, index_offset + 61).unwrap();
        assert_eq!(
            original_size, compressed_size,
            "stored size must equal original size for an uncompressed entry"
        );
    }
}
