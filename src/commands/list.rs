use clap::ArgMatches;

pub fn call(matches: &ArgMatches) {
    let file = matches.get_one::<String>("file").expect("File required");
    println!("Listing archive {}...", file)
}
