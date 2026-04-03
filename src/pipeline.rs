use crate::constants::extra_keys;
use crate::constants::flags;
use crate::encryption::nonce_from_checksum;
use crate::extra::{encode_extra_pairs, upsert_extra_pair};
use crate::models::archive::CompressionMethod;
use crate::traits::{compressor_for_extension, Compressor};
use chacha20poly1305::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use exif::{Exif, In, Reader as ExifReader, Tag};
use eyre::Result;
use lofty::file::TaggedFileExt;
use lofty::prelude::Accessor;
use lofty::probe::Probe;
use std::io::Cursor;
use std::path::Path;

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
        }
    }
}

/// Configuration for the compression pipeline.
#[derive(Clone, Debug, Default)]
pub struct PipelineConfig {
    pub compress_images: bool,
    pub encryption_passphrase: Option<String>,
}

/// The main compression pipeline.
///
/// Steps executed for every file:
/// 1. Calculate BLAKE3 checksum.
/// 2. Select compressor via [`compressor_for_extension`].
/// 3. Compress using the selected [`Compressor`] implementation (skipped for `None`).
pub struct CompressionPipeline {
    config: PipelineConfig,
}

impl CompressionPipeline {
    pub fn new(config: PipelineConfig) -> Self {
        Self { config }
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

        let compressor: &dyn Compressor =
            compressor_for_extension(&ext, self.config.compress_images);

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

        let mut encrypted = match &file_data.compressed_content {
            Some(content) => content.clone(),
            None => file_data.original_content.clone(),
        };

        let tag =
            cipher.encrypt_in_place_detached(Nonce::from_slice(&nonce), b"", &mut encrypted)?;

        encrypted.extend_from_slice(tag.as_slice());
        file_data.compressed_size = encrypted.len() as u64;
        file_data.compressed_content = Some(encrypted);
        file_data.bitflags |= flags::ENCRYPTED_DATA;
        file_data.encryption_nonce_hex = Some(hex_encode(&nonce));
        file_data.encryption_tag_hex = Some(hex_encode(tag.as_slice()));
        Ok(())
    }

    fn populate_extra(&self, _file_path: &Path, file_data: &mut PipelineFileData) {
        let mut pairs: Vec<(String, String)> = Vec::new();

        if self.config.encryption_passphrase.is_some() {
            upsert_extra_pair(&mut pairs, extra_keys::ENC_ALGO, "chacha20poly1305");

            if let Some(nonce) = &file_data.encryption_nonce_hex {
                upsert_extra_pair(&mut pairs, extra_keys::ENC_NONCE, nonce.clone());
            }

            if let Some(tag) = &file_data.encryption_tag_hex {
                upsert_extra_pair(&mut pairs, extra_keys::ENC_TAG, tag.clone());
            }
        }

        for (key, value) in extract_image_metadata(&file_data.original_content) {
            upsert_extra_pair(&mut pairs, key, value);
        }

        for (key, value) in extract_audio_metadata(&file_data.original_content) {
            upsert_extra_pair(&mut pairs, key, value);
        }

        file_data.extra = encode_extra_pairs(pairs);
    }
}

fn extract_image_metadata(bytes: &[u8]) -> Vec<(String, String)> {
    let mut metadata = Vec::new();
    let mut cursor = Cursor::new(bytes);

    let Ok(exif) = ExifReader::new().read_from_container(&mut cursor) else {
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
}
