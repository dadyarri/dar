use clap::{Arg, ArgAction, Command, arg};

pub fn build_cli() -> Command {
    Command::new("dar")
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
                        .help("Name of the resulting archive"),
                    Arg::new("verbose")
                        .short('v')
                        .long("verbose")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("progress")
                        .help("Enables verbose output"),
                    Arg::new("progress")
                        .short('p')
                        .long("progress")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("verbose")
                        .help("Enables progress bar"),
                    Arg::new("content")
                        .num_args(1..)
                        .required(true)
                        .action(ArgAction::Append)
                        .help("Files/folders to add to archive"),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help("Shows help of the command"),
                ]),
            Command::new("extract")
                .short_flag('x')
                .about("Extracts existing archive")
                .args(vec![
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .required(true)
                        .help("Name of the archive to extract"),
                    Arg::new("out")
                        .short('C')
                        .long("out")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .default_value(".")
                        .help("Target directory to extract"),
                    Arg::new("verbose")
                        .short('v')
                        .long("verbose")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("progress")
                        .help("Enables verbose output"),
                    Arg::new("progress")
                        .short('p')
                        .long("progress")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("verbose")
                        .help("Enables progress bar"),
                    Arg::new("entries")
                        .num_args(0..)
                        .action(ArgAction::Append)
                        .help("Specific files or directories to extract (optional, extracts all if not specified)"),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help("Shows help of the command"),
                ]),
            Command::new("list")
                .short_flag('t')
                .about("Shows content of the archive")
                .args(vec![
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .required(true)
                        .help("Name of the archive to list"),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help("Shows help of the command"),
                ]),
            Command::new("append")
                .short_flag('r')
                .about("Appends data to existing archive")
                .args(vec![
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .required(true)
                        .help("Name of the archive to list"),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help("Shows help of the command"),
                    arg!(<ENTRY> "Entry (file or folder) to append"),
                ]),
            Command::new("defragment")
                .short_flag('d')
                .about("Removes obsolete metadata from the archive (possibly reducing its size)")
                .args(vec![
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .required(true)
                        .help("Name of the archive to defragment"),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help("Shows help of the command"),
                ]),
            Command::new("validate")
                .short_flag('v')
                .about("Validates archive integrity (just its metadata's or all contents')")
                .args(vec![
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .num_args(1)
                        .required(true)
                        .help("Name of the archive to validate"),
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help)
                        .help("Shows help of the command"),
                    Arg::new("slow")
                        .short('s')
                        .long("slow")
                        .action(ArgAction::SetTrue)
                        .help("Enables slow validation of the archive (each entry)"),
                    Arg::new("verbose")
                        .short('v')
                        .long("verbose")
                        .action(ArgAction::SetTrue)
                        .help("Enables verbose output"),
                ]),
        ])
}
