use crate::i18n::Locale;
use crate::reader;
use crate::tui::{App, state::AppState};
use clap::ArgMatches;
use eyre::{Context, Result};
use std::fs::File;
use std::path::PathBuf;

pub fn call(matches: &ArgMatches, locale: &Locale) -> Result<()> {
    let file_path = matches
        .get_one::<String>("file")
        .expect("--file is required");
    let passphrase = matches.get_one::<String>("encrypt-passphrase").cloned();

    let mut file = File::open(file_path).wrap_err_with(|| {
        rust_i18n::t!(
            "cli.inspect.errors.open_failed",
            locale = locale.as_str(),
            file = file_path
        )
        .to_string()
    })?;

    let archive_state = reader::load_archive(&mut file, file_path, locale)?;

    let app_state = AppState {
        archive_path: PathBuf::from(file_path),
        entries: archive_state.entries,
        passphrase,
        locale: locale.clone(),
    };

    App::run(app_state)
}


