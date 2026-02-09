use clap::ArgMatches;
use eyre::{Result, eyre};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::archive::{
    calculate_archive_checksum, decompress_data, parse_index_entry, read_end_record, read_header,
};
use crate::models::archive::{ArchiveHeader, ArchiveIndexEntry};
use crate::pager::PagerWriter;
use crate::terminal::success;
use crate::validation::{ValidationContext, ValidationLevel};

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
    let output = PagerWriter::new()?;
    let mut ctx = ValidationContext::new(file_size, verbose, output);

    let _ = ctx.writeln(format_args!("Validating archive: {}", path));
    let _ = ctx.writeln(format_args!("File size: {} bytes", file_size));
    let _ = ctx.writeln(format_args!(""));

    // Basic validation
    let _ = ctx.writeln(format_args!("Basic Checks:"));
    ctx.check("Header present (≥512 bytes)", check_min_size(&file, 512));
    ctx.check("End record present (≥64 bytes)", check_min_size(&file, 64));

    let (header, header_result) = read_header(&mut file);
    ctx.check("Header readable", header_result);

    let (end_record, end_result) = read_end_record(&mut file, file_size);
    ctx.check("End record readable", end_result);

    if header.is_some() && end_record.is_some() {
        let h = header.as_ref().unwrap();
        let e = end_record.as_ref().unwrap();

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
        let _ = ctx.writeln(format_args!("\nChecksum Verification:"));
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
        let _ = ctx.writeln(format_args!("\nIndex Validation:"));
        if let Some(ref header) = header {
            match validate_index(&mut file, header) {
                Ok((entry_count, index_entries)) => {
                    ctx.check(&format!("Index readable ({} entries)", entry_count), Ok(()));

                    // Validate offsets and sizes
                    for (i, entry) in index_entries.iter().enumerate() {
                        ctx.check(
                            &format!("Entry {} offset valid ({})", i + 1, &entry.path),
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
        let _ = ctx.writeln(format_args!("\nEntry Checksum Verification (Slow Mode):"));
        if let Some(ref header) = header {
            match validate_index(&mut file, header) {
                Ok((_, index_entries)) => {
                    for (i, entry) in index_entries.iter().enumerate() {
                        match verify_entry_data(&mut file, &header, &entry) {
                            Ok(()) => {
                                ctx.check(
                                    &format!("Entry {} checksum ({})", i + 1, entry.path),
                                    Ok(()),
                                );
                            }
                            Err(e) => {
                                ctx.check(
                                    &format!("Entry {} checksum ({})", i + 1, entry.path),
                                    Err(e),
                                );
                            }
                        }
                    }
                }
                Err(_) => {
                    let _ = ctx.writeln(format_args!(
                        "  ✗ Cannot validate entries: index not readable"
                    ));
                }
            }
        }
    }

    // Summary
    let _ = ctx.writeln(format_args!("\n{}", "=".repeat(50)));
    let _ = ctx.writeln(format_args!("Validation Summary: {}", ctx.summary()));

    if ctx.is_valid() {
        success("Archive is valid!");
    } else {
        let _ = ctx.writeln(format_args!("\nErrors found:"));
        let errors = ctx.errors.clone();
        for error in &errors {
            let _ = ctx.writeln(format_args!("  • {}", error));
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

/// Parse and validate all index entries
fn validate_index(
    file: &mut File,
    header: &ArchiveHeader,
) -> Result<(u32, Vec<ArchiveIndexEntry>)> {
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

/// Verify entry data by decompressing and checking checksum
fn verify_entry_data(
    file: &mut File,
    header: &ArchiveHeader,
    entry: &ArchiveIndexEntry,
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
    let uncompressed = decompress_data(compressed, &entry).unwrap();

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
