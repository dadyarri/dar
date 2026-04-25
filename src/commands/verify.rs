use crate::extractor::{read_raw_entry_bytes, resolve_volume_path, try_decrypt_bytes};
use crate::i18n::Locale;
use crate::reader::load_with_auto_index;
use crate::sidecar::verify_b3_sidecar;
use crate::traits::decompress_bytes;
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
    let full = matches
        .try_get_one::<bool>("full")
        .ok()
        .flatten()
        .copied()
        .unwrap_or(false);
    let json = matches
        .try_get_one::<bool>("json")
        .ok()
        .flatten()
        .copied()
        .unwrap_or(false);
    let no_index = matches
        .try_get_one::<bool>("no-index")
        .ok()
        .flatten()
        .copied()
        .unwrap_or(false);
    let passphrase = matches
        .try_get_one::<String>("encrypt-passphrase")
        .ok()
        .flatten()
        .map(String::as_str);

    let archive_path = Path::new(file);
    if !archive_path.exists() {
        return Err(eyre!(t!(
            "cli.verify.errors.archive_missing",
            locale = locale.as_str(),
            file = file
        )));
    }
    let mut file_handle = File::open(archive_path).wrap_err_with(|| {
        t!(
            "cli.verify.errors.open_failed",
            locale = locale.as_str(),
            file = file
        )
        .to_string()
    })?;
    let state = load_with_auto_index(&mut file_handle, archive_path, no_index, locale)?;

    let mut ok = true;
    let mut json_rows = Vec::new();

    for volume in 0..state.total_volumes.max(1) {
        let volume_path = resolve_volume_path(archive_path, volume);
        let volume_ok = verify_b3_sidecar(&volume_path).unwrap_or(false);
        ok &= volume_ok;
        if json {
            json_rows.push(serde_json::json!({
                "layer": 1,
                "volume": volume,
                "ok": volume_ok,
                "path": volume_path,
            }));
        } else if volume_ok {
            let bytes = std::fs::metadata(&volume_path).map(|m| m.len()).unwrap_or(0);
            println!(
                "{}",
                t!(
                    "cli.verify.messages.layer1_ok",
                    locale = locale.as_str(),
                    v = volume + 1,
                    bytes = bytes
                )
            );
        } else {
            println!(
                "{}",
                t!(
                    "cli.verify.messages.layer1_fail",
                    locale = locale.as_str(),
                    v = volume + 1
                )
            );
        }
    }

    for entry in &state.entries {
        let entry_offset = entry.entry.offset;
        let entry_volume = entry.volume_number;
        let raw = read_raw_entry_bytes(archive_path, entry, &state.entries)
            .ok_or_else(|| eyre!(t!("cli.verify.errors.verify_failed", locale = locale.as_str())))?;
        let stored_ok = entry
            .stored_checksum_v6()
            .is_none_or(|expected| blake3::hash(&raw).as_bytes() == expected);
        ok &= stored_ok;

        if json {
            json_rows.push(serde_json::json!({
                "layer": 2,
                "path": entry.path,
                "ok": stored_ok,
                "volume": entry_volume,
                "offset": entry_offset,
            }));
        } else if stored_ok {
            println!(
                "{}",
                t!(
                    "cli.verify.messages.layer2_ok",
                    locale = locale.as_str(),
                    path = entry.path.as_str()
                )
            );
        } else {
            println!(
                "{}",
                t!(
                    "cli.verify.messages.layer2_fail",
                    locale = locale.as_str(),
                    path = entry.path.as_str(),
                    v = entry_volume + 1,
                    off = entry_offset
                )
            );
        }

        if !full {
            continue;
        }

        let decrypted = if entry.entry.bitflags & crate::constants::flags::ENCRYPTED_DATA != 0 {
            let pass = passphrase.ok_or_else(|| {
                eyre!(t!(
                    "cli.extractor.errors.passphrase_required",
                    path = entry.path.as_str()
                ))
            })?;
            try_decrypt_bytes(&raw, &entry.entry.checksum, pass).ok_or_else(|| {
                eyre!(t!(
                    "cli.extractor.errors.decrypt_invalid",
                    locale = locale.as_str()
                ))
            })?
        } else {
            raw
        };
        let plain = decompress_bytes(entry.entry.compression_method, &decrypted)?;
        let content_ok = blake3::hash(&plain).as_bytes() == &entry.entry.checksum;
        ok &= content_ok;

        if json {
            json_rows.push(serde_json::json!({
                "layer": 3,
                "path": entry.path,
                "ok": content_ok,
            }));
        } else if content_ok {
            println!(
                "{}",
                t!(
                    "cli.verify.messages.layer3_ok",
                    locale = locale.as_str(),
                    path = entry.path.as_str()
                )
            );
        } else {
            println!(
                "{}",
                t!(
                    "cli.verify.messages.layer3_fail",
                    locale = locale.as_str(),
                    path = entry.path.as_str()
                )
            );
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&json_rows)?);
    }

    if ok {
        Ok(())
    } else {
        Err(eyre!(t!(
            "cli.verify.errors.verify_failed",
            locale = locale.as_str()
        )))
    }
}
