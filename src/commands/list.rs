use clap::ArgMatches;
use eyre::Result;
use std::fs::File;
use std::io::{Read, Seek};
use std::time::{UNIX_EPOCH, SystemTime};

use crate::models::archive::{ArchiveHeader, ArchiveIndexEntry};

pub fn call(matches: &ArgMatches) -> Result<()> {
    let file_path = matches.get_one::<String>("file").expect("File required");
    
    let mut file = File::open(file_path)?;
    
    // Read and parse header
    let mut header_buf = [0u8; ArchiveHeader::SIZE];
    file.read_exact(&mut header_buf)?;
    
    // Verify magic
    if &header_buf[0..4] != ArchiveHeader::MAGIC {
        eyre::bail!("Invalid archive format: wrong magic number");
    }
    
    // Verify version
    if &header_buf[4..8] != ArchiveHeader::VERSION {
        eyre::bail!("Unsupported archive version");
    }
    
    // Parse header fields
    let index_section_start = u64::from_be_bytes([
        header_buf[16], header_buf[17], header_buf[18], header_buf[19],
        header_buf[20], header_buf[21], header_buf[22], header_buf[23],
    ]);
    
    let total_files = u32::from_be_bytes([
        header_buf[24], header_buf[25], header_buf[26], header_buf[27],
    ]);
    
    let created_timestamp = u64::from_be_bytes([
        header_buf[28], header_buf[29], header_buf[30], header_buf[31],
        header_buf[32], header_buf[33], header_buf[34], header_buf[35],
    ]);
    
    // Seek to index section
    file.seek(std::io::SeekFrom::Start(index_section_start))?;
    
    // Read entry count
    let mut entry_count_buf = [0u8; 4];
    file.read_exact(&mut entry_count_buf)?;
    let entry_count = u32::from_be_bytes(entry_count_buf);
    
    // Display archive header info
    let created_datetime = UNIX_EPOCH + std::time::Duration::from_secs(created_timestamp);
    let created_str = humanize_time(created_datetime);
    
    println!("Archive: {}", file_path);
    println!("Created: {}", created_str);
    println!("Total Files: {}", entry_count);
    println!("{:-<80}", "");
    println!("{:<60} {:>8} {:>10}", "Path", "Size", "Compressed");
    println!("{:-<80}", "");
    
    // Parse and display each index entry
    let mut total_uncompressed = 0u64;
    let mut total_compressed = 0u64;
    
    for _ in 0..entry_count {
        // Read entry length
        let mut entry_len_buf = [0u8; 4];
        file.read_exact(&mut entry_len_buf)?;
        let entry_len = u32::from_be_bytes(entry_len_buf) as usize;
        
        // Read entire entry
        let mut entry_buf = vec![0u8; entry_len];
        file.read_exact(&mut entry_buf)?;
        
        // Parse path length and path
        let path_len = u32::from_be_bytes([
            entry_buf[0], entry_buf[1], entry_buf[2], entry_buf[3],
        ]) as usize;
        
        let path = String::from_utf8(entry_buf[4..4 + path_len].to_vec())?;
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
        
        // Display entry
        let display_path = if path.len() > 60 {
            format!("{}...{}", &path[..27], &path[path.len()-30..])
        } else {
            path.clone()
        };
        
        println!("{:<60} {:>8} {:>10}",
            display_path,
            format_size(uncompressed_size),
            format_size(compressed_size)
        );
        
        total_uncompressed += uncompressed_size;
        total_compressed += compressed_size;
    }
    
    println!("{:-<80}", "");
    println!("{:<60} {:>8} {:>10}",
        "TOTAL",
        format_size(total_uncompressed),
        format_size(total_compressed)
    );
    
    Ok(())
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    
    if unit_idx == 0 {
        format!("{}", bytes)
    } else {
        format!("{:.1}{}", size, UNITS[unit_idx])
    }
}

fn humanize_time(time: SystemTime) -> String {
    match time.duration_since(UNIX_EPOCH) {
        Ok(_) => {
            // Simple ISO-like format
            let duration = time.duration_since(UNIX_EPOCH).unwrap();
            let secs = duration.as_secs();
            let days_since_epoch = secs / 86400;
            let seconds_today = secs % 86400;
            
            let hours = seconds_today / 3600;
            let minutes = (seconds_today % 3600) / 60;
            let seconds = seconds_today % 60;
            
            // Approximate date calculation (not accounting for leap seconds)
            let year = 1970 + (days_since_epoch / 365);
            let day_of_year = days_since_epoch % 365;
            let month = (day_of_year / 31).min(11) + 1;
            let day = (day_of_year % 31) + 1;
            
            format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, hours, minutes, seconds)
        }
        Err(_) => "Unknown".to_string(),
    }
}
