use std::io::Write;

use eyre::Result;

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

enum CompressionAlgorithm {
    None,
    Brotli,
    Zstandard,
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

struct ArchiveEndRecord {
    index_offset: u64,
    index_length: u64,
    archive_checksum: String,
}
