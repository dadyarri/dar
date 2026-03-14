use crate::models::archive::{
    ArchiveFooter, ArchiveHeader, ArchiveIndexEntry, ArchiveIndexEntryWrapper, CompressionMethod,
};
use crate::utils::get_mode;
use eyre::{Context, Result};
use std::fs::{metadata, File};
use std::io::{Seek, Write};
use std::path::PathBuf;
use std::time::SystemTime;

const CHUNK_SIZE: usize = 512 * 1024; // 512KB

pub struct ArchiveBuilder<W: Write + Seek> {
    writer: W,
    index_offset: u32,
    entries: Vec<ArchiveIndexEntryWrapper>,
}

impl<W: Write + Seek> ArchiveBuilder<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            index_offset: 0,
            entries: Vec::new(),
        }
    }

    pub fn write_header(&mut self) -> Result<()> {
        ArchiveHeader::new()
            .write(&mut self.writer)
            .wrap_err("Failed to write archive header")?;

        self.index_offset += size_of::<ArchiveHeader>() as u32 + 1;

        Ok(())
    }

    pub fn add_file(&mut self, file_path: &PathBuf) -> Result<()> {
        // Определить порядок действий для сжатия;
        // Сжать файл;

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

        let hash = blake3::hash(&file_content);
        let mut checksum = [0u8; 32];
        checksum.copy_from_slice(hash.as_bytes());

        self.entries.push(ArchiveIndexEntryWrapper::new(
            ArchiveIndexEntry::new(
                0,
                CompressionMethod::None,
                timestamp,
                uid,
                gid,
                perm,
                checksum,
                file_size as u32,
                0,
                file_path.display().to_string().len() as u32,
                0,
            ),
            file_path.display().to_string(),
            "".to_string(),
        ));

        Ok(())
    }

    pub fn build(&mut self) -> Result<()> {
        ArchiveFooter::new(self.index_offset, self.entries.len() as u32)
            .write(&mut self.writer)
            .wrap_err("Failed to write archive footer")?;

        self.writer.flush().wrap_err("Failed to flush archive")?;

        Ok(())
    }
}
