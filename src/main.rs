use eyre::{Context, Result};

rust_i18n::i18n!("locales", fallback = "en");

mod archive_builder;
mod cli;
mod commands;
mod counting_writer;
mod encryption;
mod extra;
mod i18n;
mod models;
mod pipeline;
mod reader;
mod traits;
mod utils;
mod walker;

fn main() -> Result<()> {
    let locale = i18n::detect_locale();
    rust_i18n::set_locale(locale.as_str());
    let matches = cli::build_cli_with_translator(|key| {
        rust_i18n::t!(key, locale = locale.as_str()).to_string()
    })
    .get_matches();

    match matches.subcommand() {
        Some(("create", sub_matches)) => {
            commands::create::call(sub_matches, &locale).wrap_err(
                rust_i18n::t!(
                    "cli.create.errors.create_archive_failed",
                    locale = locale.as_str()
                )
                .to_string(),
            )?;
        }
        Some(("append", sub_matches)) => {
            commands::append::call(sub_matches, &locale).wrap_err(
                rust_i18n::t!("cli.append.errors.append_failed", locale = locale.as_str())
                    .to_string(),
            )?;
        }
        _ => unreachable!(),
    }

    Ok(())
}
