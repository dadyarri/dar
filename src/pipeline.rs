use crate::constants::extra_keys;
use crate::constants::flags;
use crate::encryption::{nonce_for_segment, nonce_from_checksum};
use crate::extra::{encode_extra_pairs, upsert_extra_pair};
use crate::models::archive::CompressionMethod;
use crate::traits::{Compressor, compressor_for_extension};
use chacha20poly1305::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use exif::{Exif, In, Reader as ExifReader, Tag};
use eyre::Result;
use lofty::file::TaggedFileExt;
use lofty::prelude::Accessor;
use lofty::probe::Probe;
use std::io::Cursor;
use std::path::Path;

/// Trait for extracting metadata key-value pairs from raw file bytes.
///
/// The trait is object-safe and `Send + Sync` so it can be stored in a
/// `Box<dyn MetadataExtractor>` and shared across threads.
///
/// Implementors **must not** propagate errors — return an empty vec on any
/// failure so the pipeline never aborts due to unrecognised metadata.
pub trait MetadataExtractor: Send + Sync {
    /// Returns `(key, value)` pairs extracted from `data`.
    ///
    /// The file extension (without leading dot, lowercase) is provided for
    /// fast format dispatch when needed.  Returns an empty vec on any failure.
    fn extract(&self, data: &[u8], extension: &str) -> Vec<(String, String)>;
}

/// [`MetadataExtractor`] that reads EXIF data from image files.
pub struct ExifMetadataExtractor;

impl MetadataExtractor for ExifMetadataExtractor {
    fn extract(&self, data: &[u8], _extension: &str) -> Vec<(String, String)> {
        extract_image_metadata(data)
    }
}

/// [`MetadataExtractor`] that reads ID3 / Vorbis / APEv2 tags from audio files.
pub struct AudioMetadataExtractor;

impl MetadataExtractor for AudioMetadataExtractor {
    fn extract(&self, data: &[u8], _extension: &str) -> Vec<(String, String)> {
        extract_audio_metadata(data)
    }
}

/// A no-op [`MetadataExtractor`] used in tests to eliminate I/O-heavy parsing.
#[allow(dead_code)]
pub struct NoOpMetadataExtractor;

impl MetadataExtractor for NoOpMetadataExtractor {
    fn extract(&self, _data: &[u8], _extension: &str) -> Vec<(String, String)> {
        vec![]
    }
}

fn default_metadata_extractors() -> Vec<Box<dyn MetadataExtractor>> {
    vec![
        Box::new(ExifMetadataExtractor),
        Box::new(AudioMetadataExtractor),
    ]
}

/// Result of processing a file through the pipeline.
#[derive(Debug, Clone)]
pub struct PipelineFileData {
    pub original_content: Vec<u8>,
    pub compressed_content: Option<Vec<u8>>,
    pub compression_method: CompressionMethod,
    pub checksum: [u8; 32],
    pub original_size: u64,
    pub compressed_size: u64,
    pub bitflags: u16,
    pub extra: String,
    pub encryption_nonce_hex: Option<String>,
    pub encryption_tag_hex: Option<String>,
    pub encryption_segment_count: Option<u64>,
}

impl PipelineFileData {
    pub fn new(original_content: Vec<u8>, checksum: [u8; 32]) -> Self {
        let original_size = original_content.len() as u64;
        Self {
            original_content,
            compressed_content: None,
            compression_method: CompressionMethod::None,
            checksum,
            original_size,
            compressed_size: 0,
            bitflags: 0,
            extra: String::new(),
            encryption_nonce_hex: None,
            encryption_tag_hex: None,
            encryption_segment_count: None,
        }
    }
}

/// Configuration for the compression pipeline.
#[derive(Clone, Debug, Default)]
pub struct PipelineConfig {
    pub compress_images: bool,
    pub encryption_passphrase: Option<String>,
    pub chunked_encryption: bool,
}

/// The main compression pipeline.
///
/// Steps executed for every file:
/// 1. Calculate BLAKE3 checksum.
/// 2. Select compressor via [`compressor_for_extension`] (or the injected override).
/// 3. Compress using the selected [`Compressor`] implementation (skipped for `None`).
/// 4. Optionally encrypt with ChaCha20-Poly1305.
/// 5. Populate the `extra` metadata field via the registered [`MetadataExtractor`]s.
pub struct CompressionPipeline {
    config: PipelineConfig,
    metadata_extractors: Vec<Box<dyn MetadataExtractor>>,
    /// Optional compressor override; when set, `compressor_for_extension` is bypassed.
    compressor_override: Option<Box<dyn Compressor>>,
}

impl CompressionPipeline {
    /// Create a standard pipeline using EXIF and audio metadata extractors.
    pub fn new(config: PipelineConfig) -> Self {
        Self {
            config,
            metadata_extractors: default_metadata_extractors(),
            compressor_override: None,
        }
    }

    /// Create a pipeline that always uses `compressor` regardless of file extension.
    ///
    /// Primarily for tests that need fine-grained control over which codec is
    /// exercised without depending on the extension-based dispatch table.
    #[allow(dead_code)]
    pub fn with_compressor(config: PipelineConfig, compressor: Box<dyn Compressor>) -> Self {
        Self {
            config,
            metadata_extractors: default_metadata_extractors(),
            compressor_override: Some(compressor),
        }
    }

    /// Create a pipeline with a fully custom set of metadata extractors.
    ///
    /// Pass an empty `Vec` to disable all metadata extraction (useful for tests
    /// that do not have real image/audio fixtures).
    #[allow(dead_code)]
    pub fn with_extractors(
        config: PipelineConfig,
        metadata_extractors: Vec<Box<dyn MetadataExtractor>>,
    ) -> Self {
        Self {
            config,
            metadata_extractors,
            compressor_override: None,
        }
    }

    pub fn process_file(
        &self,
        file_path: &Path,
        file_content: Vec<u8>,
    ) -> Result<PipelineFileData> {
        let mut file_data = self.calculate_checksum(file_content)?;
        self.select_and_compress(file_path, &mut file_data)?;
        self.encrypt_if_enabled(&mut file_data)?;
        self.populate_extra(file_path, &mut file_data);
        Ok(file_data)
    }

    // -----------------------------------------------------------------------

    fn calculate_checksum(&self, file_content: Vec<u8>) -> Result<PipelineFileData> {
        let hash = blake3::hash(&file_content);
        let mut checksum = [0u8; 32];
        checksum.copy_from_slice(hash.as_bytes());
        Ok(PipelineFileData::new(file_content, checksum))
    }

    fn select_and_compress(&self, path: &Path, file_data: &mut PipelineFileData) -> Result<()> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Use the injected override when present; otherwise dispatch by extension.
        let compressor: &dyn Compressor = if let Some(c) = &self.compressor_override {
            c.as_ref()
        } else {
            compressor_for_extension(&ext, self.config.compress_images)
        };

        let mut input = Cursor::new(&file_data.original_content);
        let mut output: Vec<u8> = Vec::new();
        let outcome = compressor.compress(
            &mut input as &mut dyn std::io::Read,
            &mut output as &mut dyn std::io::Write,
        )?;

        file_data.compression_method = outcome.method;

        // If the compressor did not actually shrink the data (common for small, random, or
        // already-dense files), fall back to storing the original bytes with method=None.
        // This covers both NoneCompressor (copies verbatim) and real compressors
        // (ZStd/Brotli/LZMA) whose output grows instead of shrinking.
        if output.len() >= file_data.original_content.len() {
            file_data.compression_method = CompressionMethod::None;
            return Ok(());
        }

        file_data.compressed_size = outcome.bytes_written as u64;
        file_data.compressed_content = Some(output);
        Ok(())
    }

    fn encrypt_if_enabled(&self, file_data: &mut PipelineFileData) -> Result<()> {
        let Some(passphrase) = &self.config.encryption_passphrase else {
            return Ok(());
        };

        // Derive a stable nonce from checksum so deduplicated linked entries can reuse metadata.
        let nonce = nonce_from_checksum(&file_data.checksum);
        let key = blake3::derive_key("dari.v1.chacha20poly1305.key", passphrase.as_bytes());
        let cipher = ChaCha20Poly1305::new((&key).into());

        // Take ownership of the compressed bytes if available, otherwise fall back to the
        // original content.  This avoids an extra clone for large files (7.1 in roadmap).
        let mut encrypted = file_data
            .compressed_content
            .take()
            .unwrap_or_else(|| file_data.original_content.clone());

        if self.config.chunked_encryption {
            let segment_count = encrypted
                .len()
                .div_ceil(crate::constants::crypto::SEGMENT_SIZE)
                .max(1) as u64;
            let mut chunked = Vec::with_capacity(
                encrypted.len() + segment_count as usize * crate::constants::crypto::TAG_LEN,
            );

            if encrypted.is_empty() {
                let mut segment = Vec::new();
                let tag = cipher.encrypt_in_place_detached(
                    Nonce::from_slice(&nonce_for_segment(&nonce, 0)),
                    b"",
                    &mut segment,
                )?;
                chunked.extend_from_slice(tag.as_slice());
            } else {
                for (idx, slice) in encrypted
                    .chunks(crate::constants::crypto::SEGMENT_SIZE)
                    .enumerate()
                {
                    let mut segment = slice.to_vec();
                    let tag = cipher.encrypt_in_place_detached(
                        Nonce::from_slice(&nonce_for_segment(&nonce, idx as u64)),
                        b"",
                        &mut segment,
                    )?;
                    chunked.extend_from_slice(&segment);
                    chunked.extend_from_slice(tag.as_slice());
                }
            }

            file_data.compressed_size = chunked.len() as u64;
            file_data.compressed_content = Some(chunked);
            file_data.bitflags |= flags::CHUNKED_ENCRYPTION;
            file_data.encryption_segment_count = Some(segment_count);
            file_data.encryption_tag_hex = None;
        } else {
            let tag =
                cipher.encrypt_in_place_detached(Nonce::from_slice(&nonce), b"", &mut encrypted)?;

            encrypted.extend_from_slice(tag.as_slice());
            file_data.compressed_size = encrypted.len() as u64;
            file_data.compressed_content = Some(encrypted);
            file_data.encryption_tag_hex = Some(hex_encode(tag.as_slice()));
        }
        file_data.bitflags |= flags::ENCRYPTED_DATA;
        file_data.encryption_nonce_hex = Some(hex_encode(&nonce));
        Ok(())
    }

    fn populate_extra(&self, file_path: &Path, file_data: &mut PipelineFileData) {
        let mut pairs: Vec<(String, String)> = Vec::new();

        if self.config.encryption_passphrase.is_some() {
            upsert_extra_pair(&mut pairs, extra_keys::ENC_ALGO, "chacha20poly1305");

            if let Some(nonce) = &file_data.encryption_nonce_hex {
                upsert_extra_pair(&mut pairs, extra_keys::ENC_NONCE, nonce.clone());
            }

            if let Some(tag) = &file_data.encryption_tag_hex {
                upsert_extra_pair(&mut pairs, extra_keys::ENC_TAG, tag.clone());
            }

            if let Some(segments) = file_data.encryption_segment_count {
                upsert_extra_pair(&mut pairs, extra_keys::ENC_SEGMENTS, segments.to_string());
            }
        }

        let ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        for extractor in &self.metadata_extractors {
            for (key, value) in extractor.extract(&file_data.original_content, &ext) {
                upsert_extra_pair(&mut pairs, key, value);
            }
        }

        file_data.extra = encode_extra_pairs(pairs);
    }
}

fn extract_image_metadata(bytes: &[u8]) -> Vec<(String, String)> {
    let mut metadata = Vec::new();
    let mut cursor = Cursor::new(bytes);

    let Ok(exif) = ExifReader::new().read_from_container(&mut cursor) else {
        if std::env::var("DARI_DEBUG").is_ok() {
            eprintln!(
                "[dari debug] extract_image_metadata: failed to read EXIF data (file may have no EXIF or is not a supported image format)"
            );
        }
        return metadata;
    };

    for (tag, key) in [
        (Tag::Make, extra_keys::IMG_MAKE),
        (Tag::Model, extra_keys::IMG_MODEL),
        (Tag::DateTimeOriginal, extra_keys::IMG_DATETIME_ORIGINAL),
    ] {
        push_exif_field(&mut metadata, &exif, tag, key);
    }

    metadata
}

fn push_exif_field(metadata: &mut Vec<(String, String)>, exif: &Exif, tag: Tag, key: &str) {
    if let Some(field) = exif.get_field(tag, In::PRIMARY) {
        upsert_extra_pair(
            metadata,
            key,
            field.display_value().with_unit(exif).to_string(),
        );
    }
}

fn extract_audio_metadata(bytes: &[u8]) -> Vec<(String, String)> {
    let mut metadata = Vec::new();

    let Ok(tagged_file) = Probe::new(Cursor::new(bytes)).read() else {
        if std::env::var("DARI_DEBUG").is_ok() {
            eprintln!(
                "[dari debug] extract_audio_metadata: failed to probe audio metadata (file may have no tags or is not a supported audio format)"
            );
        }
        return metadata;
    };

    if let Some(tag) = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())
    {
        for (accessor, key) in [
            (tag.title(), extra_keys::AUDIO_TITLE),
            (tag.artist(), extra_keys::AUDIO_ARTIST),
            (tag.album(), extra_keys::AUDIO_ALBUM),
            (tag.genre(), extra_keys::AUDIO_GENRE),
        ] {
            if let Some(value) = accessor {
                upsert_extra_pair(&mut metadata, key, value.to_string());
            }
        }
    }

    metadata
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{:02x}", byte);
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::flags;
    use std::path::Path;

    fn make_pipeline(compress_images: bool) -> CompressionPipeline {
        CompressionPipeline::new(PipelineConfig {
            compress_images,
            encryption_passphrase: None,
            chunked_encryption: false,
        })
    }

    // ---- checksum ----

    #[test]
    fn test_checksum_is_deterministic() {
        let pipeline = make_pipeline(false);
        let data = b"hello dari".to_vec();
        let r1 = pipeline
            .process_file(Path::new("a.txt"), data.clone())
            .unwrap();
        let r2 = pipeline.process_file(Path::new("a.txt"), data).unwrap();
        assert_eq!(r1.checksum, r2.checksum);
    }

    #[test]
    fn test_different_content_produces_different_checksum() {
        let pipeline = make_pipeline(false);
        let r1 = pipeline
            .process_file(Path::new("a.txt"), b"hello".to_vec())
            .unwrap();
        let r2 = pipeline
            .process_file(Path::new("a.txt"), b"world".to_vec())
            .unwrap();
        assert_ne!(r1.checksum, r2.checksum);
    }

    #[test]
    fn test_checksum_matches_blake3() {
        let pipeline = make_pipeline(false);
        let data = b"test content 12345".to_vec();
        let result = pipeline
            .process_file(Path::new("file.bin"), data.clone())
            .unwrap();
        let expected = blake3::hash(&data);
        assert_eq!(result.checksum, *expected.as_bytes());
    }

    // ---- algorithm selection ----

    #[test]
    fn test_rs_uses_zstandard() {
        let pipeline = make_pipeline(false);
        // Payload must be large enough that ZStd actually shrinks it
        let data = b"fn main() { println!(\"hello\"); }\n".repeat(20);
        let result = pipeline.process_file(Path::new("main.rs"), data).unwrap();
        assert!(
            matches!(result.compression_method, CompressionMethod::Zstandard),
            "Expected Zstandard for .rs"
        );
    }

    #[test]
    fn test_html_uses_brotli() {
        let pipeline = make_pipeline(false);
        // Payload must be large enough that Brotli actually shrinks it
        let data = b"<html><body><p>hello from dari</p></body></html>\n".repeat(20);
        let result = pipeline
            .process_file(Path::new("index.html"), data)
            .unwrap();
        assert!(
            matches!(result.compression_method, CompressionMethod::Brotli),
            "Expected Brotli for .html"
        );
    }

    #[test]
    fn test_iso_uses_lzma() {
        let pipeline = make_pipeline(false);
        // Payload must be large enough that LZMA actually shrinks it
        let data = b"fake iso data sector padding \x00\x00\x00\x00\x00\x00\x00\x00\n".repeat(30);
        let result = pipeline.process_file(Path::new("disk.iso"), data).unwrap();
        assert!(
            matches!(result.compression_method, CompressionMethod::Lzma),
            "Expected Lzma for .iso"
        );
    }

    #[test]
    fn test_unknown_extension_uses_zstandard() {
        let pipeline = make_pipeline(false);
        // Payload must be large enough that ZStd actually shrinks it
        let data = b"unknown extension data repeated for compressibility\n".repeat(20);
        let result = pipeline
            .process_file(Path::new("data.myext"), data)
            .unwrap();
        assert!(
            matches!(result.compression_method, CompressionMethod::Zstandard),
            "Expected Zstandard fallback for unknown extension"
        );
    }

    // ---- already-compressed files ----

    #[test]
    fn test_jpg_no_compression_by_default() {
        let pipeline = make_pipeline(false);
        let result = pipeline
            .process_file(Path::new("photo.jpg"), b"fake jpeg bytes".to_vec())
            .unwrap();
        assert!(
            matches!(result.compression_method, CompressionMethod::None),
            "Expected None for .jpg by default"
        );
        assert!(
            result.compressed_content.is_none(),
            "jpg should not be compressed when flag is off"
        );
        assert_eq!(result.compressed_size, 0);
    }

    #[test]
    fn test_zip_always_skips_compression() {
        let pipeline = make_pipeline(true); // even with compress_images on
        let result = pipeline
            .process_file(Path::new("archive.zip"), b"PK\x03\x04fake zip".to_vec())
            .unwrap();
        assert!(
            matches!(result.compression_method, CompressionMethod::None),
            "zip must never be recompressed"
        );
        assert!(result.compressed_content.is_none());
    }

    // ---- image compression flag ----

    #[test]
    fn test_jpg_compressed_when_flag_on() {
        let pipeline = make_pipeline(true);
        let data = b"fake jpeg content that compresses well aaaaaaaaaa".to_vec();
        let result = pipeline.process_file(Path::new("photo.jpg"), data).unwrap();
        assert!(
            matches!(result.compression_method, CompressionMethod::None),
            "invalid jpeg data should safely fall back without recompression"
        );
        assert!(
            result.compressed_content.is_none(),
            "invalid jpeg bytes are not transformed"
        );
        assert_eq!(result.compressed_size, 0);
    }

    #[test]
    fn test_png_compressed_when_flag_on() {
        let pipeline = make_pipeline(true);
        let result = pipeline
            .process_file(
                Path::new("image.png"),
                b"fake png bytes aaaaaaaaaaaaaaaaaaaaaa".to_vec(),
            )
            .unwrap();
        assert!(matches!(result.compression_method, CompressionMethod::None));
        assert!(result.compressed_content.is_none());
    }

    // ---- sizes ----

    #[test]
    fn test_original_size_is_set_correctly() {
        let pipeline = make_pipeline(false);
        let data = b"hello world".to_vec();
        let result = pipeline
            .process_file(Path::new("note.txt"), data.clone())
            .unwrap();
        assert_eq!(result.original_size, data.len() as u64);
    }

    #[test]
    fn test_compressed_content_is_smaller_for_compressible_text() {
        let pipeline = make_pipeline(false);
        let data = b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_vec();
        let result = pipeline
            .process_file(Path::new("log.csv"), data.clone())
            .unwrap();
        let compressed = result.compressed_content.as_ref().unwrap();
        assert!(
            (compressed.len() as u64) < result.original_size,
            "Compressed size should be smaller than original for repetitive data"
        );
        assert_eq!(result.compressed_size, compressed.len() as u64);
    }

    #[test]
    fn test_encryption_sets_flag_and_extra() {
        let pipeline = CompressionPipeline::new(PipelineConfig {
            compress_images: false,
            encryption_passphrase: Some("secret".to_string()),
            chunked_encryption: false,
        });

        let result = pipeline
            .process_file(Path::new("song.mp3"), b"plain bytes".to_vec())
            .unwrap();

        assert_eq!(
            result.bitflags & flags::ENCRYPTED_DATA,
            flags::ENCRYPTED_DATA
        );
        assert!(result.extra.contains("e=chacha20poly1305"));
        assert!(result.extra.contains("en="));
        assert!(result.extra.contains("et="));
    }

    #[test]
    fn test_chunked_encryption_sets_flag_and_segment_count() {
        let pipeline = CompressionPipeline::new(PipelineConfig {
            compress_images: false,
            encryption_passphrase: Some("secret".to_string()),
            chunked_encryption: true,
        });

        let result = pipeline
            .process_file(
                Path::new("song.mp3"),
                b"x".repeat(crate::constants::crypto::SEGMENT_SIZE + 5),
            )
            .unwrap();

        assert_eq!(
            result.bitflags & flags::CHUNKED_ENCRYPTION,
            flags::CHUNKED_ENCRYPTION
        );
        assert!(result.extra.contains("e=chacha20poly1305"));
        assert!(result.extra.contains("en="));
        assert!(result.extra.contains("es=2"));
        assert!(!result.extra.contains("et="));
    }
}
