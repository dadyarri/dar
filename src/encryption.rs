use crate::i18n::Locale;
use clap::ArgMatches;
use eyre::{Context, eyre};
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

#[cfg(test)]
mod tests {
    use super::resolve_encryption_passphrase;
    use crate::cli::build_cli_with_translator;
    use crate::i18n::Locale;

    fn parse(args: &[&str]) -> clap::ArgMatches {
        build_cli_with_translator(|key| rust_i18n::t!(key, locale = "en").to_string())
            .try_get_matches_from(args)
            .unwrap()
            .subcommand_matches("create")
            .unwrap()
            .clone()
    }

    #[test]
    fn test_no_encryption_flags_returns_none() {
        let matches = parse(&["dari", "create", "-f", "out.dar", "."]);
        let locale = Locale::new("en");
        let result = resolve_encryption_passphrase(&matches, &locale).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_encrypt_passphrase_flag_returns_passphrase() {
        let matches = parse(&[
            "dari",
            "create",
            "-f",
            "out.dar",
            "--encrypt-passphrase",
            "secret",
            ".",
        ]);
        let locale = Locale::new("en");
        let result = resolve_encryption_passphrase(&matches, &locale).unwrap();
        assert_eq!(result, Some("secret".to_string()));
    }
}
