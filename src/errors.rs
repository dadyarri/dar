#[allow(dead_code)]
/// Structured error type for the most common `dari` failure categories.
///
/// Placing validation errors behind a concrete enum rather than `eyre!()` with
/// a translated string serves two goals:
///
/// 1. **Machine consumption** — a future `--json` output mode can serialise the
///    error variant and its fields without parsing a human-readable message.
/// 2. **Unit-testing** — tests can `assert!(matches!(err, DariError::CorruptArchive(_)))`
///    instead of fragile string comparisons.
///
/// All variants implement [`std::error::Error`].  Because `DariError: Error + Send + Sync`,
/// eyre's blanket `From<E: Error> for eyre::Report` applies automatically, so
/// existing `?` call-sites that return `eyre::Result<T>` work without any
/// additional `impl From` boilerplate.
#[derive(Debug)]
pub enum DariError {
    /// The archive bytes are structurally invalid (bad magic, truncated index, …).
    CorruptArchive(String),
    /// The passphrase supplied is incompatible with the archive's encryption mode.
    ///
    /// E.g. providing a passphrase for an unencrypted archive, or omitting one
    /// for an encrypted archive.
    EncryptionMismatch(String),
    /// An archive-relative path already exists in the archive and the chosen
    /// conflict mode does not allow overwriting or renaming.
    PathConflict { existing: String },
    /// The archive was written with a format version that this binary does not
    /// understand.
    UnsupportedVersion { found: u8, max_supported: u8 },
}

impl std::fmt::Display for DariError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DariError::CorruptArchive(msg) => write!(f, "corrupt archive: {msg}"),
            DariError::EncryptionMismatch(msg) => write!(f, "encryption mismatch: {msg}"),
            DariError::PathConflict { existing } => {
                write!(
                    f,
                    "path conflict: '{existing}' already exists in the archive"
                )
            }
            DariError::UnsupportedVersion { found, max_supported } => write!(
                f,
                "unsupported archive version: found {found}, max supported {max_supported}"
            ),
        }
    }
}

impl std::error::Error for DariError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_corrupt_archive_display() {
        let e = DariError::CorruptArchive("bad magic bytes".to_string());
        assert!(e.to_string().contains("corrupt archive"));
        assert!(e.to_string().contains("bad magic bytes"));
    }

    #[test]
    fn test_encryption_mismatch_display() {
        let e = DariError::EncryptionMismatch("passphrase on unencrypted archive".to_string());
        assert!(e.to_string().contains("encryption mismatch"));
    }

    #[test]
    fn test_path_conflict_display() {
        let e = DariError::PathConflict {
            existing: "src/main.rs".to_string(),
        };
        assert!(e.to_string().contains("src/main.rs"));
        assert!(e.to_string().contains("already exists"));
    }

    #[test]
    fn test_unsupported_version_display() {
        let e = DariError::UnsupportedVersion {
            found: 6,
            max_supported: 5,
        };
        let msg = e.to_string();
        assert!(msg.contains("6"));
        assert!(msg.contains("5"));
    }

    #[test]
    fn test_convert_to_eyre_report() {
        let e = DariError::CorruptArchive("test".to_string());
        // DariError implements std::error::Error so eyre can wrap it via Into<eyre::Report>.
        let report = eyre::Report::new(e);
        assert!(report.to_string().contains("corrupt archive"));
    }
}
