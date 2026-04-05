use super::shared::{compression_method_label, format_size};
use crate::constants::flags;
use crate::i18n::Locale;
use crate::models::archive::CompressionMethod;
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
            "cli.list.errors.archive_missing",
            locale = locale.as_str(),
            file = file
        )));
    }

    let json = matches.get_flag("json");

    let mut file_handle = File::open(file).wrap_err_with(|| {
        t!(
            "cli.list.errors.open_failed",
            locale = locale.as_str(),
            file = file
        )
        .to_string()
    })?;

    let archive_state = load_archive(&mut file_handle, file, locale)?;

    if json {
        let mut items = Vec::with_capacity(archive_state.entries.len());
        for entry in &archive_state.entries {
            let path = entry.path.clone();
            let original_size = entry.entry.original_size;
            let compressed_size = entry.entry.compressed_size;
            let compression_method = u8::from(entry.entry.compression_method);
            let checksum_hex = hex_string(&entry.entry.checksum);
            let encrypted = entry.entry.bitflags & flags::ENCRYPTED_DATA != 0;
            let linked = entry.entry.bitflags & flags::LINKED_DATA != 0;

            items.push(serde_json::json!({
                "path": path,
                "original_size": original_size,
                "compressed_size": compressed_size,
                "compression_method": compression_method,
                "checksum": checksum_hex,
                "encrypted": encrypted,
                "linked": linked,
            }));
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&items).wrap_err(
                t!("cli.list.errors.list_failed", locale = locale.as_str()).to_string()
            )?
        );
    } else {
        println!(
            "{}",
            t!(
                "cli.list.messages.header",
                locale = locale.as_str(),
                file = file
            )
        );

        let col_path = t!("cli.list.columns.path", locale = locale.as_str());
        let col_orig = t!("cli.list.columns.original_size", locale = locale.as_str());
        let col_stored = t!("cli.list.columns.stored_size", locale = locale.as_str());
        let col_method = t!("cli.list.columns.method", locale = locale.as_str());
        let col_checksum = t!("cli.list.columns.checksum", locale = locale.as_str());

        println!(
            "{:<50}  {:>8}  {:>8}  {:<8}  {}",
            col_path, col_orig, col_stored, col_method, col_checksum
        );
        println!("{}", "-".repeat(90));

        for entry in &archive_state.entries {
            println!(
                "{}",
                format_entry_row(
                    &entry.path,
                    entry.entry.original_size,
                    entry.entry.compressed_size,
                    entry.entry.compression_method,
                    &entry.entry.checksum,
                    locale.as_str(),
                )
            );
        }
    }

    Ok(())
}

/// Format a single archive entry as a fixed-width table row.
///
/// Returns a `String` with columns separated by fixed-width padding:
/// path (50), original size (8), stored size (8), method (8), checksum prefix (8 hex chars).
///
/// Extracted as a pure function so it can be tested without a real archive on disk.
pub(crate) fn format_entry_row(
    path: &str,
    original_size: u64,
    compressed_size: u64,
    method: CompressionMethod,
    checksum: &[u8],
    locale: &str,
) -> String {
    let orig_human = format_size(original_size);
    let stored_human = format_size(compressed_size);
    let method_label = compression_method_label(method, locale);
    let checksum_prefix = &hex_string(checksum)[..8];
    format!(
        "{:<50}  {:>8}  {:>8}  {:<8}  {}",
        path, orig_human, stored_human, method_label, checksum_prefix
    )
}

fn hex_string(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_entry_row_stored_method() {
        let row = format_entry_row(
            "foo/bar.txt",
            1024,
            1024,
            CompressionMethod::None,
            &[0u8; 32],
            "en",
        );
        assert!(row.contains("foo/bar.txt"), "path must appear in row");
        assert!(
            row.contains("stored"),
            "method label must be 'stored' for None"
        );
        assert!(row.contains("1.00 KB"), "original size must be formatted");
        assert!(
            row.contains("00000000"),
            "first 8 checksum hex chars must appear"
        );
    }

    #[test]
    fn test_format_entry_row_brotli_method() {
        let row = format_entry_row(
            "doc.html",
            4096,
            2048,
            CompressionMethod::Brotli,
            &[0xabu8; 32],
            "en",
        );
        assert!(row.contains("doc.html"));
        assert!(row.contains("brotli"));
        assert!(row.contains("4.00 KB"));
        assert!(row.contains("abababab"));
    }

    #[test]
    fn test_format_entry_row_zstandard_method() {
        let row = format_entry_row(
            "src/main.rs",
            512,
            300,
            CompressionMethod::Zstandard,
            &[0xffu8; 32],
            "en",
        );
        assert!(row.contains("src/main.rs"));
        assert!(row.contains("zstd"));
        assert!(row.contains("ffffffff"));
    }

    #[test]
    fn test_format_entry_row_lzma_method() {
        let row = format_entry_row(
            "disk.iso",
            1_048_576,
            524_288,
            CompressionMethod::Lzma,
            &[0x12u8; 32],
            "en",
        );
        assert!(row.contains("disk.iso"));
        assert!(row.contains("lzma"));
        assert!(row.contains("1.00 MB"));
        assert!(row.contains("12121212"));
    }

    #[test]
    fn test_format_entry_row_russian_locale() {
        let row = format_entry_row(
            "file.txt",
            100,
            100,
            CompressionMethod::None,
            &[0u8; 32],
            "ru",
        );
        assert!(row.contains("file.txt"));
        assert!(
            row.contains("без сжатия"),
            "Russian locale must use translated method label"
        );
    }

    #[test]
    fn test_format_entry_row_path_fits_column_width() {
        // A 50-char path should be left-padded to exactly 50 chars in the output.
        let path = "a".repeat(50);
        let row = format_entry_row(&path, 0, 0, CompressionMethod::None, &[0u8; 32], "en");
        assert!(row.starts_with(&path));
    }
}
