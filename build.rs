use clap::{crate_authors, crate_description, crate_version};

use clap::ValueEnum;
use clap_complete::{generate_to, Shell};

include!("src/cli.rs");

fn main() {
    let mut app = build_cli();
    for &shell in Shell::value_variants() {
        generate_to(shell, &mut app, "dar", "completions/").ok();
    }
}
