use clap::ArgMatches;
use eyre::{Result, eyre};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::models::archive::{ArchiveHeader, ArchiveEndRecord};
use crate::terminal::success;

/// Validation levels
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
enum ValidationLevel {
    /// Quick check: header, footer, overall checksum
    Basic,
    /// Detailed check: all of basic + index parsing
    Full,
    /// Slow check: all of full + individual entry checksums
    Slow,
}

/// Validation context for tracking results
struct ValidationContext {
    verbose: bool,
    _file_size: u64,
    checks_passed: u32,
    checks_failed: u32,
    errors: Vec<String>,
}

impl ValidationContext {
    fn new(file_size: u64, verbose: bool) -> Self {
        Self {
            verbose,
            _file_size: file_size,
            checks_passed: 0,
            checks_failed: 0,
            errors: Vec::new(),
        }
    }

    fn check(&mut self, name: &str, result: Result<()>) {
        match result {
            Ok(()) => {
                self.checks_passed += 1;
                if self.verbose {
                    println!("  ✓ {}", name);
                }
            }
            Err(e) => {
                self.checks_failed += 1;
                let msg = format!("{}: {}", name, e);
                self.errors.push(msg.clone());
                if self.verbose {
                    println!("  ✗ {}", msg);
                }
            }
        }
    }

    fn summary(&self) -> String {
        format!(
            "{} passed, {} failed",
            self.checks_passed, self.checks_failed
        )
    }

    fn is_valid(&self) -> bool {
        self.checks_failed == 0
    }
}

pub fn call(matches: &ArgMatches) -> Result<()> {
    let file = matches
        .get_one::<String>("file")
        .ok_or_else(|| eyre!("File required"))?;

    let verbose = matches.get_flag("verbose");
    let slow = matches.get_flag("slow");

    let level = if slow {
        ValidationLevel::Slow
    } else {
        ValidationLevel::Full
    };

    validate_archive(file, level, verbose)?;

    Ok(())
}

fn validate_archive(path: &str, level: ValidationLevel, verbose: bool) -> Result<()> {
    if !Path::new(path).exists() {
        return Err(eyre!("Archive file not found: {}", path));
    }

    let file_size = std::fs::metadata(path)?.len();
    let mut file = File::open(path)?;
    let mut ctx = ValidationContext::new(file_size, verbose);

    println!("Validating archive: {}", path);
    println!("File size: {} bytes", file_size);
    println!();

    // Basic validation
    println!("Basic Checks:");
    ctx.check("Header present (≥512 bytes)", check_min_size(&file, 512));
    ctx.check("End record present (≥64 bytes)", check_min_size(&file, 64));

    let (header, header_result) = read_header(&mut file);
    ctx.check("Header readable", header_result);

    let (end_record, end_result) = read_end_record(&mut file, file_size);
    ctx.check("End record readable", end_result);

    if header.is_some() && end_record.is_some() {
        let h = header.as_ref().unwrap();
        let e = end_record.as_ref().unwrap();

        ctx.check("Header magic valid", check_magic(b"DAR\0", "Header"));
        ctx.check("End record magic valid", check_magic(b"DEND", "End Record"));
        ctx.check(
            "Data section offset valid",
            check_offset(h.data_section_start, file_size, "Data"),
        );
        ctx.check(
            "Index section offset valid",
            check_offset(h.index_section_start, file_size, "Index"),
        );
        ctx.check(
            "Index offsets match (header vs end record)",
            if h.index_section_start == e.index_offset {
                Ok(())
            } else {
                Err(eyre!(
                    "Mismatch: header says {} but end record says {}",
                    h.index_section_start,
                    e.index_offset
                ))
            },
        );

        // Archive checksum verification
        println!("\nChecksum Verification:");
        match calculate_archive_checksum(&mut file, &h, file_size) {
            Ok(calculated) => {
                ctx.check(
                    "Archive checksum (header)",
                    if h.archive_checksum == calculated {
                        Ok(())
                    } else {
                        Err(eyre!("Header checksum mismatch"))
                    },
                );
                ctx.check(
                    "Archive checksum (end record)",
                    if e.archive_checksum == calculated {
                        Ok(())
                    } else {
                        Err(eyre!("End record checksum mismatch"))
                    },
                );
            }
            Err(e) => {
                ctx.check("Archive checksum calculation", Err(e));
            }
        }
    }

    // Full validation (index parsing)
    if matches!(level, ValidationLevel::Full | ValidationLevel::Slow) {
        println!("\nIndex Validation:");
        if let Some(ref header) = header {
            match validate_index(&mut file, header) {
                Ok((entry_count, index_entries)) => {
                    ctx.check(
                        &format!("Index readable ({} entries)", entry_count),
                        Ok(()),
                    );

                    // Validate offsets and sizes
                    for (i, entry) in index_entries.iter().enumerate() {
                        ctx.check(
                            &format!(
                                "Entry {} offset valid ({})",
                                i + 1,
                                &entry.path
                            ),
                            check_offset(entry.data_offset, file_size, "Data entry"),
                        );
                    }
                }
                Err(e) => {
                    ctx.check("Index readable", Err(e));
                }
            }
        }
    }

    // Slow validation (entry checksums)
    if matches!(level, ValidationLevel::Slow) {
        println!("\nEntry Checksum Verification (Slow Mode):");
        if let Some(ref header) = header {
            match validate_index(&mut file, header) {
                Ok((_, index_entries)) => {
                    for (i, entry) in index_entries.iter().enumerate() {
                        match verify_entry_data(&mut file, &header, &entry) {
                            Ok(()) => {
                                ctx.check(&format!("Entry {} checksum ({})", i + 1, entry.path), Ok(()));
                            }
                            Err(e) => {
                                ctx.check(&format!("Entry {} checksum ({})", i + 1, entry.path), Err(e));
                            }
                        }
                    }
                }
                Err(_) => {
                    println!("  ✗ Cannot validate entries: index not readable");
                }
            }
        }
    }

    // Summary
    println!("\n{}", "=".repeat(50));
    println!("Validation Summary: {}", ctx.summary());

    if ctx.is_valid() {
        success("Archive is valid!");
    } else {
        println!("\nErrors found:");
        for error in ctx.errors {
            println!("  • {}", error);
        }
        return Err(eyre!("Archive validation failed"));
    }

    Ok(())
}

/// Check minimum file size
fn check_min_size(file: &File, min_size: u64) -> Result<()> {
    file.metadata()?
        .len()
        .ge(&min_size)
        .then_some(())
        .ok_or_else(|| eyre!("File too small"))
}

/// Check magic bytes
fn check_magic(_expected: &[u8], _location: &str) -> Result<()> {
    // Magic checking is handled in read_header/read_end_record
    Ok(())
}

/// Check offset is valid within file bounds
fn check_offset(offset: u64, file_size: u64, location: &str) -> Result<()> {
    if offset < file_size {
        Ok(())
    } else {
        Err(eyre!(
            "{} offset {} exceeds file size {}",
            location,
            offset,
            file_size
        ))
    }
}

/// Read and parse archive header
fn read_header(file: &mut File) -> (Option<ArchiveHeader>, Result<()>) {
    file.seek(SeekFrom::Start(0)).ok();

    let mut buf = vec![0u8; 512];
    match file.read_exact(&mut buf) {
        Ok(()) => {
            // Check magic and version
            if &buf[0..3] != b"DAR" || &buf[4..8] != b"0003" {
                (None, Err(eyre!("Invalid header magic or version")))
            } else {
                // Parse header fields (big-endian)
                let data_section_start = u64::from_be_bytes([
                    buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
                ]);
                let index_section_start = u64::from_be_bytes([
                    buf[16], buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23],
                ]);
                let total_files = u32::from_be_bytes([buf[24], buf[25], buf[26], buf[27]]);

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
fn read_end_record(file: &mut File, file_size: u64) -> (Option<ArchiveEndRecord>, Result<()>) {
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
fn calculate_archive_checksum(
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

/// Parse and validate all index entries
fn validate_index(
    file: &mut File,
    header: &ArchiveHeader,
) -> Result<(u32, Vec<ParsedIndexEntry>)> {
    file.seek(SeekFrom::Start(header.index_section_start))?;

    let mut buf = [0u8; 4];
    file.read_exact(&mut buf)?;
    let entry_count = u32::from_be_bytes(buf);

    let mut entries = Vec::new();

    for _ in 0..entry_count {
        match parse_index_entry(file) {
            Ok(entry) => entries.push(entry),
            Err(e) => return Err(eyre!("Failed to parse index entry: {}", e)),
        }
    }

    Ok((entry_count, entries))
}

/// Parsed index entry for validation
#[derive(Debug, Clone)]
struct ParsedIndexEntry {
    path: String,
    data_offset: u64,
    uncompressed_size: u64,
    compressed_size: u64,
    compression_algorithm: u8,
    #[allow(dead_code)]
    modification_time: u64,
    #[allow(dead_code)]
    uid: u8,
    #[allow(dead_code)]
    gid: u8,
    #[allow(dead_code)]
    permissions: u16,
    checksum: [u8; 32],
}

/// Parse single index entry
fn parse_index_entry(file: &mut File) -> Result<ParsedIndexEntry> {
    let mut buf = [0u8; 4];
    file.read_exact(&mut buf)?;
    let entry_length = u32::from_be_bytes(buf) as usize;

    let mut entry_buf = vec![0u8; entry_length];
    file.read_exact(&mut entry_buf)?;

    let mut offset = 0;

    // Path length and path
    let path_len = u32::from_be_bytes([
        entry_buf[offset],
        entry_buf[offset + 1],
        entry_buf[offset + 2],
        entry_buf[offset + 3],
    ]) as usize;
    offset += 4;

    let path = String::from_utf8(entry_buf[offset..offset + path_len].to_vec())?;
    offset += path_len;

    // Metadata
    let data_offset = u64::from_be_bytes([
        entry_buf[offset],
        entry_buf[offset + 1],
        entry_buf[offset + 2],
        entry_buf[offset + 3],
        entry_buf[offset + 4],
        entry_buf[offset + 5],
        entry_buf[offset + 6],
        entry_buf[offset + 7],
    ]);
    offset += 8;

    let uncompressed_size = u64::from_be_bytes([
        entry_buf[offset],
        entry_buf[offset + 1],
        entry_buf[offset + 2],
        entry_buf[offset + 3],
        entry_buf[offset + 4],
        entry_buf[offset + 5],
        entry_buf[offset + 6],
        entry_buf[offset + 7],
    ]);
    offset += 8;

    let compressed_size = u64::from_be_bytes([
        entry_buf[offset],
        entry_buf[offset + 1],
        entry_buf[offset + 2],
        entry_buf[offset + 3],
        entry_buf[offset + 4],
        entry_buf[offset + 5],
        entry_buf[offset + 6],
        entry_buf[offset + 7],
    ]);
    offset += 8;

    let compression_algorithm = entry_buf[offset];
    offset += 1;

    let modification_time = u64::from_be_bytes([
        entry_buf[offset],
        entry_buf[offset + 1],
        entry_buf[offset + 2],
        entry_buf[offset + 3],
        entry_buf[offset + 4],
        entry_buf[offset + 5],
        entry_buf[offset + 6],
        entry_buf[offset + 7],
    ]);
    offset += 8;

    let uid = entry_buf[offset];
    offset += 1;
    let gid = entry_buf[offset];
    offset += 1;

    let permissions = u16::from_be_bytes([entry_buf[offset], entry_buf[offset + 1]]);
    offset += 2;

    let mut checksum = [0u8; 32];
    checksum.copy_from_slice(&entry_buf[offset..offset + 32]);

    Ok(ParsedIndexEntry {
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

/// Verify entry data by decompressing and checking checksum
fn verify_entry_data(
    file: &mut File,
    header: &ArchiveHeader,
    entry: &ParsedIndexEntry,
) -> Result<()> {
    let data_abs_offset = header.data_section_start + entry.data_offset;
    file.seek(SeekFrom::Start(data_abs_offset))?;

    // Read entry length
    let mut buf = [0u8; 8];
    file.read_exact(&mut buf)?;
    let entry_len = u64::from_be_bytes(buf);

    if entry_len != entry.compressed_size {
        return Err(eyre!(
            "Entry length mismatch: {} vs {}",
            entry_len,
            entry.compressed_size
        ));
    }

    // Read compressed data
    let mut compressed = vec![0u8; entry.compressed_size as usize];
    file.read_exact(&mut compressed)?;

    // Decompress
    let uncompressed = match entry.compression_algorithm {
        0 => compressed, // None
        1 => {
            // For Brotli, we need to use a different approach
            let mut decompressed = Vec::new();
            use std::io::Cursor;
            let mut decompressor = brotli::Decompressor::new(Cursor::new(&compressed[..]), 4096);
            std::io::Read::read_to_end(&mut decompressor, &mut decompressed)
                .map_err(|e| eyre!("Brotli decompression error: {}", e))?;
            decompressed
        }
        2 => zstd::decode_all(std::io::Cursor::new(&compressed))
            .map_err(|e| eyre!("Zstandard decompression error: {}", e))?,
        _ => return Err(eyre!("Unknown compression algorithm: {}", entry.compression_algorithm)),
    };

    // Verify size
    if uncompressed.len() as u64 != entry.uncompressed_size {
        return Err(eyre!(
            "Uncompressed size mismatch: {} vs {}",
            uncompressed.len(),
            entry.uncompressed_size
        ));
    }

    // Verify checksum
    let hash = blake3::hash(&uncompressed);
    let mut calculated = [0u8; 32];
    calculated.copy_from_slice(hash.as_bytes());

    if calculated != entry.checksum {
        return Err(eyre!("Checksum mismatch for entry"));
    }

    Ok(())
}
