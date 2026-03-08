use crate::models::archive::ArchiveHeader;
use clap::ArgMatches;
use eyre::{eyre, Context, Result};
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};
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

    if overwrite {
        fs::remove_file(file).wrap_err("Failed to delete file")?;
    }

    println!("Creating archive {}", file);

    let file_handle = File::create(file).wrap_err("Failed to create file")?;
    let mut writer = BufWriter::new(file_handle);

    ArchiveHeader::new()
        .write(&mut writer)
        .wrap_err("Failed to write archive header")?;

    writer.flush().wrap_err("Failed to flush archive")?;

    Ok(())
}
