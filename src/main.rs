#[macro_use]
extern crate clap;

extern crate term;

mod cli;
mod errors;
mod terminal;

fn main() {
    let matches = cli::build_cli().get_matches();
}
