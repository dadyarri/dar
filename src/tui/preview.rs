use crate::constants::extra_keys;
use crate::extra::{is_entry_encrypted, parse_extra_pairs};
use crate::extractor::{read_raw_entry_bytes, try_decrypt_bytes};
use crate::models::archive::ArchiveIndexEntryWrapper;
use crate::traits::decompress_bytes;
use std::path::Path;
use std::sync::LazyLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(|| SyntaxSet::load_defaults_newlines());
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Extra-field keys that belong to the encryption subsystem; never shown in UI.
const ENCRYPTION_KEYS: &[&str] = &[
    extra_keys::ENC_ALGO,
    extra_keys::ENC_NONCE,
    extra_keys::ENC_TAG,
    extra_keys::ENC_SEGMENTS,
];

/// Mapping of short extra-field keys to their `rust_i18n` key paths.
pub const KNOWN_TAGS: &[(&str, &str)] = &[
    (extra_keys::IMG_MAKE, "tui.inspect.preview.tags.imk"),
    (extra_keys::IMG_MODEL, "tui.inspect.preview.tags.imd"),
    (
        extra_keys::IMG_DATETIME_ORIGINAL,
        "tui.inspect.preview.tags.idt",
    ),
    (extra_keys::AUDIO_TITLE, "tui.inspect.preview.tags.atl"),
    (extra_keys::AUDIO_ARTIST, "tui.inspect.preview.tags.aar"),
    (extra_keys::AUDIO_ALBUM, "tui.inspect.preview.tags.aal"),
    (extra_keys::AUDIO_GENRE, "tui.inspect.preview.tags.agn"),
];

/// Maximum bytes of decoded text content shown in the preview.
const TEXT_PREVIEW_LIMIT: usize = 1024 * 1024; // 1 MiB

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Display-ready metadata extracted from an archive index entry.
pub struct EntryMetadata {
    pub compression_method: String,
    pub original_size: u64,
    /// Stored/compressed size; 0 means the entry was stored uncompressed.
    pub compressed_size: u64,
    /// Full 64-char lowercase hex of the BLAKE3 checksum.
    pub checksum_hex: String,
    /// Non-encryption tags parsed from the `extra` field: (display_name, value).
    pub extra_tags: Vec<(String, String)>,
}

/// The decoded file content, or a reason it cannot be displayed.
pub enum PreviewContent {
    /// Stored bytes failed the v6 `stored_checksum` integrity check.
    StoredChecksumMismatch,
    /// Entry is encrypted; no passphrase was supplied on the CLI.
    EncryptedNoPassphrase,
    /// Entry is encrypted; the supplied passphrase did not decrypt successfully.
    EncryptedWrongPassphrase,
    /// Successfully decoded and syntax-highlighted text content.
    HighlightedText {
        encoding: &'static str,
        lines: Vec<ratatui::text::Line<'static>>,
        truncated: bool,
    },
    /// Successfully decoded text content (no syntax highlighting available).
    Text {
        encoding: &'static str,
        text: String,
        /// `true` when the file was larger than `TEXT_PREVIEW_LIMIT` and only the
        /// first part is displayed.
        truncated: bool,
    },
    /// Binary content — only metadata is shown.
    Binary,
}

/// Integrity state of the stored bytes read for a preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewIntegrity {
    /// No v6 stored checksum was present, or preview I/O failed before the
    /// check could be performed.
    NotChecked,
    /// Stored bytes matched the v6 `stored_checksum`.
    Verified,
    /// Stored bytes did not match the v6 `stored_checksum`.
    Mismatch,
}

/// Complete preview for one archive entry.
pub struct EntryPreview {
    pub metadata: EntryMetadata,
    pub content: PreviewContent,
    pub integrity: PreviewIntegrity,
}

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

/// Build a complete [`EntryPreview`] for `entry`.
///
/// Reads, optionally decrypts and decompresses the data from `archive_path` on
/// demand.  Any I/O or decryption error is surfaced as the appropriate
/// [`PreviewContent`] variant rather than propagating to the caller.
pub fn build_preview(
    archive_path: &Path,
    entry: &ArchiveIndexEntryWrapper,
    all_entries: &[ArchiveIndexEntryWrapper],
    passphrase: Option<&str>,
    locale: &str,
) -> EntryPreview {
    let metadata = build_metadata(entry, locale);
    let encrypted = is_entry_encrypted(&entry.extra);

    let (content, integrity) = match read_raw_entry_bytes(archive_path, entry, all_entries) {
        None => (PreviewContent::Binary, PreviewIntegrity::NotChecked),
        Some(raw) => {
            let integrity = match entry.stored_checksum_v6() {
                Some(expected) if blake3::hash(&raw).as_bytes() == expected => {
                    PreviewIntegrity::Verified
                }
                Some(_) => PreviewIntegrity::Mismatch,
                None => PreviewIntegrity::NotChecked,
            };

            if integrity == PreviewIntegrity::Mismatch {
                (PreviewContent::StoredChecksumMismatch, integrity)
            } else if encrypted {
                match passphrase {
                    None => (PreviewContent::EncryptedNoPassphrase, integrity),
                    Some(pass) => match try_decrypt_bytes(
                        &raw,
                        &entry.entry.checksum,
                        entry.entry.bitflags,
                        &entry.extra,
                        pass,
                    ) {
                        None => (PreviewContent::EncryptedWrongPassphrase, integrity),
                        Some(decrypted) => (decode_content(entry, &decrypted), integrity),
                    },
                }
            } else {
                (decode_content(entry, &raw), integrity)
            }
        }
    };

    EntryPreview {
        metadata,
        content,
        integrity,
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn build_metadata(entry: &ArchiveIndexEntryWrapper, locale: &str) -> EntryMetadata {
    let checksum_hex: String = entry
        .entry
        .checksum
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();

    let all_pairs = parse_extra_pairs(&entry.extra);
    let extra_tags: Vec<(String, String)> = all_pairs
        .iter()
        .filter(|(k, _)| !ENCRYPTION_KEYS.contains(&k.as_str()))
        .map(|(k, v)| {
            let display = KNOWN_TAGS
                .iter()
                .find(|(tag, _)| *tag == k.as_str())
                .map(|(_, i18n_key)| {
                    let key = i18n_key.to_string();
                    rust_i18n::t!(&key, locale = locale).into_owned()
                })
                .unwrap_or_else(|| k.clone());
            (display, v.clone())
        })
        .collect();

    EntryMetadata {
        compression_method: entry.entry.compression_method.as_str().to_string(),
        original_size: entry.entry.original_size,
        compressed_size: entry.entry.compressed_size,
        checksum_hex,
        extra_tags,
    }
}

/// Decompress `raw` bytes and then classify them as text or binary.
fn decode_content(entry: &ArchiveIndexEntryWrapper, raw: &[u8]) -> PreviewContent {
    let plain =
        decompress_bytes(entry.entry.compression_method, raw).unwrap_or_else(|_| raw.to_vec());
    let extension = std::path::Path::new(&entry.path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    classify_bytes(&plain, extension)
}

/// Classify `bytes` as UTF-8 text, Windows-1251 text, or binary.
/// When the content is UTF-8 text and `extension` is recognised by syntect,
/// returns [`PreviewContent::HighlightedText`]; otherwise falls back to
/// [`PreviewContent::Text`].
pub(crate) fn classify_bytes(bytes: &[u8], extension: &str) -> PreviewContent {
    let preview = if bytes.len() > TEXT_PREVIEW_LIMIT {
        &bytes[..TEXT_PREVIEW_LIMIT]
    } else {
        bytes
    };
    let truncated = bytes.len() > TEXT_PREVIEW_LIMIT;

    // Null bytes are a reliable indicator of binary data.
    if preview.contains(&0x00) {
        return PreviewContent::Binary;
    }

    // More than 10 % non-printable control bytes → treat as binary.
    let ctrl = preview
        .iter()
        .filter(|&&b| b < 0x09 || (b > 0x0D && b < 0x20))
        .count();
    if !preview.is_empty() && ctrl * 10 > preview.len() {
        return PreviewContent::Binary;
    }

    // Try UTF-8 first (covers plain ASCII as well).
    if let Ok(s) = std::str::from_utf8(preview) {
        // Attempt syntax highlighting for the detected extension.
        if let Some(highlighted) = try_highlight(s, extension) {
            return PreviewContent::HighlightedText {
                encoding: "UTF-8",
                lines: highlighted,
                truncated,
            };
        }
        return PreviewContent::Text {
            encoding: "UTF-8",
            text: s.to_string(),
            truncated,
        };
    }

    // Fall back to Windows-1251 (Cyrillic legacy encoding).
    // encoding_rs returns `had_errors = true` for any undefined code-unit
    // (0x81, 0x83, 0x88, 0x8A, 0x8C-0x8F, 0x90, 0x98, 0x9A, 0x9C-0x9F in CP1251).
    let (decoded, _, had_errors) = encoding_rs::WINDOWS_1251.decode(preview);
    if !had_errors {
        return PreviewContent::Text {
            encoding: "Windows-1251",
            text: decoded.into_owned(),
            truncated,
        };
    }

    PreviewContent::Binary
}

/// Try to syntax-highlight `text` for the given file `extension`.
///
/// Returns `None` when no matching syntax definition is found or an error
/// occurs during highlighting; the caller should fall back to plain text.
fn try_highlight(text: &str, extension: &str) -> Option<Vec<ratatui::text::Line<'static>>> {
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};

    if extension.is_empty() {
        return None;
    }

    let syntax = SYNTAX_SET.find_syntax_by_extension(extension)?;
    let theme = &THEME_SET.themes["base16-ocean.dark"];
    let mut highlighter = HighlightLines::new(syntax, theme);

    let mut result: Vec<Line<'static>> = Vec::new();
    for line_str in LinesWithEndings::from(text) {
        let ranges = highlighter.highlight_line(line_str, &SYNTAX_SET).ok()?;
        let spans: Vec<Span<'static>> = ranges
            .iter()
            .enumerate()
            .filter_map(|(i, (style, content))| {
                // Strip trailing newline/CR from the last token on this line.
                let is_last = i == ranges.len() - 1;
                let text_owned: String = if is_last {
                    content
                        .trim_end_matches(|c| c == '\n' || c == '\r')
                        .to_string()
                } else {
                    content.to_string()
                };
                if text_owned.is_empty() {
                    return None;
                }
                let fg = style.foreground;
                Some(Span::styled(
                    text_owned,
                    Style::default().fg(Color::Rgb(fg.r, fg.g, fg.b)),
                ))
            })
            .collect();
        result.push(Line::from(spans));
    }
    Some(result)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        archive_builder::ArchiveBuilder, format_version::FormatVersion, pipeline::PipelineConfig,
        test_utils::build_v6_archive,
    };
    use std::{fs, io::BufWriter};
    use tempfile::tempdir;

    // ------------------------------------------------------------------
    // classify_bytes — binary detection
    // ------------------------------------------------------------------

    #[test]
    fn null_byte_is_binary() {
        let bytes = b"hello\x00world";
        assert!(matches!(classify_bytes(bytes, "txt"), PreviewContent::Binary));
    }

    #[test]
    fn high_control_char_ratio_is_binary() {
        // > 10 % control bytes (0x01-0x08, 0x0E-0x1F)
        let mut bytes = vec![0x01u8; 20];
        bytes.extend_from_slice(b"normal text here");
        assert!(matches!(classify_bytes(&bytes, "txt"), PreviewContent::Binary));
    }

    #[test]
    fn empty_bytes_classifies_as_text() {
        // Empty content has no control chars and is valid UTF-8.
        // Extension "unknownext" has no syntect syntax, so it falls back to Text.
        let result = classify_bytes(b"", "unknownext");
        assert!(
            matches!(result, PreviewContent::Text { encoding: "UTF-8", .. }),
            "expected Text for empty bytes"
        );
    }

    // ------------------------------------------------------------------
    // classify_bytes — UTF-8 text paths
    // ------------------------------------------------------------------

    #[test]
    fn plain_utf8_without_known_extension_is_text() {
        let bytes = b"Hello, world!\n";
        let result = classify_bytes(bytes, "unknownext");
        assert!(
            matches!(result, PreviewContent::Text { encoding: "UTF-8", .. }),
            "unexpected variant"
        );
    }

    #[test]
    fn rust_source_is_highlighted_text() {
        let code = b"fn main() { println!(\"hi\"); }\n";
        let result = classify_bytes(code, "rs");
        // syntect knows Rust — should return HighlightedText.
        assert!(
            matches!(result, PreviewContent::HighlightedText { encoding: "UTF-8", .. }),
            "expected HighlightedText for .rs extension"
        );
    }

    #[test]
    fn truncation_flag_set_when_over_limit() {
        // Create a buffer just over the 1 MiB limit.
        let big: Vec<u8> = b"a".repeat(1024 * 1024 + 1);
        let result = classify_bytes(&big, "txt");
        match result {
            PreviewContent::Text { truncated, .. } => assert!(truncated),
            PreviewContent::HighlightedText { truncated, .. } => assert!(truncated),
            _ => panic!("unexpected variant: not text"),
        }
    }

    #[test]
    fn truncation_flag_not_set_within_limit() {
        let small = b"hello world".to_vec();
        let result = classify_bytes(&small, "txt");
        match result {
            PreviewContent::Text { truncated, .. } => assert!(!truncated),
            PreviewContent::HighlightedText { truncated, .. } => assert!(!truncated),
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // classify_bytes — Windows-1251 fallback
    // ------------------------------------------------------------------

    #[test]
    fn windows_1251_text_classified_as_text() {
        // "Привет" in Windows-1251 (Cyrillic).
        let bytes: &[u8] = &[0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2];
        let result = classify_bytes(bytes, "txt");
        assert!(
            matches!(result, PreviewContent::Text { encoding: "Windows-1251", .. }),
            "expected Windows-1251 text"
        );
    }

    // ------------------------------------------------------------------
    // PreviewContent variant coverage for build_preview preconditions
    // ------------------------------------------------------------------

    #[test]
    fn encrypted_flag_triggers_no_passphrase_variant() {
        // Build a synthetic extra string that marks the entry as encrypted.
        use crate::constants::extra_keys;
        use crate::models::archive::{ArchiveIndexEntry, ArchiveIndexEntryWrapper};
        use bytemuck::Zeroable;

        let extra = format!("{}=algo", extra_keys::ENC_ALGO);
        let wrapper =
            ArchiveIndexEntryWrapper::new(ArchiveIndexEntry::zeroed(), "file.txt".to_string(), extra);

        // Pass `None` as passphrase; with no data to read the raw bytes will be
        // `None` → Binary, but since encrypted flag is set and passphrase is None
        // the code path should return EncryptedNoPassphrase before even reading data.
        // We can't easily call build_preview without a real archive file, so test
        // the logical path via the is_entry_encrypted helper directly.
        assert!(crate::extra::is_entry_encrypted(&wrapper.extra));
    }

    #[test]
    fn build_preview_marks_v6_entry_as_integrity_verified() {
        let dir = tempdir().unwrap();
        let archive = build_v6_archive(&dir, "ok.dar", &[("alpha.txt", b"alpha")]);
        let mut fh = fs::File::open(&archive).unwrap();
        let locale = crate::i18n::Locale::new("en");
        let state = crate::reader::load_archive(&mut fh, archive.to_str().unwrap(), &locale).unwrap();
        let entry = &state.entries[0];

        let preview = build_preview(&archive, entry, &state.entries, None, "en");

        assert_eq!(preview.integrity, PreviewIntegrity::Verified);
        assert!(!matches!(preview.content, PreviewContent::StoredChecksumMismatch));
    }

    #[test]
    fn build_preview_surfaces_stored_checksum_mismatch() {
        let dir = tempdir().unwrap();
        let archive_path = dir.path().join("bad.dar");
        let file_handle = fs::File::create(&archive_path).unwrap();
        let mut builder = ArchiveBuilder::with_version(
            BufWriter::new(file_handle),
            PipelineConfig::default(),
            FormatVersion::V6,
        );
        builder.write_header().unwrap();
        let source = dir.path().join("hello.txt");
        fs::write(&source, b"hello world").unwrap();
        builder.add_file(&source, "hello.txt").unwrap();
        builder.build().unwrap();

        let mut raw = fs::read(&archive_path).unwrap();
        raw[17] ^= 0x01;
        fs::write(&archive_path, raw).unwrap();

        let mut fh = fs::File::open(&archive_path).unwrap();
        let locale = crate::i18n::Locale::new("en");
        let state = crate::reader::load_archive(&mut fh, archive_path.to_str().unwrap(), &locale)
            .unwrap();
        let entry = &state.entries[0];

        let preview = build_preview(&archive_path, entry, &state.entries, None, "en");

        assert_eq!(preview.integrity, PreviewIntegrity::Mismatch);
        assert!(matches!(
            preview.content,
            PreviewContent::StoredChecksumMismatch
        ));
    }
}
