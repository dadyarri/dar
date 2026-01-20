#[macro_use]
extern crate clap;
extern crate clap_complete;

use clap::ValueEnum;
use clap_complete::{generate_to, Shell};

include!("src/cli.rs");

fn main() {
    let mut app = build_cli();
    for &shell in Shell::value_variants() {
        generate_to(shell, &mut app, "dar", "completions/").ok();
    }
}
