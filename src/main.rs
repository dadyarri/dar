mod cli;
mod traits;
mod utils;

fn main() -> eyre::Result<()> {
    let matches = cli::build_cli().get_matches();

    Ok(())
}
