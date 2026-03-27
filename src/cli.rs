use clap::{crate_authors, crate_description, crate_version};
use clap::{Arg, ArgAction, Command};

pub fn build_cli() -> Command {
    Command::new("dari")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .disable_help_flag(true)
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .subcommands(vec![
            Command::new("create")
                .short_flag('c')
                .about("Creates new archive")
                .args(vec![
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .required(true)
                        .help("The path to the resulting archive file"),
                    Arg::new("overwrite")
                        .short('o')
                        .long("overwrite")
                        .action(ArgAction::SetTrue)
                        .help("Overwrite existing archive file"),
                    Arg::new("compress-images")
                        .long("compress-images")
                        .action(ArgAction::SetTrue)
                        .help("Losslessly optimize PNG/JPEG using image-specific codecs"),
                    Arg::new("encrypt")
                        .long("encrypt")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .value_name("PASSPHRASE")
                        .conflicts_with("encrypt-passphrase")
                        .help("Encrypt file data with passphrase (alias of --encrypt-passphrase)"),
                    Arg::new("encrypt-passphrase")
                        .long("encrypt-passphrase")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .value_name("PASSPHRASE")
                        .conflicts_with("encrypt")
                        .help("Encrypt file data with passphrase (alias of --encrypt)"),
                    Arg::new("verbose")
                        .short('v')
                        .long("verbose")
                        .action(ArgAction::SetTrue)
                        .help("Enables verbose output"),
                    Arg::new("content")
                        .num_args(0..)
                        .required(false)
                        .action(ArgAction::Append)
                        .help("Files/folders to add to archive"),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help("Shows help of the command"),
                ]),
        ])
}

#[cfg(test)]
mod tests {
    use super::build_cli;
    use clap::error::ErrorKind;

    #[test]
    fn test_encrypt_flags_conflict() {
        let result = build_cli().try_get_matches_from(vec![
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
        let matches = build_cli()
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
            create.get_one::<String>("encrypt-passphrase").map(String::as_str),
            Some("pw")
        );
    }
}

