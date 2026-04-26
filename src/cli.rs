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
                    Arg::new("chunked-encryption")
                        .long("chunked-encryption")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.create.args.chunked_encryption")),
                    Arg::new("preserve-xattrs")
                        .long("preserve-xattrs")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.create.args.preserve_xattrs")),
                    Arg::new("verbose")
                        .short('v')
                        .long("verbose")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.common.args.verbose")),
                    Arg::new("dry-run")
                        .long("dry-run")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.create.args.dry_run")),
                    Arg::new("split-size")
                        .long("split-size")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .help(translate("cli.create.args.split_size")),
                    Arg::new("format-version")
                        .long("format-version")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .default_value("6")
                        .value_parser(["5", "6"])
                        .help(translate("cli.common.flags.format_version")),
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
                    Arg::new("chunked-encryption")
                        .long("chunked-encryption")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.append.args.chunked_encryption")),
                    Arg::new("preserve-xattrs")
                        .long("preserve-xattrs")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.append.args.preserve_xattrs")),
                    Arg::new("verbose")
                        .short('v')
                        .long("verbose")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.common.args.verbose")),
                    Arg::new("dry-run")
                        .long("dry-run")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.append.args.dry_run")),
                    Arg::new("format-version")
                        .long("format-version")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .default_value("6")
                        .value_parser(["5", "6"])
                        .help(translate("cli.common.flags.format_version")),
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
                    Arg::new("no-index")
                        .long("no-index")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.common.args.no_index")),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help(translate("cli.common.args.help")),
                ]),
            Command::new("incremental")
                .about(translate("cli.incremental.about"))
                .help_template(command_help_template(&translate))
                .args(vec![
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .required(true)
                        .help(translate("cli.incremental.args.file")),
                    Arg::new("since")
                        .long("since")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .help(translate("cli.incremental.args.since")),
                    Arg::new("compress-images")
                        .long("compress-images")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.incremental.args.compress_images")),
                    Arg::new("encrypt")
                        .long("encrypt")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("encrypt-passphrase")
                        .help(translate("cli.incremental.args.encrypt")),
                    Arg::new("encrypt-passphrase")
                        .long("encrypt-passphrase")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .value_name(passphrase_value)
                        .conflicts_with("encrypt")
                        .help(translate("cli.incremental.args.encrypt_passphrase")),
                    Arg::new("chunked-encryption")
                        .long("chunked-encryption")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.incremental.args.chunked_encryption")),
                    Arg::new("preserve-xattrs")
                        .long("preserve-xattrs")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.incremental.args.preserve_xattrs")),
                    Arg::new("verbose")
                        .short('v')
                        .long("verbose")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.common.args.verbose")),
                    Arg::new("dry-run")
                        .long("dry-run")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.incremental.args.dry_run")),
                    Arg::new("format-version")
                        .long("format-version")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .default_value("6")
                        .value_parser(["5", "6"])
                        .help(translate("cli.common.flags.format_version")),
                    Arg::new("on-conflict")
                        .long("on-conflict")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .default_value("error")
                        .value_parser(["error", "rename", "overwrite"])
                        .help(translate("cli.incremental.args.on_conflict")),
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
                    Arg::new("no-index")
                        .long("no-index")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.common.args.no_index")),
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
                    Arg::new("no-index")
                        .long("no-index")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.common.args.no_index")),
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
            Command::new("reindex")
                .about(translate("cli.reindex.about"))
                .help_template(command_help_template(&translate))
                .args(vec![
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .required(true)
                        .help(translate("cli.reindex.args.file")),
                    Arg::new("encrypt-passphrase")
                        .long("encrypt-passphrase")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .value_name(passphrase_value)
                        .help(translate("cli.reindex.args.encrypt_passphrase")),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help(translate("cli.common.args.help")),
                ]),
            Command::new("migrate")
                .about(translate("cli.migrate.about"))
                .help_template(command_help_template(&translate))
                .args(vec![
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .required(true)
                        .help(translate("cli.migrate.args.file")),
                    Arg::new("output")
                        .short('o')
                        .long("output")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .help(translate("cli.migrate.args.output")),
                    Arg::new("split-size")
                        .long("split-size")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .help(translate("cli.migrate.args.split_size")),
                    Arg::new("preserve-xattrs")
                        .long("preserve-xattrs")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.migrate.args.preserve_xattrs")),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help(translate("cli.common.args.help")),
                ]),
            Command::new("verify")
                .about(translate("cli.verify.about"))
                .help_template(command_help_template(&translate))
                .args(vec![
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .required(true)
                        .help(translate("cli.verify.args.file")),
                    Arg::new("encrypt-passphrase")
                        .long("encrypt-passphrase")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .value_name(passphrase_value)
                        .help(translate("cli.verify.args.encrypt_passphrase")),
                    Arg::new("full")
                        .long("full")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.verify.args.full")),
                    Arg::new("json")
                        .long("json")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.verify.args.json")),
                    Arg::new("no-index")
                        .long("no-index")
                        .action(ArgAction::SetTrue)
                        .help(translate("cli.common.args.no_index")),
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
    fn test_chunked_encryption_flag_is_accepted_for_create() {
        let matches =
            build_cli_with_translator(|key| rust_i18n::t!(key, locale = "en").to_string())
                .try_get_matches_from(vec![
                    "dari",
                    "create",
                    "-f",
                    "out.dar",
                    "--encrypt-passphrase",
                    "pw",
                    "--chunked-encryption",
                    "src",
                ])
                .unwrap();

        let create = matches.subcommand_matches("create").unwrap();
        assert!(create.get_flag("chunked-encryption"));
    }

    #[test]
    fn test_preserve_xattrs_flag_is_accepted_for_create() {
        let matches =
            build_cli_with_translator(|key| rust_i18n::t!(key, locale = "en").to_string())
                .try_get_matches_from(vec![
                    "dari",
                    "create",
                    "-f",
                    "out.dar",
                    "--preserve-xattrs",
                    "src",
                ])
                .unwrap();

        let create = matches.subcommand_matches("create").unwrap();
        assert!(create.get_flag("preserve-xattrs"));
    }

    #[test]
    fn test_chunked_encryption_flag_is_accepted_for_append() {
        let matches =
            build_cli_with_translator(|key| rust_i18n::t!(key, locale = "en").to_string())
                .try_get_matches_from(vec![
                    "dari",
                    "append",
                    "-f",
                    "out.dar",
                    "--encrypt-passphrase",
                    "pw",
                    "--chunked-encryption",
                    "src",
                ])
                .unwrap();

        let append = matches.subcommand_matches("append").unwrap();
        assert!(append.get_flag("chunked-encryption"));
    }

    #[test]
    fn test_preserve_xattrs_flag_is_accepted_for_append() {
        let matches =
            build_cli_with_translator(|key| rust_i18n::t!(key, locale = "en").to_string())
                .try_get_matches_from(vec![
                    "dari",
                    "append",
                    "-f",
                    "out.dar",
                    "--preserve-xattrs",
                    "src",
                ])
                .unwrap();

        let append = matches.subcommand_matches("append").unwrap();
        assert!(append.get_flag("preserve-xattrs"));
    }

    #[test]
    fn test_incremental_since_flag_is_accepted() {
        let matches =
            build_cli_with_translator(|key| rust_i18n::t!(key, locale = "en").to_string())
                .try_get_matches_from(vec![
                    "dari",
                    "incremental",
                    "-f",
                    "out.dar",
                    "--since",
                    "@42",
                    "src",
                ])
                .unwrap();

        let incremental = matches.subcommand_matches("incremental").unwrap();
        assert_eq!(
            incremental.get_one::<String>("since").map(String::as_str),
            Some("@42")
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

    #[test]
    fn test_migrate_flags_are_accepted() {
        let matches =
            build_cli_with_translator(|key| rust_i18n::t!(key, locale = "en").to_string())
                .try_get_matches_from(vec![
                    "dari",
                    "migrate",
                    "-f",
                    "old.dar",
                    "-o",
                    "new.dar",
                    "--split-size",
                    "2M",
                    "--preserve-xattrs",
                ])
                .unwrap();

        let migrate = matches.subcommand_matches("migrate").unwrap();
        assert_eq!(
            migrate.get_one::<String>("output").map(String::as_str),
            Some("new.dar")
        );
        assert_eq!(
            migrate.get_one::<String>("split-size").map(String::as_str),
            Some("2M")
        );
        assert!(migrate.get_flag("preserve-xattrs"));
    }
}
