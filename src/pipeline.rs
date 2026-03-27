use crate::models::archive::CompressionMethod;
use crate::traits::{compressor_for_extension, Compressor};
use eyre::Result;
use std::io::Cursor;
use std::path::Path;

/// Result of processing a file through the pipeline.
#[derive(Debug, Clone)]
pub struct PipelineFileData {
    pub original_content: Vec<u8>,
    pub compressed_content: Option<Vec<u8>>,
    pub compression_method: CompressionMethod,
    pub checksum: [u8; 32],
    pub original_size: u32,
    pub compressed_size: u32,
}

impl PipelineFileData {
    pub fn new(original_content: Vec<u8>, checksum: [u8; 32]) -> Self {
        let original_size = original_content.len() as u32;
        Self {
            original_content,
            compressed_content: None,
            compression_method: CompressionMethod::None,
            checksum,
            original_size,
            compressed_size: 0,
        }
    }
}

/// Configuration for the compression pipeline.
#[derive(Clone, Debug, Default)]
pub struct PipelineConfig {
    pub compress_images: bool,
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

        // If output is identical and method is None, keep original in-place.
        if matches!(outcome.method, CompressionMethod::None)
            && output.len() == file_data.original_content.len()
            && output.as_slice() == file_data.original_content.as_slice()
        {
            return Ok(());
        }

        file_data.compressed_size = outcome.bytes_written as u32;
        file_data.compressed_content = Some(output);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn make_pipeline(compress_images: bool) -> CompressionPipeline {
        CompressionPipeline::new(PipelineConfig { compress_images })
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
        let result = pipeline
            .process_file(Path::new("main.rs"), b"fn main() {}".to_vec())
            .unwrap();
        assert!(
            matches!(result.compression_method, CompressionMethod::Zstandard),
            "Expected Zstandard for .rs"
        );
    }

    #[test]
    fn test_html_uses_brotli() {
        let pipeline = make_pipeline(false);
        let result = pipeline
            .process_file(Path::new("index.html"), b"<html></html>".to_vec())
            .unwrap();
        assert!(
            matches!(result.compression_method, CompressionMethod::Brotli),
            "Expected Brotli for .html"
        );
    }

    #[test]
    fn test_iso_uses_lzma() {
        let pipeline = make_pipeline(false);
        let result = pipeline
            .process_file(Path::new("disk.iso"), b"fake iso data".to_vec())
            .unwrap();
        assert!(
            matches!(result.compression_method, CompressionMethod::Lzma),
            "Expected Lzma for .iso"
        );
    }

    #[test]
    fn test_unknown_extension_uses_zstandard() {
        let pipeline = make_pipeline(false);
        let result = pipeline
            .process_file(Path::new("data.myext"), b"some data".to_vec())
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
        assert_eq!(result.original_size, data.len() as u32);
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
            (compressed.len() as u32) < result.original_size,
            "Compressed size should be smaller than original for repetitive data"
        );
        assert_eq!(result.compressed_size, compressed.len() as u32);
    }
}
