use clap::ArgMatches;
use eyre::{OptionExt, Result, eyre};
use std::{fs::File, io::Write, path::Path};

use crate::models::archive::ArchiveHeader;
use crate::terminal::success;

pub fn call(matches: &ArgMatches) -> Result<()> {
    let file = matches
        .get_one::<String>("file")
        .ok_or_eyre("File required")
        .unwrap();

    if Path::new(file).exists() {
        return Err(eyre!("File {file} already exists"));
    }

    println!("Creating new archive {}...", file);

    let mut archive_bytes: Vec<u8> = Vec::new();
    ArchiveHeader::write_to(&mut archive_bytes)?;

    let mut archive_file = File::create(file)?;
    archive_file.write_all(&archive_bytes)?;
    archive_file.flush()?;

    success(&format!("Archive {} successfully created!", file));

    return Ok(());
}
