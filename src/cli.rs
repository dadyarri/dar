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

// Create: -cf <FILE> v (verbose) p (progress) h (help) --exclude <PATTERN> --exclude-from <FILE> <CONTENT>
// Extract: -xf <FILE> v (verbose) p (progress) h (help) -C <output dir>
// List: -tf <FILE> h (help)
// Append (new files to existing archive): -rf <FILE> <DIRECTORY/FILE> -v (verbose)
// Defragment (remove old indexes): -df <FILE>
// Validate (check if existing archive is valid): -Vf <FILE> -v (verbose) -s (slow, validating CRC of all files)
//
//
// ARCHIVE FORMAT (v0003)
// ======================
// Designed for efficient random access parsing, validation, and listing.
//
// STRUCTURE:
// [HEADER: 512 bytes fixed size]
//   Magic (4 bytes):                "DAR\0"
//   Version (4 bytes):              0003 (padded)
//   Data Section Start (8 bytes):   u64 big-endian offset
//   Index Section Start (8 bytes):  u64 big-endian offset
//   Total Files (4 bytes):          u32 big-endian count
//   Created Timestamp (8 bytes):    u64 big-endian UNIX time
//   Archive Checksum (32 bytes):    BLAKE3 of entire archive (computed last)
//   Flags (1 byte):                 reserved bits
//   [Padding: remaining to 512 bytes]
//
// [DATA SECTION]
//   For each file entry:
//     Entry Length (8 bytes):       u64 big-endian (excludes this 8-byte field)
//     Compressed Data:              [entry_length bytes]
//
// [INDEX SECTION]
//   Entry Count (4 bytes):          u32 big-endian count (matches header total_files)
//   For each index entry:
//     Entry Length (4 bytes):       u32 big-endian (excludes this 4-byte field)
//     Path Length (4 bytes):        u32 big-endian
//     Path:                         [path_length UTF-8 bytes]
//     Data Offset (8 bytes):        u64 big-endian offset into data section
//     Uncompressed Size (8 bytes):  u64 big-endian
//     Compressed Size (8 bytes):    u64 big-endian
//     Compression Algo (1 byte):    0=None, 1=Brotli, 2=Zstandard
//     Modification Time (8 bytes):  u64 big-endian UNIX timestamp
//     UID (1 byte):                 user ID (Unix only)
//     GID (1 byte):                 group ID (Unix only)
//     Permissions (2 bytes):        u16 big-endian mode bits
//     Checksum (32 bytes):          BLAKE3 of uncompressed data
//
// [END RECORD: 64 bytes fixed size]
//   Magic (4 bytes):                "DEND"
//   Index Offset (8 bytes):         u64 big-endian
//   Index Length (8 bytes):         u64 big-endian
//   Archive Checksum (32 bytes):    BLAKE3 of entire archive
//   Flags (1 byte):                 reserved
//   [Padding: 11 bytes]
//
// ADVANTAGES:
// - Fixed header/end records enable quick seeks
// - Length-prefixed entries allow safe skipping
// - Index completely separate from data for parallel access
// - End record checksum enables integrity verification
// - Structured format makes validation straightforward
