use clap::Command;
use clap::{crate_authors, crate_description, crate_version};

pub fn build_cli() -> Command {
    Command::new("dari")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .disable_help_flag(true)
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
}
