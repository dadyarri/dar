use crate::extractor::{extract_entries, extract_entry};
use crate::i18n::Locale;
use crate::reader::load_archive;
use clap::ArgMatches;
use eyre::{Context, Result, eyre};
use rust_i18n::t;
use std::fs::File;
use std::path::Path;

pub fn call(matches: &ArgMatches, locale: &Locale) -> Result<()> {
    let file = matches.get_one::<String>("file").ok_or_else(|| {
        eyre!(t!(
            "cli.common.errors.file_required",
            locale = locale.as_str()
        ))
    })?;

    if !Path::new(file).exists() {
        return Err(eyre!(t!(
            "cli.extract.errors.archive_missing",
            locale = locale.as_str(),
            file = file
        )));
    }

    let output_dir = matches
        .get_one::<String>("output-dir")
        .map(|s| s.as_str())
        .unwrap_or(".");

    let passphrase = matches
        .get_one::<String>("encrypt-passphrase")
        .map(|s| s.as_str());

    let filter_paths: Option<Vec<&str>> = matches
        .get_many::<String>("paths")
        .map(|v| v.map(|s| s.as_str()).collect());

    let mut file_handle = File::open(file).wrap_err_with(|| {
        t!(
            "cli.extract.errors.open_failed",
            locale = locale.as_str(),
            file = file
        )
        .to_string()
    })?;

    let archive_state = load_archive(&mut file_handle, file, locale)?;
    let created_at = archive_state.header.timestamp;

    println!(
        "{}",
        t!(
            "cli.extract.messages.extracting_archive",
            locale = locale.as_str(),
            file = file,
            timestamp = created_at
        )
    );

    let dest_dir = Path::new(output_dir);

    let entries_to_extract: Vec<_> = match &filter_paths {
        Some(paths) => archive_state
            .entries
            .iter()
            .filter(|e| paths.contains(&e.path.as_str()))
            .collect(),
        None => archive_state.entries.iter().collect(),
    };

    match entries_to_extract.as_slice() {
        [] => {}
        [single] => {
            extract_entry(
                Path::new(file),
                single,
                &archive_state.entries,
                dest_dir,
                passphrase,
            )?;
        }
        multiple => {
            extract_entries(
                Path::new(file),
                multiple,
                &archive_state.entries,
                dest_dir,
                passphrase,
            )?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::build_archive;
    use clap::{Arg, ArgAction, Command};
    use tempfile::tempdir;

    fn make_matches(args: &[&str]) -> ArgMatches {
        let cmd = Command::new("dari").subcommand(
            Command::new("extract")
                .arg(
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .required(true),
                )
                .arg(
                    Arg::new("output-dir")
                        .short('d')
                        .long("output-dir")
                        .action(ArgAction::Set),
                )
                .arg(
                    Arg::new("encrypt-passphrase")
                        .long("encrypt-passphrase")
                        .action(ArgAction::Set),
                )
                .arg(Arg::new("paths").num_args(0..).action(ArgAction::Append)),
        );
        let mut full_args = vec!["dari", "extract"];
        full_args.extend_from_slice(args);
        let matches = cmd.get_matches_from(full_args);
        matches.subcommand_matches("extract").unwrap().clone()
    }

    #[test]
    fn test_extract_all_files() {
        let dir = tempdir().unwrap();
        let archive = build_archive(
            &dir,
            "test.dar",
            &[("a.txt", b"hello"), ("b.txt", b"world")],
            None,
        );
        let out_dir = dir.path().join("out");
        std::fs::create_dir_all(&out_dir).unwrap();

        let sub_matches = make_matches(&[
            "-f",
            archive.to_str().unwrap(),
            "-d",
            out_dir.to_str().unwrap(),
        ]);

        let locale = Locale::new("en");
        call(&sub_matches, &locale).unwrap();

        assert_eq!(std::fs::read(out_dir.join("a.txt")).unwrap(), b"hello");
        assert_eq!(std::fs::read(out_dir.join("b.txt")).unwrap(), b"world");
    }

    #[test]
    fn test_extract_specific_paths() {
        let dir = tempdir().unwrap();
        let archive = build_archive(
            &dir,
            "test2.dar",
            &[("a.txt", b"aaa"), ("b.txt", b"bbb"), ("c.txt", b"ccc")],
            None,
        );
        let out_dir = dir.path().join("out2");
        std::fs::create_dir_all(&out_dir).unwrap();

        let sub_matches = make_matches(&[
            "-f",
            archive.to_str().unwrap(),
            "-d",
            out_dir.to_str().unwrap(),
            "a.txt",
            "c.txt",
        ]);

        let locale = Locale::new("en");
        call(&sub_matches, &locale).unwrap();

        assert_eq!(std::fs::read(out_dir.join("a.txt")).unwrap(), b"aaa");
        assert_eq!(std::fs::read(out_dir.join("c.txt")).unwrap(), b"ccc");
        // b.txt was not requested
        assert!(!out_dir.join("b.txt").exists());
    }

    #[test]
    fn test_extract_missing_archive_returns_error() {
        let sub_matches = make_matches(&["-f", "/tmp/nonexistent_dari_test.dar"]);
        let locale = Locale::new("en");
        assert!(call(&sub_matches, &locale).is_err());
    }

    #[test]
    fn test_extract_encrypted_with_passphrase() {
        let dir = tempdir().unwrap();
        let archive = build_archive(
            &dir,
            "enc.dar",
            &[("secret.txt", b"secret data")],
            Some("pass"),
        );
        let out_dir = dir.path().join("out_enc");
        std::fs::create_dir_all(&out_dir).unwrap();

        let sub_matches = make_matches(&[
            "-f",
            archive.to_str().unwrap(),
            "-d",
            out_dir.to_str().unwrap(),
            "--encrypt-passphrase",
            "pass",
        ]);

        let locale = Locale::new("en");
        call(&sub_matches, &locale).unwrap();

        assert_eq!(
            std::fs::read(out_dir.join("secret.txt")).unwrap(),
            b"secret data"
        );
    }
}
