use crate::models::archive::{ArchiveIndexEntryWrapper, CompressionMethod};
use crate::pipeline::INDEX_FLAG_LINKED_DATA;
use crate::traits::decompress_bytes;
use chacha20poly1305::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce, Tag};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Extra-field keys that belong to the encryption subsystem; never shown in UI.
const ENCRYPTION_KEYS: &[&str] = &["e", "en", "et"];

/// Mapping of short extra-field keys to their `rust_i18n` key paths.
pub const KNOWN_TAGS: &[(&str, &str)] = &[
    ("imk", "tui.inspect.preview.tags.imk"),
    ("imd", "tui.inspect.preview.tags.imd"),
    ("idt", "tui.inspect.preview.tags.idt"),
    ("atl", "tui.inspect.preview.tags.atl"),
    ("aar", "tui.inspect.preview.tags.aar"),
    ("aal", "tui.inspect.preview.tags.aal"),
    ("agn", "tui.inspect.preview.tags.agn"),
];

/// Maximum bytes of decoded text content shown in the preview.
const TEXT_PREVIEW_LIMIT: usize = 1024 * 1024; // 1 MiB

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Display-ready metadata extracted from an archive index entry.
pub struct EntryMetadata {
    pub compression_method: String,
    pub original_size: u32,
    /// Stored/compressed size; 0 means the entry was stored uncompressed.
    pub compressed_size: u32,
    /// Full 64-char lowercase hex of the BLAKE3 checksum.
    pub checksum_hex: String,
    /// Non-encryption tags parsed from the `extra` field: (display_name, value).
    pub extra_tags: Vec<(String, String)>,
}

/// The decoded file content, or a reason it cannot be displayed.
pub enum PreviewContent {
    /// Entry is encrypted; no passphrase was supplied on the CLI.
    EncryptedNoPassphrase,
    /// Entry is encrypted; the supplied passphrase did not decrypt successfully.
    EncryptedWrongPassphrase,
    /// Successfully decoded text content.
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

/// Complete preview for one archive entry.
pub struct EntryPreview {
    pub metadata: EntryMetadata,
    pub content: PreviewContent,
}

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

/// Returns `true` when the `extra` field contains an `e=…` encryption marker.
pub fn is_entry_encrypted(extra: &str) -> bool {
    parse_pairs(extra).iter().any(|(k, _)| k == "e")
}

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

    let content = match load_bytes(archive_path, entry, all_entries) {
        None => PreviewContent::Binary,
        Some(raw) => {
            if encrypted {
                match passphrase {
                    None => PreviewContent::EncryptedNoPassphrase,
                    Some(pass) => match decrypt_bytes(&raw, &entry.entry.checksum, pass) {
                        None => PreviewContent::EncryptedWrongPassphrase,
                        Some(decrypted) => decode_content(entry, &decrypted),
                    },
                }
            } else {
                decode_content(entry, &raw)
            }
        }
    };

    EntryPreview { metadata, content }
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

    let all_pairs = parse_pairs(&entry.extra);
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

    let method_name = match entry.entry.compression_method {
        CompressionMethod::None => "None",
        CompressionMethod::Brotli => "Brotli",
        CompressionMethod::Zstandard => "Zstandard",
        CompressionMethod::Lzma => "LZMA",
        CompressionMethod::LeptonJpeg => "Lepton JPEG",
    };

    EntryMetadata {
        compression_method: method_name.to_string(),
        original_size: entry.entry.original_size,
        compressed_size: entry.entry.compressed_size,
        checksum_hex,
        extra_tags,
    }
}

/// Parse `"k1=v1;k2=v2;…"` into a `Vec<(String, String)>`, skipping empty segments.
pub fn parse_pairs(extra: &str) -> Vec<(String, String)> {
    extra
        .split(';')
        .filter_map(|seg| {
            let mut it = seg.splitn(2, '=');
            let k = it.next()?.trim();
            let v = it.next()?.trim();
            if k.is_empty() || v.is_empty() {
                None
            } else {
                Some((k.to_string(), v.to_string()))
            }
        })
        .collect()
}

/// Resolve the data offset for `entry`, following the linked-data chain.
fn resolve_offset(
    entry: &ArchiveIndexEntryWrapper,
    all_entries: &[ArchiveIndexEntryWrapper],
) -> Option<u64> {
    if entry.entry.bitflags & INDEX_FLAG_LINKED_DATA != 0 {
        let cs = entry.entry.checksum;
        all_entries
            .iter()
            .find(|e| e.entry.checksum == cs && (e.entry.bitflags & INDEX_FLAG_LINKED_DATA) == 0)
            .map(|e| e.entry.offset as u64)
    } else {
        Some(entry.entry.offset as u64)
    }
}

/// Read the raw (compressed / encrypted) bytes from the archive file.
fn load_bytes(
    archive_path: &Path,
    entry: &ArchiveIndexEntryWrapper,
    all_entries: &[ArchiveIndexEntryWrapper],
) -> Option<Vec<u8>> {
    let offset = resolve_offset(entry, all_entries)?;
    let mut file = File::open(archive_path).ok()?;
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = vec![0u8; entry.entry.compressed_size as usize];
    file.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// Decrypt a ChaCha20-Poly1305 ciphertext using the same scheme as `pipeline.rs`.
///
/// Returns `None` if the data is too short or the AEAD tag does not verify.
fn decrypt_bytes(data: &[u8], checksum: &[u8; 32], passphrase: &str) -> Option<Vec<u8>> {
    if data.len() < 16 {
        return None;
    }
    let tag_bytes = &data[data.len() - 16..];
    let mut ciphertext = data[..data.len() - 16].to_vec();
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&checksum[..12]);
    let key = blake3::derive_key("dari.v1.chacha20poly1305.key", passphrase.as_bytes());
    let cipher = ChaCha20Poly1305::new((&key).into());
    cipher
        .decrypt_in_place_detached(
            Nonce::from_slice(&nonce),
            b"",
            &mut ciphertext,
            Tag::from_slice(tag_bytes),
        )
        .ok()?;
    Some(ciphertext)
}

/// Decompress `raw` bytes and then classify them as text or binary.
fn decode_content(entry: &ArchiveIndexEntryWrapper, raw: &[u8]) -> PreviewContent {
    let plain =
        decompress_bytes(entry.entry.compression_method, raw).unwrap_or_else(|_| raw.to_vec());
    classify_bytes(&plain)
}

/// Classify `bytes` as UTF-8 text, Windows-1251 text, or binary.
fn classify_bytes(bytes: &[u8]) -> PreviewContent {
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


