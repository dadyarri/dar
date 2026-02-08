use std::io::Write;

use eyre::Result;

const COLUMNS_DELIMITER: [u8; 3] = [0u8; 3];
const SECTION_DELIMITER: [u8; 512] = [0u8; 512];

pub struct ArchiveHeader {}

impl ArchiveHeader {
    pub const VERSION_PREFIX: &'static [u8] = b"DAR";
    pub const VERSION_NUMBER: &'static [u8] = b"0002";

    pub fn write_to(buf: &mut Vec<u8>) -> Result<()> {
        buf.write_all(Self::VERSION_PREFIX)?;
        buf.write_all(Self::VERSION_NUMBER)?;
        Ok(())
    }
}

pub struct ArchiveEntry {
    data: Vec<u8>,
}

impl ArchiveEntry {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    pub fn write_to(&self, buf: &mut Vec<u8>) -> Result<()> {
        buf.write_all(&self.data)?;
        buf.write_all(&SECTION_DELIMITER)?;
        Ok(())
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum CompressionAlgorithm {
    None,
    Brotli,
    Zstandard,
}

impl CompressionAlgorithm {
    pub fn write_to(&self, buf: &mut Vec<u8>) {
        let byte: u8 = match self {
            CompressionAlgorithm::None => 0,
            CompressionAlgorithm::Brotli => 1,
            CompressionAlgorithm::Zstandard => 2,
        };
        buf.push(byte);
    }
}

pub struct ArchiveIndexEntry {
    pub offset: u64,
    pub compression_algorithm: CompressionAlgorithm,
    pub timestamp: u64,
    pub uid: u8,
    pub gid: u8,
    pub perm: u16,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
    pub checksum: String,
    pub path_length: u32,
    pub path: String,
    pub extra_length: u64,
    pub extra: String,
}

impl ArchiveIndexEntry {
    pub fn new(
        offset: u64,
        path: String,
        uncompressed_size: u64,
    ) -> Self {
        let path_length = path.len() as u32;
        Self {
            offset,
            compression_algorithm: CompressionAlgorithm::None,
            timestamp: 0,
            uid: 0,
            gid: 0,
            perm: 0o644,
            uncompressed_size,
            compressed_size: uncompressed_size,
            checksum: String::new(),
            path_length,
            path,
            extra_length: 0,
            extra: String::new(),
        }
    }

    pub fn write_to(&self, buf: &mut Vec<u8>) -> Result<()> {
        buf.write_all(&self.offset.to_be_bytes())?;
        buf.write_all(&COLUMNS_DELIMITER)?;
        self.compression_algorithm.write_to(buf);
        buf.write_all(&COLUMNS_DELIMITER)?;
        buf.write_all(&self.timestamp.to_be_bytes())?;
        buf.write_all(&COLUMNS_DELIMITER)?;
        buf.write_all(&self.uid.to_be_bytes())?;
        buf.write_all(&COLUMNS_DELIMITER)?;
        buf.write_all(&self.gid.to_be_bytes())?;
        buf.write_all(&COLUMNS_DELIMITER)?;
        buf.write_all(&self.perm.to_be_bytes())?;
        buf.write_all(&COLUMNS_DELIMITER)?;
        buf.write_all(&self.uncompressed_size.to_be_bytes())?;
        buf.write_all(&COLUMNS_DELIMITER)?;
        buf.write_all(&self.compressed_size.to_be_bytes())?;
        buf.write_all(&COLUMNS_DELIMITER)?;
        buf.write_all(&self.checksum.as_bytes())?;
        buf.write_all(&COLUMNS_DELIMITER)?;
        buf.write_all(&self.path_length.to_be_bytes())?;
        buf.write_all(&COLUMNS_DELIMITER)?;
        buf.write_all(&self.path.as_bytes())?;
        buf.write_all(&COLUMNS_DELIMITER)?;
        buf.write_all(&self.extra_length.to_be_bytes())?;
        buf.write_all(&COLUMNS_DELIMITER)?;
        buf.write_all(&self.extra.as_bytes())?;
        buf.write_all(&SECTION_DELIMITER)?;

        Ok(())
    }
}

pub struct ArchiveEndRecord {
    pub index_offset: u64,
    pub index_length: u64,
    pub archive_checksum: String,
}

impl ArchiveEndRecord {
    pub fn new(index_offset: u64, index_length: u64) -> Self {
        Self {
            index_offset,
            index_length,
            archive_checksum: String::new(),
        }
    }

    pub fn write_to(&self, buf: &mut Vec<u8>) -> Result<()> {
        buf.write_all(&self.index_offset.to_be_bytes())?;
        buf.write_all(&COLUMNS_DELIMITER)?;
        buf.write_all(&self.index_length.to_be_bytes())?;
        buf.write_all(&COLUMNS_DELIMITER)?;
        buf.write_all(&self.archive_checksum.as_bytes())?;
        buf.write_all(&SECTION_DELIMITER)?;
        Ok(())
    }
}
