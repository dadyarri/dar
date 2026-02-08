use clap::ArgMatches;
use eyre::{Result, eyre};
use std::fs::{File, create_dir_all};
use std::io::{Read, Seek, Write};
use std::path::Path;

use crate::models::archive::{ArchiveHeader, CompressionAlgorithm};
use crate::terminal::success;

pub fn call(matches: &ArgMatches) -> Result<()> {
    let file_path = matches.get_one::<String>("file").expect("File required");
    let out_dir = matches.get_one::<String>("out").expect("Output directory required");
    let verbose = matches.get_flag("verbose");
    let _progress = matches.get_flag("progress");

    let mut archive_file = File::open(file_path)
        .map_err(|e| eyre!("Failed to open archive {}: {}", file_path, e))?;

    // Read and parse header
    let mut header_buf = [0u8; ArchiveHeader::SIZE];
    archive_file.read_exact(&mut header_buf)
        .map_err(|e| eyre!("Failed to read archive header: {}", e))?;

    // Verify magic
    if &header_buf[0..4] != ArchiveHeader::MAGIC {
        return Err(eyre!("Invalid archive format: wrong magic number"));
    }

    // Verify version
    if &header_buf[4..8] != ArchiveHeader::VERSION {
        return Err(eyre!("Unsupported archive version"));
    }

    // Parse header fields
    let data_section_start = u64::from_be_bytes([
        header_buf[8], header_buf[9], header_buf[10], header_buf[11],
        header_buf[12], header_buf[13], header_buf[14], header_buf[15],
    ]);

    let index_section_start = u64::from_be_bytes([
        header_buf[16], header_buf[17], header_buf[18], header_buf[19],
        header_buf[20], header_buf[21], header_buf[22], header_buf[23],
    ]);

    println!("Extracting archive {}...", file_path);

    // Create output directory if it doesn't exist
    create_dir_all(out_dir)
        .map_err(|e| eyre!("Failed to create output directory: {}", e))?;

    // Seek to index section and read all entries
    archive_file.seek(std::io::SeekFrom::Start(index_section_start))
        .map_err(|e| eyre!("Failed to seek to index section: {}", e))?;

    // Read entry count
    let mut entry_count_buf = [0u8; 4];
    archive_file.read_exact(&mut entry_count_buf)
        .map_err(|e| eyre!("Failed to read entry count: {}", e))?;
    let entry_count = u32::from_be_bytes(entry_count_buf);

    // Parse all index entries into a vector first
    #[derive(Clone)]
    struct IndexEntry {
        path: String,
        data_offset: u64,
        uncompressed_size: u64,
        compressed_size: u64,
        compression_algorithm: CompressionAlgorithm,
        _modification_time: u64,
        _permissions: u16,
    }

    let mut entries: Vec<IndexEntry> = Vec::new();

    for i in 0..entry_count {
        // Read entry length
        let mut entry_len_buf = [0u8; 4];
        if archive_file.read_exact(&mut entry_len_buf).is_err() {
            return Err(eyre!("Failed to read entry length for entry {}", i));
        }
        let entry_len = u32::from_be_bytes(entry_len_buf) as usize;
        
        if entry_len == 0 {
            return Err(eyre!("Entry {} has length 0, which is invalid", i));
        }

        // Read entire entry
        let mut entry_buf = vec![0u8; entry_len];
        if archive_file.read_exact(&mut entry_buf).is_err() {
            return Err(eyre!("Failed to read entry {} data (expected {} bytes)", i, entry_len));
        }

        // Parse path
        let path_len = u32::from_be_bytes([
            entry_buf[0], entry_buf[1], entry_buf[2], entry_buf[3],
        ]) as usize;

        let path = String::from_utf8(entry_buf[4..4 + path_len].to_vec())
            .map_err(|e| eyre!("Invalid UTF-8 in path for entry {}: {}", i, e))?;
        let mut offset = 4 + path_len;

        // Parse metadata
        let data_offset = u64::from_be_bytes([
            entry_buf[offset], entry_buf[offset+1], entry_buf[offset+2], entry_buf[offset+3],
            entry_buf[offset+4], entry_buf[offset+5], entry_buf[offset+6], entry_buf[offset+7],
        ]);
        offset += 8;

        let uncompressed_size = u64::from_be_bytes([
            entry_buf[offset], entry_buf[offset+1], entry_buf[offset+2], entry_buf[offset+3],
            entry_buf[offset+4], entry_buf[offset+5], entry_buf[offset+6], entry_buf[offset+7],
        ]);
        offset += 8;

        let compressed_size = u64::from_be_bytes([
            entry_buf[offset], entry_buf[offset+1], entry_buf[offset+2], entry_buf[offset+3],
            entry_buf[offset+4], entry_buf[offset+5], entry_buf[offset+6], entry_buf[offset+7],
        ]);
        offset += 8;

        let compression_byte = entry_buf[offset];
        offset += 1;

        let modification_time = u64::from_be_bytes([
            entry_buf[offset], entry_buf[offset+1], entry_buf[offset+2], entry_buf[offset+3],
            entry_buf[offset+4], entry_buf[offset+5], entry_buf[offset+6], entry_buf[offset+7],
        ]);
        offset += 8;

        let _uid = entry_buf[offset];
        offset += 1;

        let _gid = entry_buf[offset];
        offset += 1;

        let permissions = u16::from_be_bytes([entry_buf[offset], entry_buf[offset+1]]);

        // Reconstruct compression algorithm
        let compression_algorithm = match compression_byte {
            0 => CompressionAlgorithm::None,
            1 => CompressionAlgorithm::Brotli,
            2 => CompressionAlgorithm::Zstandard,
            _ => return Err(eyre!("Unknown compression algorithm: {}", compression_byte)),
        };

        entries.push(IndexEntry {
            path,
            data_offset,
            uncompressed_size,
            compressed_size,
            compression_algorithm,
            _modification_time: modification_time,
            _permissions: permissions,
        });
    }

    // Now process all entries
    for entry in entries {
        // Construct output file path
        let output_file_path = Path::new(out_dir).join(&entry.path);

        // Create parent directories
        if let Some(parent) = output_file_path.parent() {
            create_dir_all(parent)
                .map_err(|e| eyre!("Failed to create directories for {}: {}", entry.path, e))?;
        }

        // Read compressed data from archive
        archive_file.seek(std::io::SeekFrom::Start(data_section_start + entry.data_offset))
            .map_err(|e| eyre!("Failed to seek to data offset for {}: {}", entry.path, e))?;

        // Read entry length prefix (8 bytes)
        let mut entry_size_buf = [0u8; 8];
        archive_file.read_exact(&mut entry_size_buf)
            .map_err(|e| eyre!("Failed to read compressed data size for {}: {}", entry.path, e))?;
        let _actual_compressed_size = u64::from_be_bytes(entry_size_buf);

        // Read compressed data
        let mut compressed_data = vec![0u8; entry.compressed_size as usize];
        archive_file.read_exact(&mut compressed_data)
            .map_err(|e| eyre!("Failed to read compressed data for {}: {}", entry.path, e))?;

        // Decompress data
        let uncompressed_data = match entry.compression_algorithm {
            CompressionAlgorithm::None => compressed_data,
            CompressionAlgorithm::Brotli => {
                let mut decompressed = Vec::new();
                brotli::BrotliDecompress(
                    &mut std::io::Cursor::new(&compressed_data),
                    &mut decompressed,
                )
                .map_err(|e| eyre!("Failed to decompress {} with Brotli: {}", entry.path, e))?;
                decompressed
            }
            CompressionAlgorithm::Zstandard => {
                zstd::decode_all(std::io::Cursor::new(&compressed_data))
                    .map_err(|e| eyre!("Failed to decompress {} with Zstandard: {}", entry.path, e))?
            }
            CompressionAlgorithm::Lzma => {
                let mut decompressed = Vec::new();
                xz2::read::XzDecoder::new(std::io::Cursor::new(&compressed_data))
                    .read_to_end(&mut decompressed)
                    .map_err(|e| eyre!("Failed to decompress {} with LZMA: {}", entry.path, e))?;
                decompressed
            }
        };

        // Verify uncompressed size matches
        if uncompressed_data.len() as u64 != entry.uncompressed_size {
            return Err(eyre!(
                "Decompressed size mismatch for {}: expected {}, got {}",
                entry.path,
                entry.uncompressed_size,
                uncompressed_data.len()
            ));
        }

        // Write file
        let mut output_file = File::create(&output_file_path)
            .map_err(|e| eyre!("Failed to create output file {}: {}", entry.path, e))?;
        output_file.write_all(&uncompressed_data)
            .map_err(|e| eyre!("Failed to write to output file {}: {}", entry.path, e))?;

        // Set modification time using filetime
        #[cfg(unix)]
        {
            let mtime = std::time::UNIX_EPOCH + std::time::Duration::from_secs(entry._modification_time);
            let filetime = filetime::FileTime::from_system_time(mtime);
            let _ = filetime::set_file_mtime(&output_file_path, filetime);
        }

        if verbose {
            println!("  Extracted: {} ({} bytes)", entry.path, entry.uncompressed_size);
        }
    }

    success(&format!("Archive {} successfully extracted to {}!", file_path, out_dir));

    Ok(())
}
