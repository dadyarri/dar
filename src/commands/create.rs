use clap::ArgMatches;
use eyre::{Result, eyre};
use std::path::Path;

pub fn call(matches: &ArgMatches) -> Result<()> {
    let file = matches.get_one::<String>("file").expect("File required");

    if Path::new(file).exists() {
        return Err(eyre!("File {file} already exists"));
    }

    println!("Creating new archive {}...", file);
    return Ok(());
}
