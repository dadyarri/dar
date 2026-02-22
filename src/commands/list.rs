use clap::ArgMatches;
use eyre::Result;

pub fn call(matches: &ArgMatches) -> Result<()> {
    let file_path = matches.get_one::<String>("file").expect("File required");
    crate::tui::run(file_path)
}
