use crate::i18n::Locale;
use clap::ArgMatches;
use eyre::{eyre, Context};
use rust_i18n::t;

pub fn resolve_encryption_passphrase(
    matches: &ArgMatches,
    locale: &Locale,
) -> eyre::Result<Option<String>> {
    if let Some(passphrase) = matches.get_one::<String>("encrypt-passphrase") {
        return Ok(Some(passphrase.clone()));
    }

    if matches.get_flag("encrypt") {
        let prompt = t!("cli.prompts.enter_passphrase", locale = locale.as_str());
        let passphrase = rpassword::prompt_password(prompt).wrap_err(
            t!(
                "cli.common.errors.encrypt_prompt_failed",
                locale = locale.as_str()
            )
            .to_string(),
        )?;

        if passphrase.is_empty() {
            return Err(eyre!(t!(
                "cli.common.errors.encrypt_prompt_empty",
                locale = locale.as_str()
            )));
        }

        return Ok(Some(passphrase));
    }

    Ok(None)
}
