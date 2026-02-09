use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use eyre::{Error, Result, eyre};

/// Archive header: 512 bytes fixed size
/// Contains metadata for locating and validating archive sections
pub struct ArchiveHeader {
    pub data_section_start: u64,
    pub index_section_start: u64,
    pub total_files: u32,
    pub created_timestamp: u64,
    pub archive_checksum: [u8; 32], // BLAKE3 hash (computed last)
}

impl ArchiveHeader {
    pub const MAGIC: &'static [u8] = b"DAR\0";
    pub const VERSION: &'static [u8] = b"0004";
    pub const SIZE: usize = 512;

    pub fn new(data_section_start: u64, index_section_start: u64, total_files: u32) -> Self {
        let created_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            data_section_start,
            index_section_start,
            total_files,
            created_timestamp,
            archive_checksum: [0u8; 32],
        }
    }

    pub fn write_to(&self, buf: &mut Vec<u8>) -> Result<()> {
        let start_pos = buf.len();

        buf.write_all(Self::MAGIC)?;
        buf.write_all(Self::VERSION)?;
        buf.write_all(&self.data_section_start.to_be_bytes())?;
        buf.write_all(&self.index_section_start.to_be_bytes())?;
        buf.write_all(&self.total_files.to_be_bytes())?;
        buf.write_all(&self.created_timestamp.to_be_bytes())?;
        buf.write_all(&self.archive_checksum)?;
        buf.push(0u8); // flags (reserved)

        // Pad to exactly 512 bytes from start position
        let bytes_written = buf.len() - start_pos;
        let padding = if bytes_written < Self::SIZE {
            Self::SIZE - bytes_written
        } else {
            0
        };
        buf.write_all(&vec![0u8; padding])?;

        Ok(())
    }
}

/// Archive index entry: file metadata for later retrieval
/// Each entry is prefixed with its length for safe parsing
pub struct ArchiveIndexEntry {
    pub path: String,
    pub data_offset: u64,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
    pub compression_algorithm: CompressionAlgorithm,
    pub modification_time: u64,
    pub uid: u8,
    pub gid: u8,
    pub permissions: u16,
    pub checksum: [u8; 32], // BLAKE3 of uncompressed data
}

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum CompressionAlgorithm {
    None,
    Brotli,
    Zstandard,
    Lzma,
}

impl TryFrom<u8> for CompressionAlgorithm {
    type Error = Error;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        return match value {
            0 => Ok(CompressionAlgorithm::None),
            1 => Ok(CompressionAlgorithm::Brotli),
            2 => Ok(CompressionAlgorithm::Zstandard),
            3 => Ok(CompressionAlgorithm::Lzma),
            _ => Err(eyre!("Invalid value for CompressionAlgorithm")),
        };
    }
}

impl Into<u8> for CompressionAlgorithm {
    fn into(self: CompressionAlgorithm) -> u8 {
        return match self {
            CompressionAlgorithm::None => 0,
            CompressionAlgorithm::Brotli => 1,
            CompressionAlgorithm::Zstandard => 2,
            CompressionAlgorithm::Lzma => 3,
        };
    }
}

impl CompressionAlgorithm {
    pub fn as_byte(&self) -> u8 {
        return *self as u8;
    }
}

impl ArchiveIndexEntry {
    pub fn new(path: String, data_offset: u64, uncompressed_size: u64) -> Self {
        Self {
            path,
            data_offset,
            uncompressed_size,
            compressed_size: 0,
            compression_algorithm: CompressionAlgorithm::None,
            modification_time: 0,
            uid: 0,
            gid: 0,
            permissions: 0o644,
            checksum: [0u8; 32],
        }
    }

    /// Write entry to buffer in binary format
    /// Format: [entry_length: u32][path_length: u32][path: utf8][data_offset: u64][uncompressed_size: u64]
    ///         [compressed_size: u64][compression_algo: u8][mod_time: u64][uid: u8][gid: u8][perm: u16][checksum: 32bytes]
    pub fn write_to(&self, buf: &mut Vec<u8>) -> Result<()> {
        let start_len = buf.len();

        // Write placeholder for entry length (will be updated later)
        buf.write_all(&0u32.to_be_bytes())?;

        // Write path
        let path_bytes = self.path.as_bytes();
        buf.write_all(&(path_bytes.len() as u32).to_be_bytes())?;
        buf.write_all(path_bytes)?;

        // Write metadata
        buf.write_all(&self.data_offset.to_be_bytes())?;
        buf.write_all(&self.uncompressed_size.to_be_bytes())?;
        buf.write_all(&self.compressed_size.to_be_bytes())?;
        buf.write_all(&self.compression_algorithm.as_byte().to_be_bytes())?;
        buf.write_all(&self.modification_time.to_be_bytes())?;
        buf.write_all(&self.uid.to_be_bytes())?;
        buf.write_all(&self.gid.to_be_bytes())?;
        buf.write_all(&self.permissions.to_be_bytes())?;
        buf.write_all(&self.checksum)?;

        // Calculate and update entry length (excluding the 4-byte length field itself)
        let entry_len = (buf.len() - start_len - 4) as u32;
        buf[start_len..start_len + 4].copy_from_slice(&entry_len.to_be_bytes());

        Ok(())
    }
}

/// Archive end record: 64 bytes fixed size
/// Located at the end of the archive for quick validation and index location
pub struct ArchiveEndRecord {
    pub index_offset: u64,
    pub index_length: u64,
    pub archive_checksum: [u8; 32], // BLAKE3 of entire archive
}

impl ArchiveEndRecord {
    pub const MAGIC: &'static [u8] = b"DEND";
    pub const SIZE: usize = 64;

    pub fn new(index_offset: u64, index_length: u64) -> Self {
        Self {
            index_offset,
            index_length,
            archive_checksum: [0u8; 32],
        }
    }

    pub fn write_to(&self, buf: &mut Vec<u8>) -> Result<()> {
        let start_pos = buf.len();

        buf.write_all(Self::MAGIC)?;
        buf.write_all(&self.index_offset.to_be_bytes())?;
        buf.write_all(&self.index_length.to_be_bytes())?;
        buf.write_all(&self.archive_checksum)?;
        buf.push(0u8); // flags (reserved)

        // Pad to exactly 64 bytes from start position
        let bytes_written = buf.len() - start_pos;
        let padding = if bytes_written < Self::SIZE {
            Self::SIZE - bytes_written
        } else {
            0
        };
        buf.write_all(&vec![0u8; padding])?;

        Ok(())
    }
}
