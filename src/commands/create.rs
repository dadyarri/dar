use clap::ArgMatches;
use eyre::{Result, eyre};
use std::fs::{File, canonicalize, metadata};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use ignore::WalkBuilder;

use crate::models::archive::{
    ArchiveEndRecord, ArchiveHeader, ArchiveIndexEntry, CompressionAlgorithm,
};
use crate::terminal::success;

const CHUNK_SIZE: usize = 512 * 1024; // 512KB

pub fn call(matches: &ArgMatches) -> Result<()> {
    let file = matches
        .get_one::<String>("file")
        .ok_or_else(|| eyre!("File required"))?;

    let verbose = matches.get_flag("verbose");
    let progress = matches.get_flag("progress");
    let content = matches.get_many::<String>("content").unwrap();

    if Path::new(file).exists() {
        return Err(eyre!("File {} already exists", file));
    }

    println!("Creating new archive {}...", file);

    // Reserve space for header (512 bytes)
    let mut archive_bytes: Vec<u8> = Vec::new();
    let header_offset = archive_bytes.len();
    let dummy_header = ArchiveHeader::new(0, 0, 0);
    dummy_header.write_to(&mut archive_bytes)?;

    // Data section starts after header
    let data_section_start = archive_bytes.len() as u64;
    let mut index_entries: Vec<ArchiveIndexEntry> = Vec::new();
    let mut file_count = 0u32;

    for item in content {
        let relative_path = Path::new(item);
        let absolute_path = canonicalize(relative_path)
            .map_err(|e| eyre!("Couldn't get absolute path for {:?}: {}", relative_path, e))?;

        if absolute_path.is_dir() {
            let walker = WalkBuilder::new(&absolute_path)
                .git_ignore(true)
                .hidden(false)
                .build();
            for entry in walker {
                let entry = entry?;
                if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    let file_path = entry.path();
                    let current_offset = (archive_bytes.len() - data_section_start as usize) as u64;
                    let file_size = metadata(file_path)?.len();
                    let algorithm = get_compression_algorithm(file_path);

                    let file_meta = add_file(file_path, &mut archive_bytes, progress, algorithm)?;

                    // Calculate relative path from the original argument (directory root)
                    let archive_path = calculate_archive_path(file_path, &absolute_path);

                    if verbose {
                        let ratio = if file_meta.compressed_size > 0 {
                            (file_meta.compressed_size as f64 / file_size as f64) * 100.0
                        } else {
                            0.0
                        };
                        println!(
                            "  Added: {:?} -> {} ({}B -> {}B, {:.1}%, {:?})",
                            file_path,
                            archive_path,
                            file_size,
                            file_meta.compressed_size,
                            ratio,
                            algorithm
                        );
                    }

                    index_entries.push(ArchiveIndexEntry {
                        path: archive_path,
                        data_offset: current_offset,
                        uncompressed_size: file_size,
                        compressed_size: file_meta.compressed_size,
                        compression_algorithm: algorithm,
                        modification_time: file_meta.modification_time,
                        uid: file_meta.uid,
                        gid: file_meta.gid,
                        permissions: file_meta.permissions,
                        checksum: file_meta.checksum,
                    });
                    file_count += 1;
                }
            }
        } else if absolute_path.is_file() {
            let current_offset = (archive_bytes.len() - data_section_start as usize) as u64;
            let file_size = metadata(&absolute_path)?.len();
            let algorithm = get_compression_algorithm(&absolute_path);

            let file_meta = add_file(&absolute_path, &mut archive_bytes, progress, algorithm)?;

            // For a single file, use just the filename
            let archive_path = absolute_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_string();

            if verbose {
                let ratio = if file_meta.compressed_size > 0 {
                    (file_meta.compressed_size as f64 / file_size as f64) * 100.0
                } else {
                    0.0
                };
                println!(
                    "  Added: {:?} -> {} ({}B -> {}B, {:.1}%, {:?})",
                    absolute_path,
                    archive_path,
                    file_size,
                    file_meta.compressed_size,
                    ratio,
                    algorithm
                );
            }

            index_entries.push(ArchiveIndexEntry {
                path: archive_path,
                data_offset: current_offset,
                uncompressed_size: file_size,
                compressed_size: file_meta.compressed_size,
                compression_algorithm: algorithm,
                modification_time: file_meta.modification_time,
                uid: file_meta.uid,
                gid: file_meta.gid,
                permissions: file_meta.permissions,
                checksum: file_meta.checksum,
            });
            file_count += 1;
        } else {
            println!("Skipping (not file/dir): {:?}", absolute_path);
        }
    }

    // Index section starts after data
    let index_section_start = archive_bytes.len() as u64;
    let index_start_len = archive_bytes.len();

    // Write index entry count
    archive_bytes.write_all(&file_count.to_be_bytes())?;

    // Write each index entry
    for entry in index_entries {
        entry.write_to(&mut archive_bytes)?;
    }

    let index_length = (archive_bytes.len() - index_start_len) as u64;
    // End record section starts after index
    let end_record_offset = archive_bytes.len() as u64;

    // Write end record with placeholder checksum
    let end_record = ArchiveEndRecord::new(index_section_start, index_length);
    end_record.write_to(&mut archive_bytes)?;

    // Update header with correct offsets and file count (BEFORE checksum calculation)
    // Bytes 8-15: data_section_start
    archive_bytes[header_offset + 8..header_offset + 16]
        .copy_from_slice(&data_section_start.to_be_bytes());
    // Bytes 16-23: index_section_start
    archive_bytes[header_offset + 16..header_offset + 24]
        .copy_from_slice(&index_section_start.to_be_bytes());
    // Bytes 24-27: total_files
    archive_bytes[header_offset + 24..header_offset + 28]
        .copy_from_slice(&file_count.to_be_bytes());

    // NOW calculate archive checksum (everything except the checksum fields themselves)
    // Skip bytes 36-67 in header (where archive_checksum is stored)
    let mut hasher = blake3::Hasher::new();
    hasher.update(&archive_bytes[0..36]); // header up to checksum field
    hasher.update(&[0u8; 32]); // skip checksum field in header
    hasher.update(&archive_bytes[68..end_record_offset as usize]); // rest up to end record

    // Also skip checksum in end record (bytes 20-51 within the 64-byte end record)
    let end_record_start = end_record_offset as usize;
    hasher.update(&archive_bytes[end_record_start..end_record_start + 20]); // magic + offsets
    hasher.update(&[0u8; 32]); // skip checksum in end record
    hasher.update(&archive_bytes[end_record_start + 52..]); // rest of end record
    let archive_hash = hasher.finalize();

    // Update both checksums
    // Bytes 36-68: archive_checksum in header
    archive_bytes[header_offset + 36..header_offset + 68].copy_from_slice(archive_hash.as_bytes());

    // Checksum in end record is at offset 20-52 within the 64-byte end record
    archive_bytes[end_record_offset as usize + 20..end_record_offset as usize + 52]
        .copy_from_slice(archive_hash.as_bytes());

    let mut archive_file = File::create(file)?;
    archive_file.write_all(&archive_bytes)?;
    archive_file.flush()?;

    success(&format!("Archive {} successfully created!", file));

    Ok(())
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

fn calculate_archive_path(file_path: &Path, dir_root: &Path) -> String {
    // Calculate relative path from directory root
    let relative = file_path.strip_prefix(dir_root).unwrap_or(file_path);

    // Sanitize path: convert to string and normalize
    let path_str = relative.to_string_lossy().to_string();
    sanitize_path(&path_str)
}

fn sanitize_path(path: &str) -> String {
    let mut components = Vec::new();
    let path_obj = PathBuf::from(path);

    for component in path_obj.components() {
        match component {
            Component::ParentDir | Component::RootDir => {
                // Skip parent directory and root directory references
            }
            Component::Normal(name) => {
                if let Some(s) = name.to_str() {
                    components.push(s.to_string());
                }
            }
            Component::CurDir => {
                // Skip current directory references
            }
            Component::Prefix(_) => {
                // Skip prefix components (Windows drive letters, etc.)
            }
        }
    }

    components.join("/")
}

fn add_file(
    path: &Path,
    archive_bytes: &mut Vec<u8>,
    progress: bool,
    algorithm: CompressionAlgorithm,
) -> Result<ArchiveIndexEntry> {
    let fs_meta = metadata(path)?;
    let file_size = fs_meta.len() as usize;

    // Extract file metadata
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
        let mut all_data = Vec::new();
        let mut bytes_read_total = 0usize;

        loop {
            let bytes_read = std::io::Read::read(&mut file, &mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            all_data.extend_from_slice(&buffer[..bytes_read]);
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

        let hash = blake3::hash(&all_data);
        let mut checksum = [0u8; 32];
        checksum.copy_from_slice(hash.as_bytes());

        let compressed_data = match algorithm {
            CompressionAlgorithm::None => all_data.clone(),
            CompressionAlgorithm::Brotli => compress_brotli(&all_data)?,
            CompressionAlgorithm::Zstandard => compress_zstandard(&all_data)?,
            CompressionAlgorithm::Lzma => compress_lzma(&all_data)?,
        };

        let compressed_size = compressed_data.len() as u64;

        // Write entry length prefix
        archive_bytes.write_all(&(compressed_data.len() as u64).to_be_bytes())?;
        // Write compressed data
        archive_bytes.write_all(&compressed_data)?;

        Ok(ArchiveIndexEntry {
            path: path.display().to_string(),
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
    } else {
        // Small file: read all at once
        let data = std::fs::read(path)?;

        let hash = blake3::hash(&data);
        let mut checksum = [0u8; 32];
        checksum.copy_from_slice(hash.as_bytes());

        let compressed_data = match algorithm {
            CompressionAlgorithm::None => data.clone(),
            CompressionAlgorithm::Brotli => compress_brotli(&data)?,
            CompressionAlgorithm::Zstandard => compress_zstandard(&data)?,
            CompressionAlgorithm::Lzma => compress_lzma(&data)?,
        };

        let compressed_size = compressed_data.len() as u64;

        // Write entry length prefix
        archive_bytes.write_all(&(compressed_data.len() as u64).to_be_bytes())?;
        // Write compressed data
        archive_bytes.write_all(&compressed_data)?;

        Ok(ArchiveIndexEntry {
            path: path.display().to_string(),
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
