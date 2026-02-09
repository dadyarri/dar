use clap::ArgMatches;
use eyre::{Result, eyre};
use ignore::WalkBuilder;
use std::fs::{File, canonicalize, metadata};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::archive::add_file;
use crate::models::archive::{ArchiveEndRecord, ArchiveHeader, ArchiveIndexEntry};
use crate::terminal::success;
use crate::utils::calculate_archive_path;

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
                    let file_meta = add_entry(
                        &entry.path(),
                        &progress,
                        &verbose,
                        &mut archive_bytes,
                        &absolute_path,
                        &data_section_start,
                    )?;
                    index_entries.push(file_meta);
                    file_count += 1;
                }
            }
        } else if absolute_path.is_file() {
            let file_meta = add_entry(
                absolute_path.as_path(),
                &progress,
                &verbose,
                &mut archive_bytes,
                &absolute_path,
                &data_section_start,
            )?;

            index_entries.push(file_meta);
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

fn add_entry(
    path: &Path,
    progress: &bool,
    verbose: &bool,
    mut archive_bytes: &mut Vec<u8>,
    absolute_path: &PathBuf,
    data_section_start: &u64,
) -> Result<ArchiveIndexEntry> {
    let current_offset = (archive_bytes.len() - *data_section_start as usize) as u64;
    let file_size = metadata(path)?.len();

    let mut file_meta = add_file(path, &mut archive_bytes, *progress)?;
    let archive_path = calculate_archive_path(path, &absolute_path);

    file_meta.path = archive_path.clone();
    file_meta.data_offset = current_offset;

    if *verbose {
        let ratio = if file_meta.compressed_size > 0 {
            (file_meta.compressed_size as f64 / file_size as f64) * 100.0
        } else {
            0.0
        };
        println!(
            "  Added: {:?} -> {} ({}B -> {}B, {:.1}%, {:?})",
            path,
            file_meta.path,
            file_size,
            file_meta.compressed_size,
            ratio,
            file_meta.compression_algorithm
        );
    }

    return Ok(file_meta);
}
