use clap::ArgMatches;
use eyre::{Result, eyre};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::archive::{
    calculate_archive_checksum, decompress_data, parse_index_entry, read_end_record, read_header,
};
use crate::models::archive::{ArchiveHeader, ArchiveIndexEntry};
use crate::terminal::success;

pub fn call(matches: &ArgMatches) -> Result<()> {
    let file = matches
        .get_one::<String>("file")
        .ok_or_else(|| eyre!("File required"))?;

    let verbose = matches.get_flag("verbose");
    let slow = matches.get_flag("slow");

    validate_archive(file, slow, verbose)?;

    Ok(())
}

fn validate_archive(path: &str, slow: bool, verbose: bool) -> Result<()> {
    if !Path::new(path).exists() {
        return Err(eyre!("Archive file not found: {}", path));
    }

    let file_size = std::fs::metadata(path)?.len();
    let mut file = File::open(path)?;

    // Basic checks
    if file_size < 512 {
        return Err(eyre!("Invalid archive: file too small"));
    }

    let (header, header_result) = read_header(&mut file);
    let (end_record, end_result) = read_end_record(&mut file, file_size);

    header_result?;
    end_result?;

    let h = header.unwrap();
    let e = end_record.unwrap();

    // Verify offsets
    if h.data_section_start >= file_size || h.index_section_start >= file_size {
        return Err(eyre!("Invalid archive: section offsets exceed file size"));
    }

    if h.index_section_start != e.index_offset {
        return Err(eyre!("Invalid archive: index offset mismatch"));
    }

    // Verify checksums
    let calculated = calculate_archive_checksum(&mut file, &h, file_size)?;
    if h.archive_checksum != calculated || e.archive_checksum != calculated {
        return Err(eyre!("Invalid archive: checksum mismatch"));
    }

    // Parse index
    file.seek(SeekFrom::Start(h.index_section_start))?;
    let mut buf = [0u8; 4];
    file.read_exact(&mut buf)?;
    let entry_count = u32::from_be_bytes(buf);

    // Verify file count matches header
    if entry_count != h.total_files {
        return Err(eyre!(
            "Invalid archive: file count mismatch (header: {}, index: {})",
            h.total_files,
            entry_count
        ));
    }

    let mut entries = Vec::new();
    let mut last_offset = 0u64;

    for i in 0..entry_count {
        let entry = parse_index_entry(&mut file)?;

        // Validate index structure
        if entry.path.is_empty() {
            return Err(eyre!("Invalid archive: entry {} has empty path", i));
        }

        // Check offsets don't overlap
        if entry.data_offset < last_offset {
            return Err(eyre!(
                "Invalid archive: entry {} offset goes backward ({} < {})",
                i,
                entry.data_offset,
                last_offset
            ));
        }
        last_offset = entry.data_offset + entry.compressed_size;

        // Validate offset is within bounds
        if h.data_section_start + entry.data_offset + entry.compressed_size > h.index_section_start
        {
            return Err(eyre!(
                "Invalid archive: entry {} extends beyond data section",
                i
            ));
        }

        entries.push(entry);
    }

    // Slow mode: thorough verification of all entries
    if slow {
        let mut verified = 0u32;
        for (i, entry) in entries.iter().enumerate() {
            verify_entry_data(&mut file, &h, entry)
                .map_err(|e| eyre!("Entry {} ({}) failed verification: {}", i, entry.path, e))?;
            verified += 1;

            if verbose && (i + 1) % 10 == 0 {
                println!("  Verified {}/{}", verified, entry_count);
            }
        }

        if verbose {
            println!("  Verified all {}", entry_count);
        }
    } else if verbose {
        println!("  {} files", entry_count);
    }

    success(&format!("Archive {} is valid", path));

    Ok(())
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
