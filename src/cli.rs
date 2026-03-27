use clap::{crate_authors, crate_version};
use clap::{Arg, ArgAction, Command};

pub fn build_cli_with_translator<T>(translate: T) -> Command
where
    T: Fn(&str) -> String,
{
    Command::new("dari")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .disable_help_flag(true)
        .version(crate_version!())
        .author(crate_authors!())
        .about(translate("cli.about"))
        .subcommands(vec![
            Command::new("create")
                .short_flag('c')
                .about(translate("cli.create.about"))
                .args(vec![
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .required(true)
                        .help(translate("cli.arg.file")),
                    Arg::new("overwrite")
                        .short('o')
                        .long("overwrite")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.arg.overwrite")),
                    Arg::new("compress-images")
                        .long("compress-images")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.arg.compress_images")),
                    Arg::new("encrypt")
                        .long("encrypt")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .value_name("PASSPHRASE")
                        .conflicts_with("encrypt-passphrase")
                        .help(translate("cli.arg.encrypt")),
                    Arg::new("encrypt-passphrase")
                        .long("encrypt-passphrase")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .value_name("PASSPHRASE")
                        .conflicts_with("encrypt")
                        .help(translate("cli.arg.encrypt_passphrase")),
                    Arg::new("verbose")
                        .short('v')
                        .long("verbose")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.arg.verbose")),
                    Arg::new("content")
                        .num_args(0..)
                        .required(false)
                        .action(ArgAction::Append)
                        .help(translate("cli.arg.content")),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help(translate("cli.arg.help")),
                ]),
        ])
}

#[cfg(test)]
mod tests {
    use super::build_cli_with_translator;
    use clap::error::ErrorKind;

    #[test]
    fn test_encrypt_flags_conflict() {
        let result = build_cli_with_translator(|key| rust_i18n::t!(key, locale = "en").to_string())
            .try_get_matches_from(vec![
            "dari",
            "create",
            "-f",
            "out.dar",
            "--encrypt",
            "pw1",
            "--encrypt-passphrase",
            "pw2",
            "src",
        ]);

        let err = result.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn test_encrypt_alias_is_accepted() {
        let matches = build_cli_with_translator(|key| rust_i18n::t!(key, locale = "en").to_string())
            .try_get_matches_from(vec![
                "dari",
                "create",
                "-f",
                "out.dar",
                "--encrypt-passphrase",
                "pw",
                "src",
            ])
            .unwrap();

        let create = matches.subcommand_matches("create").unwrap();
        assert_eq!(
            create
                .get_one::<String>("encrypt-passphrase")
                .map(String::as_str),
            Some("pw")
        );
    }

    #[test]
    fn test_help_is_localized_for_russian() {
        let help = build_cli_with_translator(|key| rust_i18n::t!(key, locale = "ru").to_string())
            .try_get_matches_from(vec!["dari", "create", "--help"])
            .unwrap_err()
            .to_string();

        let expected = rust_i18n::t!("cli.create.about", locale = "ru").to_string();
        assert!(help.contains(&expected));
    }
}
