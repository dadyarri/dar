use crate::cli::build_cli_with_translator;
use crate::i18n::Locale;
use clap::ArgMatches;
use clap_complete::{Shell, generate};
use eyre::Result;

pub fn call(matches: &ArgMatches, locale: &Locale) -> Result<()> {
    let shell = matches.get_one::<Shell>("shell").copied().unwrap();
    let mut app = build_cli_with_translator(|key| {
        rust_i18n::t!(key, locale = locale.as_str()).to_string()
    });
    generate(shell, &mut app, "dari", &mut std::io::stdout());
    Ok(())
}
