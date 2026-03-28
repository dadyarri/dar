use eyre::{Context, Result};
use rust_i18n::t;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
use crate::traits::FromLeBytes;
#[cfg(test)]
use eyre::ContextCompat;

/// Gets current timestamp in seconds since Unix Epoch
pub fn get_unix_timestamp() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .wrap_err(t!("cli.common.errors.time_before_unix_epoch"))?
        .as_secs())
}

/// Reads number of specified type in bytearray starting from specified index
#[cfg(test)]
pub fn read_bytes_as<T: FromLeBytes>(bytes: &[u8], starting_from: usize) -> Result<T> {
    let end = starting_from + T::SIZE;

    let slice = bytes.get(starting_from..end).wrap_err_with(|| {
        format!(
            "Index out of bounds for {} bytes at position {}",
            T::SIZE,
            starting_from
        )
    })?;

    Ok(T::from_le_bytes(slice))
}

/// Reads string of specified length starting from specified index from bytearray
#[cfg(test)]
pub fn read_string(bytes: &[u8], starting_from: usize, length: usize) -> Result<String> {
    let end = starting_from + length;
    let slice = bytes.get(starting_from..end).wrap_err_with(|| {
        format!(
            "Failed to read string: index out of bounds at {}",
            starting_from
        )
    })?;

    let s =
        std::str::from_utf8(slice).wrap_err("Failed to decode string: invalid UTF-8 sequence")?;

    Ok(s.to_string())
}

/// Calculate relative path starting from directory root
pub fn calculate_archive_path(dir_root: &Path, file_path: &Path) -> String {
    let relative = file_path.strip_prefix(dir_root).unwrap_or(file_path);
    let path_str = relative.to_string_lossy().to_string();
    sanitize_path(&path_str)
}

pub fn get_mode(metadata: &fs::Metadata) -> (u32, u32, u16) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        (metadata.uid(), metadata.gid(), metadata.mode() as u16)
    }

    #[cfg(not(unix))]
    {
        (1000, 1000, 644) // Placeholder for Windows/non-Unix platforms
    }
}

/// Cleanup path
fn sanitize_path(path: &str) -> String {
    let mut components = Vec::new();
    let path_obj = PathBuf::from(path);

    for component in path_obj.components() {
        match component {
            Component::ParentDir | Component::RootDir => {
                // Skip parent directory and root directory references
            }
            Component::Normal(name) => {
                if let Some(s) = name.to_str() {
                    components.push(s.to_string());
                }
            }
            Component::CurDir => {
                // Skip current directory references
            }
            Component::Prefix(_) => {
                // Skip prefix components (Windows drive letters, etc.)
            }
        }
    }

    components.join("/")
}
