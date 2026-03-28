use crate::archive_builder::ArchiveBuilder;
use crate::i18n::Locale;
use crate::pipeline::PipelineConfig;
use crate::walker::scan_files;
use clap::ArgMatches;
use eyre::{eyre, Context, Result};
use rust_i18n::t;
use std::fs;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

pub fn call(matches: &ArgMatches, locale: &Locale) -> Result<()> {
    let file = matches
        .get_one::<String>("file")
        .ok_or_else(|| eyre!(t!("cli.errors.file_required", locale = locale.as_str())))?;

    let verbose = matches.get_flag("verbose");
    let overwrite = matches.get_flag("overwrite");
    let compress_images = matches.get_flag("compress-images");
    let encryption_passphrase = resolve_encryption_passphrase(matches, locale)?;
    let content = matches.get_many::<String>("content").unwrap();

    if Path::new(file).exists() && !overwrite {
        return Err(eyre!(t!(
            "cli.errors.file_exists",
            locale = locale.as_str(),
            file = file
        )));
    }

    if Path::new(file).exists() && overwrite {
        fs::remove_file(file)
            .wrap_err(t!("cli.errors.delete_failed", locale = locale.as_str()).to_string())?;
    }

    println!(
        "{}",
        t!(
            "cli.messages.creating_archive",
            locale = locale.as_str(),
            file = file
        )
    );

    let file_handle = File::create(file)
        .wrap_err(t!("cli.errors.create_file_failed", locale = locale.as_str()).to_string())?;
    let writer = BufWriter::new(file_handle);

    let config = PipelineConfig {
        compress_images,
        encryption_passphrase,
    };

    let mut builder = ArchiveBuilder::with_config(writer, config);
    builder.write_header()?;

    for file_entry in scan_files(content, locale)? {
        if verbose {
            println!("{}", file_entry.source_path.display());
        }

        builder.add_file(&file_entry.source_path, &file_entry.archive_path)?
    }

    builder.build()?;

    Ok(())
}

fn resolve_encryption_passphrase(matches: &ArgMatches, locale: &Locale) -> Result<Option<String>> {
    if let Some(passphrase) = matches.get_one::<String>("encrypt-passphrase") {
        return Ok(Some(passphrase.clone()));
    }

    if matches.get_flag("encrypt") {
        let prompt = t!("cli.prompts.enter_passphrase", locale = locale.as_str());
        let passphrase = rpassword::prompt_password(prompt)
            .wrap_err(t!("cli.errors.encrypt_prompt_failed", locale = locale.as_str()).to_string())?;

        if passphrase.is_empty() {
            return Err(eyre!(t!("cli.errors.encrypt_prompt_empty", locale = locale.as_str())));
        }

        return Ok(Some(passphrase));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use crate::archive_builder::ArchiveBuilder;
    use crate::pipeline::PipelineConfig;
    use crate::utils::{get_unix_timestamp, read_bytes_as, read_string};
    use std::io::Cursor;

    #[test]
    fn test_archive_header_writing() {
        // Arrange
        let mut buffer = Cursor::new(Vec::new());

        // Act
        let mut builder = ArchiveBuilder::with_config(&mut buffer, PipelineConfig::default());
        builder.write_header().unwrap();

        // Assert
        let data = buffer.into_inner();
        assert!(!data.is_empty(), "Archive data should not be empty");
        assert!(
            read_string(&data, 0, 4).is_ok(),
            "Failed to read archive signature"
        );
        assert_eq!(
            read_string(&data, 0, 4).unwrap(),
            "DARI".to_string(),
            "Invalid archive signature"
        );
        assert!(
            read_bytes_as::<u8>(&data, 4).is_ok(),
            "Failed to read archive version"
        );
        assert_eq!(
            read_bytes_as::<u8>(&data, 4).unwrap(),
            5,
            "Invalid archive version"
        );
        assert!(
            read_bytes_as::<u64>(&data, 5).is_ok(),
            "Failed to read archive creation timestamp"
        );
        assert!(
            read_bytes_as::<u64>(&data, 5).unwrap() <= get_unix_timestamp().unwrap(),
            "Invalid archive creation timestamp"
        );
    }
}
