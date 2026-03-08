use eyre::Result;

mod cli;
mod commands;
mod models;
mod traits;
mod utils;

fn main() -> Result<()> {
    let matches = cli::build_cli().get_matches();

    match matches.subcommand() {
        Some(("create", sub_matches)) => {
            commands::create::call(&sub_matches)?;
        }
        _ => unreachable!(),
    }

    Ok(())
}
