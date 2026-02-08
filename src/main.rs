#[macro_use]
extern crate clap;
extern crate term;

mod cli;
mod commands;
mod models;
mod terminal;
mod pager;

fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let matches = cli::build_cli().get_matches();

    match matches.subcommand() {
        Some(("create", sub_matches)) => {
            commands::create::call(&sub_matches)?;
        }
        Some(("extract", sub_matches)) => {
            commands::extract::call(&sub_matches)?;
        }
        Some(("list", sub_matches)) => {
            commands::list::call(&sub_matches)?;
        }
        Some(("append", sub_matches)) => {
            commands::append::call(&sub_matches);
        }
        Some(("defragment", sub_matches)) => {
            commands::defragment::call(&sub_matches);
        }
        Some(("validate", sub_matches)) => {
            commands::validate::call(&sub_matches)?;
        }
        _ => unreachable!(),
    };

    Ok(())
}
