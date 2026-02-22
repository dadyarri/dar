use crate::archive::decompress_data;
use crate::models::archive::ArchiveIndexEntry;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use eyre::Result;

pub struct PreviewReader {
    file_handle: File,
    cache: Option<Vec<u8>>,
    last_file: Option<String>,
    data_section_start: u64,
}

impl PreviewReader {
    pub fn new(file_handle: File, data_section_start: u64) -> Self {
        Self {
            file_handle,
            cache: None,
            last_file: None,
            data_section_start,
        }
    }

    pub fn load_file(&mut self, entry: &ArchiveIndexEntry) -> Result<Vec<u8>> {
        if let Some(cached) = &self.cache {
            if let Some(last) = &self.last_file {
                if last == &entry.path {
                    return Ok(cached.clone());
                }
            }
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

        self.cache = Some(decompressed.clone());
        self.last_file = Some(entry.path.clone());

        Ok(decompressed)
    }

    pub fn get_lines(content: &[u8], max_width: usize) -> Vec<String> {
        match String::from_utf8(content.to_vec()) {
            Ok(text) => {
                text.lines()
                    .map(|line| {
                        if line.len() > max_width {
                            format!("{}…", &line[..max_width - 1])
                        } else {
                            line.to_string()
                        }
                    })
                    .collect()
            }
            Err(_) => {
                let sample_size = std::cmp::min(512, content.len());
                let valid_utf8 = content[..sample_size]
                    .iter()
                    .filter(|&&b| (b > 31 && b < 127) || b == b'\n' || b == b'\t' || b > 127)
                    .count();

                if (sample_size - valid_utf8) > sample_size / 3 {
                    vec![String::from("[Binary file - not displayable]")]
                } else {
                    vec![String::from("[Unable to decode file]")]
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn is_binary(content: &[u8]) -> bool {
        if content.is_empty() {
            return false;
        }

        let sample_size = std::cmp::min(512, content.len());
        let valid_utf8 = content[..sample_size]
            .iter()
            .filter(|&&b| (b > 31 && b < 127) || b == b'\n' || b == b'\t' || b > 127)
            .count();

        (sample_size - valid_utf8) > sample_size / 3
    }
}
