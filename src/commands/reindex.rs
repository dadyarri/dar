//! `dari reindex` — regenerate the external `.dari` index file for an existing archive.
//!
//! Reads the embedded index from a `.dar` file and writes a fresh `.dari` alongside it.
//! This is useful after the archive has been created or appended without a `.dari`, or
//! when the `.dari` has become stale (timestamps differ).
//!
//! Only v6 archives produce `.dari` files; running `reindex` on a v5 archive is an error.

use crate::i18n::Locale;
use crate::index_writer::{IndexWriter, index_path_for_archive};
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
            "cli.reindex.errors.archive_missing",
            locale = locale.as_str(),
            file = file
        )));
    }

    let mut file_handle = File::open(file).wrap_err_with(|| {
        t!(
            "cli.reindex.errors.open_failed",
            locale = locale.as_str(),
            file = file
        )
        .to_string()
    })?;

    let archive_state = load_archive(&mut file_handle, file, locale).wrap_err_with(|| {
        t!(
            "cli.reindex.errors.reindex_failed",
            locale = locale.as_str(),
            file = file
        )
        .to_string()
    })?;

    // Only v6 archives support external index files.
    if archive_state.header.version != 6 {
        return Err(eyre!(t!(
            "cli.reindex.errors.v5_unsupported",
            locale = locale.as_str(),
            file = file
        )));
    }

    let idx_path = index_path_for_archive(Path::new(file));

    let mut iw =
        IndexWriter::new(&idx_path, archive_state.header.timestamp, 1).wrap_err_with(|| {
            t!(
                "cli.reindex.errors.reindex_failed",
                locale = locale.as_str(),
                file = file
            )
            .to_string()
        })?;

    for wrapper in &archive_state.entries {
        iw.write_entry(wrapper).wrap_err_with(|| {
            t!(
                "cli.reindex.errors.reindex_failed",
                locale = locale.as_str(),
                file = file
            )
            .to_string()
        })?;
    }

    iw.finish().wrap_err_with(|| {
        t!(
            "cli.reindex.errors.reindex_failed",
            locale = locale.as_str(),
            file = file
        )
        .to_string()
    })?;

    println!(
        "{}",
        t!(
            "cli.reindex.messages.reindex_done",
            locale = locale.as_str(),
            path = idx_path.display().to_string()
        )
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::build_v5_archive;
    use clap::{Arg, ArgAction, Command};
    use tempfile::tempdir;

    fn make_matches(args: &[&str]) -> clap::ArgMatches {
        let cmd = Command::new("dari").subcommand(
            Command::new("reindex")
                .arg(
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .required(true),
                )
                .arg(
                    Arg::new("encrypt-passphrase")
                        .long("encrypt-passphrase")
                        .action(ArgAction::Set),
                ),
        );
        let mut full_args = vec!["dari", "reindex"];
        full_args.extend_from_slice(args);
        let matches = cmd.get_matches_from(full_args);
        matches.subcommand_matches("reindex").unwrap().clone()
    }

    #[test]
    fn test_reindex_v5_archive_returns_error() {
        let dir = tempdir().unwrap();
        let archive = build_v5_archive(&dir, "v5.dar", &[("file.txt", b"data")], None);
        let sub_matches = make_matches(&["-f", archive.to_str().unwrap()]);
        let locale = Locale::new("en");
        let result = call(&sub_matches, &locale);
        assert!(
            result.is_err(),
            "reindex on a v5 archive must return an error"
        );
    }

    #[test]
    fn test_reindex_missing_archive_returns_error() {
        let sub_matches = make_matches(&["-f", "/tmp/nonexistent_dari_reindex_test.dar"]);
        let locale = Locale::new("en");
        assert!(call(&sub_matches, &locale).is_err());
    }

    #[test]
    fn test_reindex_v6_archive_creates_dari_file() {
        use crate::archive_builder::ArchiveBuilder;
        use crate::format_version::FormatVersion;
        use crate::pipeline::PipelineConfig;
        use std::io::BufWriter;

        let dir = tempdir().unwrap();
        let archive_path = dir.path().join("v6test.dar");

        // Create a v6 archive.
        {
            let fh = File::create(&archive_path).unwrap();
            let mut builder = ArchiveBuilder::with_version(
                BufWriter::new(fh),
                PipelineConfig::default(),
                FormatVersion::V6,
            );
            builder.write_header().unwrap();
            builder.build().unwrap();
        }

        let sub_matches = make_matches(&["-f", archive_path.to_str().unwrap()]);
        let locale = Locale::new("en");
        call(&sub_matches, &locale).unwrap();

        let idx_path = dir.path().join("v6test.dari");
        assert!(idx_path.exists(), ".dari file should be created by reindex");
    }

    #[test]
    fn test_reindex_v6_archive_with_entries_produces_readable_dari() {
        use crate::archive_builder::ArchiveBuilder;
        use crate::format_version::FormatVersion;
        use crate::pipeline::PipelineConfig;
        use crate::reader::load_index;
        use std::io::BufWriter;

        let dir = tempdir().unwrap();
        let archive_path = dir.path().join("v6entries.dar");

        // Create a v6 archive with two entries.
        {
            let fh = File::create(&archive_path).unwrap();
            let mut builder = ArchiveBuilder::with_version(
                BufWriter::new(fh),
                PipelineConfig::default(),
                FormatVersion::V6,
            );
            builder.write_header().unwrap();

            let f1 = dir.path().join("alpha.txt");
            std::fs::write(&f1, b"alpha content").unwrap();
            builder.add_file(&f1, "alpha.txt").unwrap();

            let f2 = dir.path().join("beta.rs");
            std::fs::write(&f2, b"fn main() {}").unwrap();
            builder.add_file(&f2, "beta.rs").unwrap();

            builder.build().unwrap();
        }

        // Run the reindex command.
        let sub_matches = make_matches(&["-f", archive_path.to_str().unwrap()]);
        let locale = Locale::new("en");
        call(&sub_matches, &locale).unwrap();

        // Verify .dari exists and contains both entries.
        let idx_path = dir.path().join("v6entries.dari");
        assert!(idx_path.exists(), ".dari must exist after reindex");

        let mut idx_fh = File::open(&idx_path).unwrap();
        let state = load_index(&mut idx_fh, idx_path.to_str().unwrap(), &locale).unwrap();

        assert_eq!(
            state.entries.len(),
            2,
            "reindexed .dari must contain both entries"
        );
        let paths: Vec<&str> = state.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"alpha.txt"), "alpha.txt must be in .dari");
        assert!(paths.contains(&"beta.rs"), "beta.rs must be in .dari");
    }

    #[test]
    fn test_reindex_v6_dari_timestamp_matches_archive_header() {
        use crate::archive_builder::ArchiveBuilder;
        use crate::format_version::FormatVersion;
        use crate::index_writer::IndexFileHeader;
        use crate::pipeline::PipelineConfig;
        use bytemuck::from_bytes;
        use std::io::BufWriter;

        let dir = tempdir().unwrap();
        let archive_path = dir.path().join("v6ts.dar");

        {
            let fh = File::create(&archive_path).unwrap();
            let mut builder = ArchiveBuilder::with_version(
                BufWriter::new(fh),
                PipelineConfig::default(),
                FormatVersion::V6,
            );
            builder.write_header().unwrap();
            builder.build().unwrap();
        }

        let sub_matches = make_matches(&["-f", archive_path.to_str().unwrap()]);
        let locale = Locale::new("en");
        call(&sub_matches, &locale).unwrap();

        // Read archive header timestamp (bytes 5..13 of the v6 header).
        let archive_bytes = std::fs::read(&archive_path).unwrap();
        let archive_ts = u64::from_le_bytes(archive_bytes[5..13].try_into().unwrap());

        // Read .dari header.
        let idx_path = dir.path().join("v6ts.dari");
        let dari_bytes = std::fs::read(&idx_path).unwrap();
        let dari_hdr = *from_bytes::<IndexFileHeader>(&dari_bytes[..17]);
        let dari_ts = dari_hdr.archive_timestamp;

        assert_eq!(
            dari_ts, archive_ts,
            ".dari timestamp must match the archive header timestamp"
        );
    }
}
