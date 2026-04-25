use dari::commands;
use dari::i18n;
use eyre::{Context, Result};

rust_i18n::i18n!("locales", fallback = "en");

fn main() -> Result<()> {
    let locale = i18n::detect_locale();
    rust_i18n::set_locale(locale.as_str());
    let matches = dari::cli::build_cli_with_translator(|key| {
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
        Some(("inspect", sub_matches)) => {
            commands::inspect::call(sub_matches, &locale).wrap_err(
                rust_i18n::t!(
                    "cli.inspect.errors.inspect_failed",
                    locale = locale.as_str()
                )
                .to_string(),
            )?;
        }
        Some(("extract", sub_matches)) => {
            commands::extract::call(sub_matches, &locale).wrap_err(
                rust_i18n::t!(
                    "cli.extract.errors.extract_failed",
                    locale = locale.as_str()
                )
                .to_string(),
            )?;
        }
        Some(("list", sub_matches)) => {
            commands::list::call(sub_matches, &locale).wrap_err(
                rust_i18n::t!("cli.list.errors.list_failed", locale = locale.as_str()).to_string(),
            )?;
        }
        Some(("encrypt", sub_matches)) => {
            commands::encrypt::call(sub_matches, &locale).wrap_err(
                rust_i18n::t!(
                    "cli.encrypt.errors.encrypt_failed",
                    locale = locale.as_str()
                )
                .to_string(),
            )?;
        }
        Some(("completions", sub_matches)) => {
            commands::completions::call(sub_matches, &locale).wrap_err(
                rust_i18n::t!(
                    "cli.completions.errors.completions_failed",
                    locale = locale.as_str()
                )
                .to_string(),
            )?;
        }
        Some(("reindex", sub_matches)) => {
            commands::reindex::call(sub_matches, &locale).wrap_err(
                rust_i18n::t!(
                    "cli.reindex.errors.reindex_failed",
                    locale = locale.as_str(),
                    file = sub_matches.get_one::<String>("file").map(String::as_str).unwrap_or("")
                )
                .to_string(),
            )?;
        }
        _ => unreachable!(),
    }

    Ok(())
}
