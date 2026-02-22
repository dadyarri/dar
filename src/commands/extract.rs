use clap::ArgMatches;
use eyre::{Result, eyre};
use filetime::{FileTime, set_file_mtime};
use std::fs::{File, create_dir_all};
use std::io::{Read, Seek, Write};
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use crate::archive::{decompress_data, parse_index_entry};
use crate::models::archive::{ArchiveHeader, ArchiveIndexEntry};
use crate::terminal::success;

/// Check if a path matches a glob pattern
fn matches_glob(path: &str, pattern: &str) -> bool {
    // Simple glob pattern matching
    if pattern == "*" || pattern.is_empty() {
        return true;
    }

    // Handle patterns like *.rs, dir/*.txt
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        match parts.len() {
            2 => {
                let prefix = parts[0];
                let suffix = parts[1];
                if !prefix.is_empty() && !path.starts_with(prefix) {
                    return false;
                }
                if !suffix.is_empty() && !path.ends_with(suffix) {
                    return false;
                }
                true
            }
            _ => {
                // Multiple wildcards - for simplicity, just check containment
                let mut current_pos = 0;
                for part in parts {
                    if part.is_empty() {
                        continue;
                    }
                    match path[current_pos..].find(part) {
                        Some(pos) => current_pos += pos + part.len(),
                        None => return false,
                    }
                }
                true
            }
        }
    } else {
        // Exact match or prefix match (directory)
        path == pattern || path.starts_with(&format!("{}/", pattern))
    }
}

/// Strip a prefix from a path
fn strip_prefix(path: &str, prefix: &str) -> String {
    let prefix_with_slash = if prefix.ends_with('/') {
        prefix.to_string()
    } else {
        format!("{}/", prefix)
    };

    if let Some(stripped) = path.strip_prefix(&prefix_with_slash) {
        stripped.to_string()
    } else if path == prefix {
        String::new()
    } else if path.starts_with(prefix) {
        path[prefix.len()..].to_string()
    } else {
        path.to_string()
    }
}

pub fn call(matches: &ArgMatches) -> Result<()> {
    let file_path = matches.get_one::<String>("file").expect("File required");
    let out_dir = matches
        .get_one::<String>("out")
        .expect("Output directory required");
    let verbose = matches.get_flag("verbose");
    let _progress = matches.get_flag("progress");
    let dry_run = matches.get_flag("dry-run");
    let strip_path = matches.get_one::<String>("strip-path");

    // Get patterns to match (if any provided)
    let patterns: Vec<String> = matches
        .get_many::<String>("entries")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();

    let mut archive_file =
        File::open(file_path).map_err(|e| eyre!("Failed to open archive {}: {}", file_path, e))?;

    // Read and parse header
    let mut header_buf = [0u8; ArchiveHeader::SIZE];
    archive_file
        .read_exact(&mut header_buf)
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
    let data_section_start = u64::from_be_bytes(header_buf[8..16].try_into().unwrap());
    let index_section_start = u64::from_be_bytes(header_buf[16..24].try_into().unwrap());

    if !dry_run {
        println!("Extracting from {}", file_path);
    } else {
        println!("Preview: {}", file_path);
    }

    // Create output directory if it doesn't exist (skip for dry-run)
    if !dry_run {
        create_dir_all(out_dir).map_err(|e| eyre!("Failed to create output directory: {}", e))?;
    }

    // Seek to index section and read all entries
    archive_file
        .seek(std::io::SeekFrom::Start(index_section_start))
        .map_err(|e| eyre!("Failed to seek to index section: {}", e))?;

    // Read entry count
    let mut entry_count_buf = [0u8; 4];
    archive_file
        .read_exact(&mut entry_count_buf)
        .map_err(|e| eyre!("Failed to read entry count: {}", e))?;
    let entry_count = u32::from_be_bytes(entry_count_buf);

    let mut entries: Vec<ArchiveIndexEntry> = Vec::new();

    (0..entry_count).for_each(|_| {
        // Read entry length
        let entry = parse_index_entry(&mut archive_file).unwrap();
        entries.push(entry);
    });

    // Filter entries based on patterns
    let matching_entries: Vec<ArchiveIndexEntry> = entries
        .into_iter()
        .filter(|entry| {
            if patterns.is_empty() {
                true
            } else {
                patterns.iter().any(|pattern| matches_glob(&entry.path, pattern))
            }
        })
        .collect();

    // Check if any entries matched
    if matching_entries.is_empty() && !patterns.is_empty() {
        let pattern_str = patterns.join(", ");
        return Err(eyre!(
            "No files matched the pattern(s): {}",
            pattern_str
        ));
    }

    // Print or extract matched entries
    let mut extracted_count = 0;

    for entry in matching_entries {
        // Calculate output path
        let output_path = if let Some(prefix) = strip_path {
            strip_prefix(&entry.path, prefix)
        } else {
            entry.path.clone()
        };

        if output_path.is_empty() {
            continue; // Skip if path after stripping is empty
        }

        let output_file_path = Path::new(out_dir).join(&output_path);
        extracted_count += 1;

        if dry_run {
            println!("  {}", output_path);
            continue;
        }

        // Create parent directories
        if let Some(parent) = output_file_path.parent() {
            create_dir_all(parent)
                .map_err(|e| eyre!("Failed to create directories for {}: {}", entry.path, e))?;
        }

        // Read compressed data from archive
        archive_file
            .seek(std::io::SeekFrom::Start(
                data_section_start + entry.data_offset,
            ))
            .map_err(|e| eyre!("Failed to seek to data offset for {}: {}", entry.path, e))?;

        // Read entry length prefix (8 bytes)
        let mut entry_size_buf = [0u8; 8];
        archive_file.read_exact(&mut entry_size_buf).map_err(|e| {
            eyre!(
                "Failed to read compressed data size for {}: {}",
                entry.path,
                e
            )
        })?;
        let _actual_compressed_size = u64::from_be_bytes(entry_size_buf);

        // Read compressed data
        let mut compressed_data = vec![0u8; entry.compressed_size as usize];
        archive_file
            .read_exact(&mut compressed_data)
            .map_err(|e| eyre!("Failed to read compressed data for {}: {}", entry.path, e))?;

        // Decompress data
        let uncompressed_data = decompress_data(compressed_data, &entry).unwrap();

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
        output_file
            .write_all(&uncompressed_data)
            .map_err(|e| eyre!("Failed to write to output file {}: {}", entry.path, e))?;

        // Set modification time using filetime
        #[cfg(unix)]
        {
            let mtime = UNIX_EPOCH + Duration::from_secs(entry.modification_time);
            let filetime = FileTime::from_system_time(mtime);
            let _ = set_file_mtime(&output_file_path, filetime);
        }

        if verbose {
            println!("  {}", output_path);
        }
    }

    // Print summary
    if dry_run {
        let file_word = if extracted_count == 1 { "file" } else { "files" };
        println!("\nWould extract {} {}", extracted_count, file_word);
    } else {
        let file_word = if extracted_count == 1 { "file" } else { "files" };
        success(&format!(
            "Extracted {} {} to {}",
            extracted_count, file_word, out_dir
        ));
    }

    Ok(())
}
