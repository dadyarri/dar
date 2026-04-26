use eyre::{Context, Result};
use rust_i18n::t;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
use crate::traits::FromLeBytes;
#[cfg(test)]
use eyre::ContextCompat;

/// Return the current timestamp in whole seconds since the Unix epoch.
pub fn get_unix_timestamp() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .wrap_err(t!("cli.common.errors.time_before_unix_epoch"))?
        .as_secs())
}

/// Read a little-endian numeric value of type `T` from `bytes` at `starting_from`.
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

/// Read a UTF-8 string slice of `length` bytes from `bytes` at `starting_from`.
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

/// Compute the archive-relative path for `file_path` under `dir_root`.
///
/// The returned path is sanitized so parent traversals, absolute prefixes, and
/// platform-specific root components are removed.
///
/// # Examples
///
/// ```
/// use dari::utils::calculate_archive_path;
/// use std::path::Path;
///
/// let root = Path::new("/tmp/project");
/// let file = Path::new("/tmp/project/src/main.rs");
/// assert_eq!(calculate_archive_path(root, file), "src/main.rs");
/// ```
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

/// Returns the CLDR plural-category suffix for `n` in the given `locale`.
///
/// | suffix   | when (ru)                                  | when (en / default)  |
/// |----------|--------------------------------------------|----------------------|
/// | `"one"`  | n % 10 == 1, n % 100 != 11                 | n == 1               |
/// | `"few"`  | n % 10 in 2–4, n % 100 not in 12–14       | —                    |
/// | `"many"` | everything else (ru)                       | —                    |
/// | `"other"`| —                                          | everything else (en) |
///
/// Combine the suffix with a dot-separated key prefix to get the final i18n key:
/// ```
/// use dari::utils::{plural_key, plural_suffix};
///
/// assert_eq!(plural_suffix(1, "en"), "one");
/// assert_eq!(plural_suffix(2, "ru"), "few");
/// assert_eq!(
///     plural_key(5, "tui.inspect.status_total", "ru"),
///     "tui.inspect.status_total_many"
/// );
/// ```
pub fn plural_suffix(n: usize, locale: &str) -> &'static str {
    match locale {
        "ru" => {
            let rem10 = n % 10;
            let rem100 = n % 100;
            if rem10 == 1 && rem100 != 11 {
                "one"
            } else if (2..=4).contains(&rem10) && !(12..=14).contains(&rem100) {
                "few"
            } else {
                "many"
            }
        }
        _ => {
            if n == 1 {
                "one"
            } else {
                "other"
            }
        }
    }
}

/// Return the fully-qualified i18n key for a plural-aware translation.
///
/// # Examples
///
/// ```
/// use dari::utils::plural_key;
///
/// assert_eq!(
///     plural_key(5, "tui.inspect.status_total", "ru"),
///     "tui.inspect.status_total_many"
/// );
/// ```
pub fn plural_key(n: usize, prefix: &str, locale: &str) -> String {
    format!("{prefix}_{}", plural_suffix(n, locale))
}

/// Normalize a path string for storage inside an archive.
///
/// This removes `..`, root markers, and Windows-style prefixes, then rejoins
/// the surviving path components with `/`.
pub(crate) fn sanitize_path(path: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::{calculate_archive_path, plural_key, plural_suffix};
    use std::path::Path;

    // --- plural_suffix ---

    #[test]
    fn test_plural_suffix_english_one() {
        assert_eq!(plural_suffix(1, "en"), "one");
    }

    #[test]
    fn test_plural_suffix_english_other() {
        assert_eq!(plural_suffix(0, "en"), "other");
        assert_eq!(plural_suffix(2, "en"), "other");
        assert_eq!(plural_suffix(42, "en"), "other");
    }

    #[test]
    fn test_plural_suffix_russian_one() {
        assert_eq!(plural_suffix(1, "ru"), "one");
        assert_eq!(plural_suffix(21, "ru"), "one");
        assert_eq!(plural_suffix(101, "ru"), "one");
    }

    #[test]
    fn test_plural_suffix_russian_few() {
        assert_eq!(plural_suffix(2, "ru"), "few");
        assert_eq!(plural_suffix(3, "ru"), "few");
        assert_eq!(plural_suffix(4, "ru"), "few");
        assert_eq!(plural_suffix(22, "ru"), "few");
    }

    #[test]
    fn test_plural_suffix_russian_many() {
        assert_eq!(plural_suffix(5, "ru"), "many");
        assert_eq!(plural_suffix(11, "ru"), "many");
        assert_eq!(plural_suffix(12, "ru"), "many");
        assert_eq!(plural_suffix(0, "ru"), "many");
    }

    // --- plural_key ---

    #[test]
    fn test_plural_key_combines_prefix_and_suffix() {
        assert_eq!(plural_key(1, "tui.status", "en"), "tui.status_one");
        assert_eq!(plural_key(2, "tui.status", "en"), "tui.status_other");
        assert_eq!(plural_key(5, "tui.status", "ru"), "tui.status_many");
    }

    // --- calculate_archive_path (and sanitize_path) ---

    #[test]
    fn test_calculate_archive_path_strips_prefix() {
        let root = Path::new("/some/dir");
        let file = Path::new("/some/dir/sub/file.txt");
        assert_eq!(calculate_archive_path(root, file), "sub/file.txt");
    }

    #[test]
    fn test_calculate_archive_path_root_file() {
        let root = Path::new("/some/dir");
        let file = Path::new("/some/dir/file.txt");
        assert_eq!(calculate_archive_path(root, file), "file.txt");
    }

    #[test]
    fn test_calculate_archive_path_sanitizes_parent_traversal() {
        // ".." components must be stripped; the result must be a plain relative path.
        let root = Path::new("/some/dir");
        let file = Path::new("/some/dir/../../../etc/passwd");
        let result = calculate_archive_path(root, file);
        assert!(!result.contains(".."), "result must not contain '..'");
        assert!(!result.starts_with('/'), "result must not be absolute");
    }

    #[test]
    fn test_calculate_archive_path_sanitizes_absolute_prefix() {
        // strip_prefix fails here, so the raw path is sanitized instead.
        let root = Path::new("/other");
        let file = Path::new("/absolute/path/file.txt");
        let result = calculate_archive_path(root, file);
        assert!(!result.starts_with('/'), "result must not be absolute");
    }
}
