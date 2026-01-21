use std::io::Write;

use eyre::Result;

const COLUMNS_DELIMITER: &[u8; 3] = &[0u8; 3];
const SECTION_DELIMITER: &[u8; 512] = &[0u8; 512];

pub struct ArchiveHeader {}

impl ArchiveHeader {
    pub const VERSION_PREFIX: &[u8; 3] = b"DAR";
    pub const VERSION_NUMBER: &[u8; 4] = b"0002";

    pub fn write_to(buf: &mut Vec<u8>) -> Result<()> {
        buf.write_all(Self::VERSION_PREFIX)?;
        buf.write_all(Self::VERSION_NUMBER)?;
        Ok(())
    }
}

struct ArchiveEntry {
    data: Vec<u8>,
}

impl ArchiveEntry {
    pub fn write_to(&mut self, buf: &mut Vec<u8>) -> Result<()> {
        buf.write_all(&self.data)?;
        Ok(())
    }
}

#[repr(u8)]
enum CompressionAlgorithm {
    None,
    Brotli,
    Zstandard,
}

impl CompressionAlgorithm {
    pub fn write_to(&self, buf: &mut Vec<u8>) {
        let byte = match self {
            CompressionAlgorithm::None => 0,
            CompressionAlgorithm::Brotli => 1,
            CompressionAlgorithm::Zstandard => 2,
        };
        buf.push(byte);
    }
}

struct ArchiveIndexEntry {
    offset: u64,
    compression_algorithm: CompressionAlgorithm,
    timestamp: u64,
    uid: u8,
    gid: u8,
    perm: u16,
    uncompressed_size: u64,
    compressed_size: u64,
    checksum: String,
    path_length: u32,
    path: String,
    extra_length: u64,
    extra: String,
}

impl ArchiveIndexEntry {
    pub fn write_to(&mut self, buf: &mut Vec<u8>) -> Result<()> {
        buf.write_all(&self.offset.to_be_bytes())?;
        buf.write_all(COLUMNS_DELIMITER)?;
        self.compression_algorithm.write_to(buf);
        buf.write_all(COLUMNS_DELIMITER)?;
        buf.write_all(&self.timestamp.to_be_bytes())?;
        buf.write_all(COLUMNS_DELIMITER)?;
        buf.write_all(&self.uid.to_be_bytes())?;
        buf.write_all(COLUMNS_DELIMITER)?;
        buf.write_all(&self.gid.to_be_bytes())?;
        buf.write_all(COLUMNS_DELIMITER)?;
        buf.write_all(&self.perm.to_be_bytes())?;
        buf.write_all(COLUMNS_DELIMITER)?;
        buf.write_all(&self.uncompressed_size.to_be_bytes())?;
        buf.write_all(COLUMNS_DELIMITER)?;
        buf.write_all(&self.compressed_size.to_be_bytes())?;
        buf.write_all(COLUMNS_DELIMITER)?;
        buf.write_all(&self.checksum.as_bytes())?;
        buf.write_all(COLUMNS_DELIMITER)?;
        buf.write_all(&self.path_length.to_be_bytes())?;
        buf.write_all(COLUMNS_DELIMITER)?;
        buf.write_all(&self.path.as_bytes())?;
        buf.write_all(COLUMNS_DELIMITER)?;
        buf.write_all(&self.extra_length.to_be_bytes())?;
        buf.write_all(COLUMNS_DELIMITER)?;
        buf.write_all(&self.extra.as_bytes())?;
        buf.write_all(SECTION_DELIMITER)?;

        Ok(())
    }
}

struct ArchiveEndRecord {
    index_offset: u64,
    index_length: u64,
    archive_checksum: String,
}

impl ArchiveEndRecord {
    pub fn write_to(&mut self, buf: &mut Vec<u8>) -> Result<()> {
        buf.write_all(&self.index_offset.to_be_bytes())?;
        buf.write_all(COLUMNS_DELIMITER)?;
        buf.write_all(&self.index_length.to_be_bytes())?;
        buf.write_all(COLUMNS_DELIMITER)?;
        buf.write_all(&self.archive_checksum.as_bytes())?;
        buf.write_all(SECTION_DELIMITER)?;
        Ok(())
    }
}
