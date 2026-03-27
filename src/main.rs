use eyre::{Context, Result};

mod archive_builder;
mod cli;
mod commands;
mod counting_writer;
mod models;
mod pipeline;
mod traits;
mod utils;
mod walker;

fn main() -> Result<()> {
    let matches = cli::build_cli().get_matches();

    match matches.subcommand() {
        Some(("create", sub_matches)) => {
            commands::create::call(&sub_matches).wrap_err("Failed to create archive")?;
        }
        _ => unreachable!(),
    }

    Ok(())
}
