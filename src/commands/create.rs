use crate::archive_builder::ArchiveBuilder;
use crate::walker::scan_files;
use clap::ArgMatches;
use eyre::{eyre, Context, Result};
use std::fs;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

pub fn call(matches: &ArgMatches) -> Result<()> {
    let file = matches
        .get_one::<String>("file")
        .ok_or_else(|| eyre!("File required"))?;

    let verbose = matches.get_flag("verbose");
    let progress = matches.get_flag("progress");
    let overwrite = matches.get_flag("overwrite");
    let content = matches.get_many::<String>("content").unwrap();

    if Path::new(file).exists() && !overwrite {
        return Err(eyre!("File {} already exists", file));
    }

    if Path::new(file).exists() && overwrite {
        fs::remove_file(file).wrap_err("Failed to delete file")?;
    }

    println!("Creating archive {}", file);

    let file_handle = File::create(file).wrap_err("Failed to create file")?;
    let writer = BufWriter::new(file_handle);

    let mut builder = ArchiveBuilder::new(writer);
    builder.write_header()?;

    for file_entry in scan_files(content)? {
        if verbose {
            println!("{}", file_entry.display());
        }

        builder.add_file(&file_entry)?
    }

    builder.build()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::archive_builder::ArchiveBuilder;
    use crate::utils::{get_unix_timestamp, read_bytes_as, read_string};
    use std::io::Cursor;

    #[test]
    fn test_archive_header_writing() {
        // Arrange
        let mut buffer = Cursor::new(Vec::new());

        // Act
        let mut builder = ArchiveBuilder::new(&mut buffer);
        builder.write_header().unwrap();

        // Assert
        let data = buffer.into_inner();
        assert!(!data.is_empty(), "Archive data should not be empty");
        assert!(
            read_string(&*data, 0, 4).is_ok(),
            "Failed to read archive signature"
        );
        assert_eq!(
            read_string(&*data, 0, 4).unwrap(),
            "DARI".to_string(),
            "Invalid archive signature"
        );
        assert!(
            read_bytes_as::<u8>(&*data, 4).is_ok(),
            "Failed to read archive version"
        );
        assert_eq!(
            read_bytes_as::<u8>(&*data, 4).unwrap(),
            5,
            "Invalid archive version"
        );
        assert!(
            read_bytes_as::<u64>(&*data, 5).is_ok(),
            "Failed to read archive creation timestamp"
        );
        assert!(
            read_bytes_as::<u64>(&*data, 5).unwrap() <= get_unix_timestamp().unwrap(),
            "Invalid archive creation timestamp"
        );
    }
}
