use super::shared::{compression_method_label, format_size};
use crate::constants::flags;
use crate::i18n::Locale;
use crate::reader::load_archive;
use clap::ArgMatches;
use eyre::{eyre, Context, Result};
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
            let path = &entry.path;
            let original_size = entry.entry.original_size;
            let compressed_size = entry.entry.compressed_size;
            let method = entry.entry.compression_method;
            let checksum = &entry.entry.checksum;

            let orig_human = format_size(original_size);
            let stored_human = format_size(compressed_size);
            let method_label = compression_method_label(method, locale.as_str());
            let checksum_prefix = &hex_string(checksum)[..8];

            println!(
                "{:<50}  {:>8}  {:>8}  {:<8}  {}",
                path, orig_human, stored_human, method_label, checksum_prefix
            );
        }
    }

    Ok(())
}

fn hex_string(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
