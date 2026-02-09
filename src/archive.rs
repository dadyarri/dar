use eyre::{Result, eyre};
use std::{
    fs::{File, metadata},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    time::SystemTime,
};

use crate::models::archive::{
    ArchiveEndRecord, ArchiveHeader, ArchiveIndexEntry, CompressionAlgorithm,
};

const CHUNK_SIZE: usize = 512 * 1024; // 512KB

/// Read and parse archive header
pub fn read_header(file: &mut File) -> (Option<ArchiveHeader>, Result<()>) {
    file.seek(SeekFrom::Start(0)).ok();

    let mut buf = vec![0u8; 512];
    match file.read_exact(&mut buf) {
        Ok(()) => {
            // Check magic and version
            if &buf[0..3] != b"DAR" || &buf[4..8] != ArchiveHeader::VERSION {
                (None, Err(eyre!("Invalid header magic or version")))
            } else {
                // Parse header fields (big-endian)
                let data_section_start = u64::from_be_bytes(buf[8..16].try_into().unwrap());
                let index_section_start = u64::from_be_bytes(buf[16..24].try_into().unwrap());
                let total_files = u32::from_be_bytes(buf[24..28].try_into().unwrap());

                let mut archive_checksum = [0u8; 32];
                archive_checksum.copy_from_slice(&buf[36..68]);

                let header = ArchiveHeader {
                    data_section_start,
                    index_section_start,
                    total_files,
                    created_timestamp: 0, // Not needed for validation
                    archive_checksum,
                };

                (Some(header), Ok(()))
            }
        }
        Err(e) => (None, Err(eyre!("Cannot read header: {}", e))),
    }
}

/// Read and parse end record
pub fn read_end_record(file: &mut File, file_size: u64) -> (Option<ArchiveEndRecord>, Result<()>) {
    if file_size < 64 {
        return (None, Err(eyre!("File too small for end record")));
    }

    file.seek(SeekFrom::Start(file_size - 64)).ok();

    let mut buf = vec![0u8; 64];
    match file.read_exact(&mut buf) {
        Ok(()) => {
            // Check magic
            if &buf[0..4] != b"DEND" {
                (None, Err(eyre!("Invalid end record magic")))
            } else {
                let index_offset = u64::from_be_bytes([
                    buf[4], buf[5], buf[6], buf[7], buf[8], buf[9], buf[10], buf[11],
                ]);
                let index_length = u64::from_be_bytes([
                    buf[12], buf[13], buf[14], buf[15], buf[16], buf[17], buf[18], buf[19],
                ]);

                let mut archive_checksum = [0u8; 32];
                archive_checksum.copy_from_slice(&buf[20..52]);

                let end_record = ArchiveEndRecord {
                    index_offset,
                    index_length,
                    archive_checksum,
                };

                (Some(end_record), Ok(()))
            }
        }
        Err(e) => (None, Err(eyre!("Cannot read end record: {}", e))),
    }
}

/// Calculate archive checksum (BLAKE3 of entire file excluding checksum fields)
pub fn calculate_archive_checksum(
    file: &mut File,
    _header: &ArchiveHeader,
    file_size: u64,
) -> Result<[u8; 32]> {
    file.seek(SeekFrom::Start(0))?;

    let mut hasher = blake3::Hasher::new();

    // Read header (but skip checksum field at offset 36-68)
    let mut buf = vec![0u8; 512];
    file.read_exact(&mut buf)?;
    hasher.update(&buf[0..36]); // up to checksum
    hasher.update(&[0u8; 32]); // skip checksum
    hasher.update(&buf[68..]); // after checksum

    // Read data and index sections (exclude end record which is the last 64 bytes)
    let remaining = file_size - 512 - 64; // exclude header and end record
    let mut buf = vec![0u8; 65536]; // 64KB chunks
    let mut total_read = 0u64;

    while total_read < remaining {
        let to_read = std::cmp::min(buf.len(), (remaining - total_read) as usize);
        let n = file.read(&mut buf[..to_read])?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total_read += n as u64;
    }

    // Also read end record but skip its checksum field
    file.seek(SeekFrom::Start(file_size - 64))?;
    let mut end_record_buf = vec![0u8; 64];
    file.read_exact(&mut end_record_buf)?;
    hasher.update(&end_record_buf[0..20]); // magic and offsets
    hasher.update(&[0u8; 32]); // skip checksum field
    hasher.update(&end_record_buf[52..]); // after checksum

    let hash = hasher.finalize();
    let mut result = [0u8; 32];
    result.copy_from_slice(hash.as_bytes());
    Ok(result)
}

/// Parse single index entry
pub fn parse_index_entry(file: &mut File) -> Result<ArchiveIndexEntry> {
    let mut buf = [0u8; 4];
    file.read_exact(&mut buf)?;
    let entry_length = u32::from_be_bytes(buf) as usize;

    let mut entry_buf = vec![0u8; entry_length];
    file.read_exact(&mut entry_buf)?;

    let mut offset = 0;

    // Path length and path
    let path_len = u32::from_be_bytes(entry_buf[offset..offset + 4].try_into().unwrap()) as usize;
    offset += 4;

    let path = String::from_utf8(entry_buf[offset..offset + path_len].to_vec())?;
    offset += path_len;

    // Metadata
    let data_offset = u64::from_be_bytes(entry_buf[offset..offset + 8].try_into().unwrap());
    offset += 8;

    let uncompressed_size = u64::from_be_bytes(entry_buf[offset..offset + 8].try_into().unwrap());
    offset += 8;

    let compressed_size = u64::from_be_bytes(entry_buf[offset..offset + 8].try_into().unwrap());
    offset += 8;

    let compression_algorithm = CompressionAlgorithm::try_from(entry_buf[offset]).unwrap();
    offset += 1;

    let modification_time = u64::from_be_bytes(entry_buf[offset..offset + 8].try_into().unwrap());
    offset += 8;

    let uid = entry_buf[offset];
    offset += 1;
    let gid = entry_buf[offset];
    offset += 1;

    let permissions = u16::from_be_bytes([entry_buf[offset], entry_buf[offset + 1]]);
    offset += 2;

    let mut checksum = [0u8; 32];
    checksum.copy_from_slice(&entry_buf[offset..offset + 32]);

    Ok(ArchiveIndexEntry {
        path,
        data_offset,
        uncompressed_size,
        compressed_size,
        compression_algorithm,
        modification_time,
        uid,
        gid,
        permissions,
        checksum,
    })
}

pub fn add_file(
    path: &Path,
    archive_bytes: &mut Vec<u8>,
    progress: bool,
) -> Result<ArchiveIndexEntry> {
    let fs_meta = metadata(path)?;
    let file_size = fs_meta.len() as usize;
    let algorithm = get_compression_algorithm(path);
    let mut data = Vec::new();
    let timestamp = fs_meta
        .modified()?
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();

    #[cfg(unix)]
    let (uid, gid, perm) = {
        use std::os::unix::fs::MetadataExt;
        (
            (fs_meta.uid() % 256) as u8,
            (fs_meta.gid() % 256) as u8,
            (fs_meta.mode() & 0o777) as u16,
        )
    };

    #[cfg(not(unix))]
    let (uid, gid, perm) = (0u8, 0u8, 0o644u16);

    if file_size > CHUNK_SIZE {
        // Large file: read in chunks, calculate checksum, compress, then write
        let mut file = File::open(path)?;
        let mut buffer = vec![0u8; CHUNK_SIZE];
        let mut bytes_read_total = 0usize;

        loop {
            let bytes_read = std::io::Read::read(&mut file, &mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            data.extend_from_slice(&buffer[..bytes_read]);
            bytes_read_total += bytes_read;

            if progress {
                let percentage = (bytes_read_total as f64 / file_size as f64) * 100.0;
                eprint!(
                    "\r  {}: {:.1}% ({}/{}B)",
                    path.display(),
                    percentage,
                    bytes_read_total,
                    file_size
                );
            }
        }

        if progress {
            eprintln!(); // newline after progress
        }
    } else {
        // Small file: read all at once
        data = std::fs::read(path)?;
    }

    let hash = blake3::hash(&data);
    let mut checksum = [0u8; 32];
    checksum.copy_from_slice(hash.as_bytes());

    let compressed_data = compress_data(&data, algorithm)?;
    let compressed_size = compressed_data.len() as u64;

    archive_bytes.write_all(&compressed_size.to_be_bytes())?;
    archive_bytes.write_all(&compressed_data)?;

    Ok(ArchiveIndexEntry {
        path: "".to_string(),
        data_offset: 0,
        uncompressed_size: file_size as u64,
        compressed_size: compressed_size,
        compression_algorithm: algorithm,
        modification_time: timestamp,
        uid: uid,
        gid: gid,
        permissions: perm,
        checksum: checksum,
    })
}

fn get_compression_algorithm(path: &Path) -> CompressionAlgorithm {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        match ext.as_str() {
            // Source code - use LZMA (best compression for text)
            "rs" | "py" | "js" | "c" | "h" | "cpp" | "cc" | "cxx" | "go" | "java" | "rb"
            | "tsx" | "jsx" | "css" | "html" | "json" | "yaml" | "yml" | "xml" | "txt" | "md"
            | "toml" | "sh" | "bash" | "scala" | "kt" | "cs" | "vb" | "php" | "pl" | "lua"
            | "vim" | "lisp" | "clj" | "ex" | "erl" | "gradle" | "maven" | "sbt" => {
                CompressionAlgorithm::Lzma
            }

            // Images - already compressed, skip
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "ico" | "bmp" | "tiff" | "psd"
            | "heic" => CompressionAlgorithm::None,

            // Videos - already compressed, skip
            "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "m4v" | "wmv" | "3gp" | "m2ts"
            | "mts" | "ts" => CompressionAlgorithm::None,

            // Audio - already compressed, skip
            "mp3" | "aac" | "flac" | "wav" | "m4a" | "opus" => CompressionAlgorithm::None,

            // Archives - already compressed
            "zip" | "tar" | "gz" | "bz2" | "7z" | "rar" | "xz" => CompressionAlgorithm::None,

            // Everything else - use Zstandard as safe default
            _ => CompressionAlgorithm::Zstandard,
        }
    } else {
        CompressionAlgorithm::Zstandard
    }
}

pub fn compress_data(data: &Vec<u8>, algorithm: CompressionAlgorithm) -> Result<Vec<u8>> {
    return match algorithm {
        CompressionAlgorithm::None => Ok(data.clone()),
        CompressionAlgorithm::Brotli => Ok(compress_brotli(data)?),
        CompressionAlgorithm::Zstandard => Ok(compress_zstandard(data)?),
        CompressionAlgorithm::Lzma => Ok(compress_lzma(data)?),
    };
}

fn compress_brotli(data: &[u8]) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut params = brotli::enc::BrotliEncoderParams::default();
    params.quality = 11; // Maximum quality
    params.lgwin = 24; // Larger window size for better compression
    brotli::BrotliCompress(&mut std::io::Cursor::new(data), &mut output, &params)
        .map_err(|e| eyre!("Brotli compression error: {}", e))?;
    Ok(output)
}

fn compress_zstandard(data: &[u8]) -> Result<Vec<u8>> {
    zstd::encode_all(std::io::Cursor::new(data), 19) // Level 19 for better compression
        .map_err(|e| eyre!("Zstandard compression error: {}", e))
}

fn compress_lzma(data: &[u8]) -> Result<Vec<u8>> {
    use std::io::Write;
    let mut output = Vec::new();
    let mut encoder = xz2::write::XzEncoder::new(&mut output, 9); // Maximum compression
    encoder.write_all(data)?;
    encoder.finish()?;
    Ok(output)
}

pub fn decompress_data(compressed_data: Vec<u8>, entry: &ArchiveIndexEntry) -> Result<Vec<u8>> {
    return match entry.compression_algorithm {
        CompressionAlgorithm::None => Ok(compressed_data),
        CompressionAlgorithm::Brotli => {
            let mut decompressed = Vec::new();
            brotli::BrotliDecompress(
                &mut std::io::Cursor::new(&compressed_data),
                &mut decompressed,
            )
            .map_err(|e| eyre!("Failed to decompress {} with Brotli: {}", entry.path, e))?;
            return Ok(decompressed);
        }
        CompressionAlgorithm::Zstandard => {
            let decompressed = zstd::decode_all(std::io::Cursor::new(&compressed_data))
                .map_err(|e| eyre!("Failed to decompress {} with Zstandard: {}", entry.path, e))?;
            return Ok(decompressed);
        }
        CompressionAlgorithm::Lzma => {
            let mut decompressed = Vec::new();
            xz2::read::XzDecoder::new(std::io::Cursor::new(&compressed_data))
                .read_to_end(&mut decompressed)
                .map_err(|e| eyre!("Failed to decompress {} with LZMA: {}", entry.path, e))?;
            return Ok(decompressed);
        }
    };
}
