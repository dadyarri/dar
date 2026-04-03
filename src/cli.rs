use clap::{Arg, ArgAction, Command};
use clap::{crate_authors, crate_version};
use clap_complete::Shell;

fn root_help_template<T>(translate: &T) -> String
where
    T: Fn(&str) -> String,
{
    color_print::cformat!(
        r#"{{before-help}}{{about-with-newline}}
<bold><underline>{usage_heading}</underline></bold>: {{usage}}

<bold><underline>{options_heading}</underline></bold>:
{{options}}

<bold><underline>{commands_heading}</underline></bold>:
{{subcommands}}{{after-help}}"#,
        usage_heading = translate("cli.common.headings.usage"),
        options_heading = translate("cli.common.headings.options"),
        commands_heading = translate("cli.common.headings.commands"),
    )
}

fn command_help_template<T>(translate: &T) -> String
where
    T: Fn(&str) -> String,
{
    color_print::cformat!(
        r#"{{before-help}}{{about-with-newline}}
<bold><underline>{usage_heading}</underline></bold>: {{usage}}

<bold><underline>{arguments_heading}</underline></bold>:
{{positionals}}

<bold><underline>{options_heading}</underline></bold>:
{{options}}{{after-help}}"#,
        usage_heading = translate("cli.common.headings.usage"),
        arguments_heading = translate("cli.common.headings.arguments"),
        options_heading = translate("cli.common.headings.options"),
    )
}

pub fn build_cli_with_translator<T>(translate: T) -> Command
where
    T: Fn(&str) -> String,
{
    let passphrase_value: &'static str =
        Box::leak(translate("cli.create.args.passphrase_value").into_boxed_str());

    Command::new("dari")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .disable_help_flag(true)
        .disable_help_subcommand(true)
        .disable_version_flag(true)
        .version(crate_version!())
        .author(crate_authors!())
        .about(translate("cli.about"))
        .help_template(root_help_template(&translate))
        .arg(
            Arg::new("help")
                .short('h')
                .long("help")
                .action(ArgAction::Help)
                .help(translate("cli.common.args.help")),
        )
        .arg(
            Arg::new("version")
                .short('V')
                .long("version")
                .action(ArgAction::Version)
                .help(translate("cli.common.args.version")),
        )
        .subcommands(vec![
            Command::new("create")
                .short_flag('c')
                .about(translate("cli.create.about"))
                .help_template(command_help_template(&translate))
                .args(vec![
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .required(true)
                        .help(translate("cli.create.args.file")),
                    Arg::new("overwrite")
                        .short('o')
                        .long("overwrite")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.create.args.overwrite")),
                    Arg::new("compress-images")
                        .long("compress-images")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.create.args.compress_images")),
                    Arg::new("encrypt")
                        .long("encrypt")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("encrypt-passphrase")
                        .help(translate("cli.create.args.encrypt")),
                    Arg::new("encrypt-passphrase")
                        .long("encrypt-passphrase")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .value_name(passphrase_value)
                        .conflicts_with("encrypt")
                        .help(translate("cli.create.args.encrypt_passphrase")),
                    Arg::new("verbose")
                        .short('v')
                        .long("verbose")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.common.args.verbose")),
                    Arg::new("dry-run")
                        .long("dry-run")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.create.args.dry_run")),
                    Arg::new("content")
                        .num_args(0..)
                        .required(false)
                        .action(ArgAction::Append)
                        .help(translate("cli.common.args.content")),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help(translate("cli.common.args.help")),
                ]),
            Command::new("append")
                .short_flag('a')
                .about(translate("cli.append.about"))
                .help_template(command_help_template(&translate))
                .args(vec![
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .required(true)
                        .help(translate("cli.append.args.file")),
                    Arg::new("compress-images")
                        .long("compress-images")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.append.args.compress_images")),
                    Arg::new("encrypt")
                        .long("encrypt")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("encrypt-passphrase")
                        .help(translate("cli.append.args.encrypt")),
                    Arg::new("encrypt-passphrase")
                        .long("encrypt-passphrase")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .value_name(passphrase_value)
                        .conflicts_with("encrypt")
                        .help(translate("cli.append.args.encrypt_passphrase")),
                    Arg::new("verbose")
                        .short('v')
                        .long("verbose")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.common.args.verbose")),
                    Arg::new("dry-run")
                        .long("dry-run")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.append.args.dry_run")),
                    Arg::new("on-conflict")
                        .long("on-conflict")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .default_value("error")
                        .value_parser(["error", "rename", "overwrite"])
                        .help(translate("cli.append.args.on_conflict")),
                    Arg::new("content")
                        .num_args(0..)
                        .required(false)
                        .action(ArgAction::Append)
                        .help(translate("cli.common.args.content")),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help(translate("cli.common.args.help")),
                ]),
            Command::new("inspect")
                .short_flag('i')
                .about(translate("cli.inspect.about"))
                .help_template(command_help_template(&translate))
                .args(vec![
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .required(true)
                        .help(translate("cli.inspect.args.file")),
                    Arg::new("encrypt-passphrase")
                        .long("encrypt-passphrase")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .value_name(passphrase_value)
                        .help(translate("cli.inspect.args.encrypt_passphrase")),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help(translate("cli.common.args.help")),
                ]),
            Command::new("extract")
                .short_flag('x')
                .about(translate("cli.extract.about"))
                .help_template(command_help_template(&translate))
                .args(vec![
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .required(true)
                        .help(translate("cli.extract.args.file")),
                    Arg::new("output-dir")
                        .short('d')
                        .long("output-dir")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .help(translate("cli.extract.args.output_dir")),
                    Arg::new("encrypt-passphrase")
                        .long("encrypt-passphrase")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .value_name(passphrase_value)
                        .help(translate("cli.extract.args.encrypt_passphrase")),
                    Arg::new("paths")
                        .num_args(0..)
                        .required(false)
                        .action(ArgAction::Append)
                        .help(translate("cli.extract.args.paths")),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help(translate("cli.common.args.help")),
                ]),
            Command::new("list")
                .short_flag('l')
                .alias("ls")
                .about(translate("cli.list.about"))
                .help_template(command_help_template(&translate))
                .args(vec![
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .required(true)
                        .help(translate("cli.list.args.file")),
                    Arg::new("json")
                        .long("json")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.list.args.json")),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help(translate("cli.common.args.help")),
                ]),
            Command::new("encrypt")
                .short_flag('e')
                .about(translate("cli.encrypt.about"))
                .help_template(command_help_template(&translate))
                .args(vec![
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .required(true)
                        .help(translate("cli.encrypt.args.file")),
                    Arg::new("encrypt")
                        .long("encrypt")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("encrypt-passphrase")
                        .help(translate("cli.encrypt.args.encrypt")),
                    Arg::new("encrypt-passphrase")
                        .long("encrypt-passphrase")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .value_name(passphrase_value)
                        .conflicts_with("encrypt")
                        .help(translate("cli.encrypt.args.encrypt_passphrase")),
                    Arg::new("output")
                        .short('o')
                        .long("output")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .conflicts_with("in-place")
                        .help(translate("cli.encrypt.args.output")),
                    Arg::new("in-place")
                        .short('i')
                        .long("in-place")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("output")
                        .help(translate("cli.encrypt.args.in_place")),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help(translate("cli.common.args.help")),
                ]),
            Command::new("completions")
                .about(translate("cli.completions.about"))
                .help_template(command_help_template(&translate))
                .args(vec![
                    Arg::new("shell")
                        .required(true)
                        .value_parser(clap::value_parser!(Shell))
                        .help(translate("cli.completions.args.shell")),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help(translate("cli.common.args.help")),
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
                "--encrypt-passphrase",
                "pw2",
                "src",
            ]);

        let err = result.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn test_encrypt_passphrase_arg_is_accepted() {
        let matches =
            build_cli_with_translator(|key| rust_i18n::t!(key, locale = "en").to_string())
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
    fn test_encrypt_flag_sets_boolean() {
        let matches =
            build_cli_with_translator(|key| rust_i18n::t!(key, locale = "en").to_string())
                .try_get_matches_from(vec!["dari", "create", "-f", "out.dar", "--encrypt", "src"])
                .unwrap();

        let create = matches.subcommand_matches("create").unwrap();
        assert!(create.get_flag("encrypt"));
        assert!(create.get_one::<String>("encrypt-passphrase").is_none());
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

    #[test]
    fn test_root_help_is_available_and_localized() {
        let help = build_cli_with_translator(|key| rust_i18n::t!(key, locale = "ru").to_string())
            .try_get_matches_from(vec!["dari", "--help"])
            .unwrap_err()
            .to_string();

        let usage = rust_i18n::t!("cli.common.headings.usage", locale = "ru").to_string();
        let commands = rust_i18n::t!("cli.common.headings.commands", locale = "ru").to_string();
        assert!(help.contains(&usage));
        assert!(help.contains(&commands));
    }

    #[test]
    fn test_completions_bash_is_accepted() {
        use clap_complete::Shell;

        let matches =
            build_cli_with_translator(|key| rust_i18n::t!(key, locale = "en").to_string())
                .try_get_matches_from(vec!["dari", "completions", "bash"])
                .unwrap();

        let completions = matches.subcommand_matches("completions").unwrap();
        assert_eq!(
            completions.get_one::<Shell>("shell").copied(),
            Some(Shell::Bash)
        );
    }

    #[test]
    fn test_completions_invalid_shell_produces_error() {
        let result = build_cli_with_translator(|key| rust_i18n::t!(key, locale = "en").to_string())
            .try_get_matches_from(vec!["dari", "completions", "invalidshell"]);

        let err = result.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidValue);
    }
}
