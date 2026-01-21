struct ArchiveHeader {
    version_prefix: String = "DAR",
    version_number: u8 = 2,
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
