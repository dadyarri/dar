use crate::archive_builder::ArchiveBuilder;
use crate::format_version::FormatVersion;
use crate::i18n::Locale;
use crate::index_writer::{IndexWriter, index_path_for_archive};
use crate::pipeline::PipelineConfig;
use crate::reader::{load_archive, read_version};
use clap::ArgMatches;
use eyre::{Context, Result, eyre};
use rust_i18n::t;
use std::fs::{self, File};
use std::io::{BufWriter, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub fn call(matches: &ArgMatches, locale: &Locale) -> Result<()> {
    let file = matches.get_one::<String>("file").ok_or_else(|| {
        eyre!(t!(
            "cli.common.errors.file_required",
            locale = locale.as_str()
        ))
    })?;
    let output = matches.get_one::<String>("output").map(PathBuf::from);
    let split_size = matches
        .get_one::<String>("split-size")
        .map(|s| parse_split_size(s, locale))
        .transpose()?;

    let source_path = Path::new(file);
    if !source_path.exists() {
        return Err(eyre!(t!(
            "cli.migrate.errors.archive_missing",
            locale = locale.as_str(),
            file = file
        )));
    }

    let in_place = output.is_none();
    if in_place && split_size.is_some() {
        return Err(eyre!(t!(
            "cli.migrate.errors.split_in_place_unsupported",
            locale = locale.as_str()
        )));
    }
    let target_path = output.unwrap_or_else(|| source_path.with_extension("dar.migrate.tmp"));
    if target_path.exists() {
        return Err(eyre!(t!(
            "cli.migrate.errors.output_exists",
            locale = locale.as_str(),
            file = target_path.display().to_string()
        )));
    }

    let mut source_reader = File::open(source_path).wrap_err_with(|| {
        t!(
            "cli.migrate.errors.open_failed",
            locale = locale.as_str(),
            file = file
        )
        .to_string()
    })?;

    let source_version = read_version(&mut source_reader, locale)?;
    if source_version == FormatVersion::V6 {
        return Err(eyre!(t!(
            "cli.migrate.errors.already_v6",
            locale = locale.as_str(),
            file = file
        )));
    }

    let archive_state = load_archive(&mut source_reader, file, locale).wrap_err_with(|| {
        t!(
            "cli.migrate.errors.migrate_failed",
            locale = locale.as_str(),
            file = file
        )
        .to_string()
    })?;
    let entry_count = archive_state.entries.len();

    println!(
        "{}",
        t!(
            "cli.migrate.messages.migrating",
            locale = locale.as_str(),
            file = file,
            version = 6
        )
    );

    let target_output_path = if split_size.is_some() {
        PathBuf::from(format!("{}.001", target_path.display()))
    } else {
        target_path.clone()
    };

    let target_file = File::create(&target_output_path).wrap_err_with(|| {
        t!(
            "cli.migrate.errors.create_failed",
            locale = locale.as_str(),
            file = target_output_path.display().to_string()
        )
        .to_string()
    })?;
    let writer = BufWriter::new(target_file);
    let mut builder =
        ArchiveBuilder::with_version(writer, PipelineConfig::default(), FormatVersion::V6);
    builder.set_archive_output_path(target_output_path.clone());
    if let Some(split_threshold) = split_size {
        let base = target_path.clone();
        builder.enable_split(base, split_threshold, |path| {
            Ok(BufWriter::new(File::create(path)?))
        });
    }
    builder.write_header()?;

    let idx_path = index_path_for_archive(&target_path);
    let iw = IndexWriter::new(&idx_path, builder.header_timestamp(), 1).wrap_err_with(|| {
        t!(
            "cli.migrate.errors.create_failed",
            locale = locale.as_str(),
            file = idx_path.display().to_string()
        )
        .to_string()
    })?;
    builder.set_index_writer(iw);

    for entry in archive_state.entries {
        source_reader
            .seek(SeekFrom::Start(entry.entry.offset))
            .wrap_err_with(|| {
                t!(
                    "cli.common.errors.seek_failed",
                    locale = locale.as_str(),
                    file = file
                )
                .to_string()
            })?;
        let mut stored_bytes = vec![0u8; entry.entry.compressed_size as usize];
        source_reader
            .read_exact(&mut stored_bytes)
            .wrap_err_with(|| {
                t!(
                    "cli.migrate.errors.read_entry_failed",
                    locale = locale.as_str(),
                    file = file,
                    path = entry.path.as_str()
                )
                .to_string()
            })?;
        builder.import_stored_entry(entry, &stored_bytes)?;
    }

    builder.build()?;

    if in_place {
        replace_in_place(source_path, &target_path, locale)?;
    }

    let final_out = if in_place {
        source_path.to_path_buf()
    } else {
        target_path
    };

    println!(
        "{}",
        t!(
            "cli.migrate.messages.migrate_done",
            locale = locale.as_str(),
            entries = entry_count,
            out = final_out.display().to_string()
        )
    );

    Ok(())
}

fn parse_split_size(value: &str, locale: &Locale) -> Result<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(eyre!(t!(
            "cli.common.errors.split_size_empty",
            locale = locale.as_str()
        )));
    }

    let split_at = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let (digits, suffix) = trimmed.split_at(split_at);
    let number = digits.parse::<u64>().map_err(|_| {
        eyre!(t!(
            "cli.common.errors.split_size_invalid",
            locale = locale.as_str()
        ))
    })?;
    let multiplier = match suffix.trim().to_ascii_lowercase().as_str() {
        "" => 1,
        "k" | "kb" => 1024,
        "m" | "mb" => 1024 * 1024,
        "g" | "gb" => 1024 * 1024 * 1024,
        other => {
            return Err(eyre!(t!(
                "cli.common.errors.split_size_suffix_invalid",
                locale = locale.as_str(),
                suffix = other
            )));
        }
    };

    number.checked_mul(multiplier).ok_or_else(|| {
        eyre!(t!(
            "cli.common.errors.split_size_invalid",
            locale = locale.as_str()
        ))
    })
}

fn replace_in_place(source_path: &Path, temp_target: &Path, locale: &Locale) -> Result<()> {
    let backup_path = source_path.with_extension("dar.v5.bak");
    fs::rename(source_path, &backup_path).wrap_err_with(|| {
        t!(
            "cli.migrate.errors.rename_failed",
            locale = locale.as_str(),
            from = source_path.display().to_string(),
            to = backup_path.display().to_string()
        )
        .to_string()
    })?;

    if let Err(err) = fs::rename(temp_target, source_path).wrap_err_with(|| {
        t!(
            "cli.migrate.errors.rename_failed",
            locale = locale.as_str(),
            from = temp_target.display().to_string(),
            to = source_path.display().to_string()
        )
        .to_string()
    }) {
        let _ = fs::rename(&backup_path, source_path);
        return Err(err);
    }

    rename_optional_artifact(
        &index_path_for_archive(temp_target),
        &index_path_for_archive(source_path),
        locale,
    )?;
    rename_optional_artifact(
        &PathBuf::from(format!("{}.b3", temp_target.display())),
        &PathBuf::from(format!("{}.b3", source_path.display())),
        locale,
    )?;

    fs::remove_file(&backup_path).wrap_err_with(|| {
        t!(
            "cli.migrate.errors.cleanup_failed",
            locale = locale.as_str(),
            file = backup_path.display().to_string()
        )
        .to_string()
    })?;

    Ok(())
}

fn rename_optional_artifact(from: &Path, to: &Path, locale: &Locale) -> Result<()> {
    if !from.exists() {
        return Ok(());
    }
    if to.exists() {
        fs::remove_file(to).wrap_err_with(|| {
            t!(
                "cli.migrate.errors.cleanup_failed",
                locale = locale.as_str(),
                file = to.display().to_string()
            )
            .to_string()
        })?;
    }
    fs::rename(from, to).wrap_err_with(|| {
        t!(
            "cli.migrate.errors.rename_failed",
            locale = locale.as_str(),
            from = from.display().to_string(),
            to = to.display().to_string()
        )
        .to_string()
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::load_archive;
    use crate::test_utils::build_archive;
    use clap::{Arg, ArgAction, Command};
    use tempfile::tempdir;

    fn make_matches(args: &[&str]) -> clap::ArgMatches {
        let cmd = Command::new("dari").subcommand(
            Command::new("migrate")
                .arg(
                    Arg::new("file")
                        .short('f')
                        .long("file")
                        .action(ArgAction::Set)
                        .required(true),
                )
                .arg(
                    Arg::new("output")
                        .short('o')
                        .long("output")
                        .action(ArgAction::Set),
                )
                .arg(
                    Arg::new("split-size")
                        .long("split-size")
                        .action(ArgAction::Set),
                )
                .arg(
                    Arg::new("preserve-xattrs")
                        .long("preserve-xattrs")
                        .action(ArgAction::SetTrue),
                ),
        );
        let mut full_args = vec!["dari", "migrate"];
        full_args.extend_from_slice(args);
        let matches = cmd.get_matches_from(full_args);
        matches.subcommand_matches("migrate").unwrap().clone()
    }

    #[test]
    fn test_migrate_v5_archive_to_v6_output() {
        let dir = tempdir().unwrap();
        let source = build_archive(
            &dir,
            "source.dar",
            &[("a.txt", b"alpha"), ("b.txt", b"beta")],
            None,
        );
        let out = dir.path().join("migrated.dar");
        let matches = make_matches(&["-f", source.to_str().unwrap(), "-o", out.to_str().unwrap()]);
        let locale = Locale::new("en");

        call(&matches, &locale).unwrap();

        let mut fh = File::open(&out).unwrap();
        let state = load_archive(&mut fh, out.to_str().unwrap(), &locale).unwrap();
        assert_eq!(state.header.version, 6);
        assert_eq!(state.entries.len(), 2);
        assert!(dir.path().join("migrated.dari").exists());
        assert!(dir.path().join("migrated.dar.b3").exists());
        for entry in &state.entries {
            assert!(entry.stored_checksum_v6().is_some());
        }
    }

    #[test]
    fn test_migrate_rejects_existing_v6_archive() {
        use crate::pipeline::PipelineConfig;
        use std::io::BufWriter;

        let dir = tempdir().unwrap();
        let source = dir.path().join("source_v6.dar");
        {
            let fh = File::create(&source).unwrap();
            let mut builder = ArchiveBuilder::with_version(
                BufWriter::new(fh),
                PipelineConfig::default(),
                FormatVersion::V6,
            );
            builder.write_header().unwrap();
            builder.build().unwrap();
        }

        let out = dir.path().join("unused.dar");
        let matches = make_matches(&["-f", source.to_str().unwrap(), "-o", out.to_str().unwrap()]);
        let locale = Locale::new("en");
        assert!(call(&matches, &locale).is_err());
    }
}
