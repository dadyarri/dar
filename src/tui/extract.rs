use crate::archive::decompress_data;
use crate::models::archive::ArchiveIndexEntry;
use std::fs::{File, create_dir_all};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use filetime::{FileTime, set_file_mtime};
use eyre::Result;

pub struct Extractor {
    file_handle: File,
    data_section_start: u64,
}

impl Extractor {
    pub fn new(file_handle: File, data_section_start: u64) -> Self {
        Self {
            file_handle,
            data_section_start,
        }
    }

    pub fn extract_files(
        &mut self,
        entries: Vec<ArchiveIndexEntry>,
        output_dir: &str,
    ) -> Result<()> {
        create_dir_all(output_dir)?;

        for entry in entries.iter() {
            self.extract_single(entry, output_dir)?;
        }

        Ok(())
    }

    fn extract_single(&mut self, entry: &ArchiveIndexEntry, base_dir: &str) -> Result<()> {
        let output_path = Path::new(base_dir).join(&entry.path);

        if let Some(parent) = output_path.parent() {
            create_dir_all(parent)?;
        }

        // data_offset is relative to data_section_start, so we need to add them
        let absolute_offset = self.data_section_start + entry.data_offset;
        self.file_handle.seek(SeekFrom::Start(absolute_offset))?;
        
        // Read the 8-byte length prefix
        let mut len_buf = [0u8; 8];
        self.file_handle.read_exact(&mut len_buf)?;
        let _stored_size = u64::from_be_bytes(len_buf);
        
        let mut compressed = vec![0u8; entry.compressed_size as usize];
        self.file_handle.read_exact(&mut compressed)?;

        let decompressed = decompress_data(compressed, entry)?;

        let hash = blake3::hash(&decompressed);
        if hash.as_bytes() != &entry.checksum[..] {
            return Err(eyre::eyre!(
                "Checksum mismatch for {}: expected {:?}, got {:?}",
                entry.path,
                entry.checksum,
                hash.as_bytes()
            ));
        }

        let mut file = File::create(&output_path)?;
        file.write_all(&decompressed)?;

        let mtime = FileTime::from_unix_time(entry.modification_time as i64, 0);
        set_file_mtime(&output_path, mtime)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(entry.permissions as u32);
            std::fs::set_permissions(&output_path, perms)?;
        }

        Ok(())
    }
}
