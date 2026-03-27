use crate::models::archive::{
    ArchiveFooter, ArchiveHeader, ArchiveIndexEntry, ArchiveIndexEntryWrapper,
};
use crate::pipeline::{
    CompressionPipeline, PipelineConfig, INDEX_FLAG_LINKED_DATA,
};
use crate::utils::get_mode;
use eyre::{Context, Result};
use std::collections::HashMap;
use std::fs::{metadata, File};
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::time::SystemTime;

const CHUNK_SIZE: usize = 512 * 1024; // 512KB

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

impl<W: Write + Seek> ArchiveBuilder<W> {
    pub fn new(writer: W) -> Self {
        Self::with_config(writer, PipelineConfig::default())
    }

    pub fn with_config(writer: W, config: PipelineConfig) -> Self {
        Self {
            writer,
            entries: Vec::new(),
            pipeline: CompressionPipeline::new(config),
            dedup_index: HashMap::new(),
        }
    }

    pub fn write_header(&mut self) -> Result<()> {
        ArchiveHeader::new()
            .write(&mut self.writer)
            .wrap_err("Failed to write archive header")?;

        Ok(())
    }

    pub fn add_file(&mut self, file_path: &PathBuf) -> Result<()> {
        let fs_meta = metadata(file_path)?;
        let file_size = fs_meta.len() as usize;
        let (uid, gid, perm) = get_mode(&fs_meta);

        let timestamp = fs_meta
            .modified()?
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        let mut file_content = Vec::new();

        if file_size > CHUNK_SIZE {
            let mut file = File::open(file_path)?;
            let mut buffer = vec![0u8; CHUNK_SIZE];

            loop {
                let bytes_read = std::io::Read::read(&mut file, &mut buffer)?;
                if bytes_read == 0 {
                    break;
                }

                file_content.extend_from_slice(&buffer[..bytes_read]);
            }
        } else {
            file_content = std::fs::read(file_path)?;
        }

        // Process file through the compression pipeline (checksum + algorithm selection + compression)
        let pipeline_result = self.pipeline.process_file(file_path, file_content)?;

        let archive_path = file_path.display().to_string();
        let mut bitflags = pipeline_result.bitflags;

        if let Some(existing) = self.dedup_index.get(&pipeline_result.checksum).copied() {
            bitflags |= INDEX_FLAG_LINKED_DATA;

            self.entries.push(ArchiveIndexEntryWrapper::new(
                ArchiveIndexEntry::new(
                    existing.offset,
                    bitflags | existing.bitflags,
                    existing.compression_method,
                    timestamp,
                    uid,
                    gid,
                    perm,
                    pipeline_result.checksum,
                    pipeline_result.original_size,
                    existing.compressed_size,
                    archive_path.len() as u32,
                    pipeline_result.extra.len() as u32,
                ),
                archive_path,
                pipeline_result.extra,
            ));

            return Ok(());
        }

        // Record byte offset where this file's data block begins
        let data_offset = self
            .writer
            .seek(SeekFrom::Current(0))
            .wrap_err("Failed to get current write position")? as u32;

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
            .wrap_err_with(|| format!("Failed to write file data for {}", file_path.display()))?;

        self.entries.push(ArchiveIndexEntryWrapper::new(
            ArchiveIndexEntry::new(
                data_offset,
                bitflags,
                pipeline_result.compression_method,
                timestamp,
                uid,
                gid,
                perm,
                pipeline_result.checksum,
                pipeline_result.original_size,
                compressed_size,
                archive_path.len() as u32,
                pipeline_result.extra.len() as u32,
            ),
            archive_path,
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

        Ok(())
    }

    pub fn build(&mut self) -> Result<()> {
        // Record where the index section begins
        let index_offset = self
            .writer
            .seek(SeekFrom::Current(0))
            .wrap_err("Failed to get index offset position")? as u32;

        // Write all index entries: fixed-size struct + path bytes + extra bytes
        for wrapper in &self.entries {
            wrapper
                .entry
                .write(&mut self.writer)
                .wrap_err("Failed to write index entry")?;
            self.writer
                .write_all(wrapper.path.as_bytes())
                .wrap_err("Failed to write entry path")?;
            self.writer
                .write_all(wrapper.extra.as_bytes())
                .wrap_err("Failed to write entry extra")?;
        }

        // Write footer pointing at the index
        ArchiveFooter::new(index_offset, self.entries.len() as u32)
            .write(&mut self.writer)
            .wrap_err("Failed to write archive footer")?;

        self.writer.flush().wrap_err("Failed to flush archive")?;

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

    /// Write a minimal archive (header + one real file + footer) and return the bytes.
    fn build_archive_with_file(path: &std::path::Path) -> Vec<u8> {
        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::new(buffer);
        builder.write_header().unwrap();
        builder.add_file(&path.to_path_buf()).unwrap();
        builder.build().unwrap();
        builder.writer.into_inner()
    }

    #[test]
    fn test_header_signature_and_version() {
        let buffer = Cursor::new(Vec::new());
        let mut builder = ArchiveBuilder::new(buffer);
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
        let mut builder = ArchiveBuilder::new(buffer);
        builder.write_header().unwrap();
        builder.add_file(&f1).unwrap();
        builder.add_file(&f2).unwrap();
        builder.build().unwrap();
        let data = builder.writer.into_inner();

        // file count is at footer_offset + 7 (signature) + 4 (index_offset) = +11
        let footer_base = data.len() - size_of::<ArchiveFooter>();
        let file_count =
            read_bytes_as::<u32>(&data, footer_base + 11).unwrap();
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
        let index_offset =
            read_bytes_as::<u32>(&data, footer_base + 7).unwrap();

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
        let index_offset =
            read_bytes_as::<u32>(&data, footer_base + 7).unwrap() as usize;

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
        std::fs::write(&file_path, b"Hello from dari").unwrap();

        let data = build_archive_with_file(&file_path);

        let footer_base = data.len() - size_of::<ArchiveFooter>();
        let index_offset =
            read_bytes_as::<u32>(&data, footer_base + 7).unwrap() as usize;

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
        std::fs::write(&file_path, b"fn main() {}").unwrap();

        let data = build_archive_with_file(&file_path);

        let footer_base = data.len() - size_of::<ArchiveFooter>();
        let index_offset =
            read_bytes_as::<u32>(&data, footer_base + 7).unwrap() as usize;

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
        let index_offset =
            read_bytes_as::<u32>(&data, footer_base + 7).unwrap() as usize;

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
        builder.add_file(&file_path.to_path_buf()).unwrap();
        builder.build().unwrap();
        let data = builder.writer.into_inner();

        let footer_base = data.len() - size_of::<ArchiveFooter>();
        let index_offset =
            read_bytes_as::<u32>(&data, footer_base + 7).unwrap() as usize;

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
        let mut builder = ArchiveBuilder::new(buffer);
        builder.write_header().unwrap();
        builder.add_file(&f1).unwrap();
        builder.add_file(&f2).unwrap();
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

        assert_eq!(first_offset, second_offset, "deduplicated file should link to first offset");
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
        let mut builder = ArchiveBuilder::new(buffer);
        builder.write_header().unwrap();
        builder.add_file(&f1).unwrap();
        builder.add_file(&f2).unwrap();
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
}
