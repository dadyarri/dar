use crate::counting_writer::CountingWriter;
use crate::models::archive::CompressionMethod;
use eyre::{Result, eyre};
use rust_i18n::t;
use std::io::{BufReader, Read, Write};

pub struct CompressionOutcome {
    pub bytes_written: u64,
    pub method: CompressionMethod,
}

/// A trait that defines how to read different integer types from bytes
#[cfg(test)]
pub trait FromLeBytes: Sized {
    const SIZE: usize;
    fn from_le_bytes(bytes: &[u8]) -> Self;
}

#[cfg(test)]
impl FromLeBytes for u8 {
    const SIZE: usize = 1;
    fn from_le_bytes(bytes: &[u8]) -> u8 {
        u8::from_le_bytes(bytes.try_into().unwrap())
    }
}

#[cfg(test)]
impl FromLeBytes for u16 {
    const SIZE: usize = 2;
    fn from_le_bytes(bytes: &[u8]) -> u16 {
        u16::from_le_bytes(bytes.try_into().unwrap())
    }
}

#[cfg(test)]
impl FromLeBytes for u32 {
    const SIZE: usize = 4;
    fn from_le_bytes(bytes: &[u8]) -> Self {
        u32::from_le_bytes(bytes.try_into().unwrap())
    }
}

#[cfg(test)]
impl FromLeBytes for u64 {
    const SIZE: usize = 8;
    fn from_le_bytes(bytes: &[u8]) -> Self {
        u64::from_le_bytes(bytes.try_into().unwrap())
    }
}

pub trait Compressor {
    fn get_best_extensions(&self) -> &[&str];
    /// Compress `input` into `output`, returning the number of bytes written.
    ///
    /// Using trait objects (`&mut dyn Read` / `&mut dyn Write`) keeps this
    /// trait object-safe so it can be used as `&dyn Compressor`.
    fn compress(&self, input: &mut dyn Read, output: &mut dyn Write) -> Result<CompressionOutcome>;
    /// Decompress `data` that was previously produced by [`compress`](Self::compress).
    fn decompress_bytes(&self, data: &[u8]) -> Result<Vec<u8>>;
}

pub struct NoneCompressor;
impl Compressor for NoneCompressor {
    fn get_best_extensions(&self) -> &[&str] {
        &[
            // Images / video / audio already using lossy or proprietary codecs
            "jpg", "jpeg", "png", "webp", "gif", "mp4", "mp3", "aac", "ogg", "flac", "wav",
            "mkv", "avi", "mov", "m4a", "m4v",
            // Already-compressed archives / containers
            "zip", "gz", "rar", "7z", "bz2", "zst", "tar", "bz", "xz", "lzma", "lz4", "lz",
            "zlib",
            // JVM / mobile / package archives (ZIP-based, already compressed)
            "jar", "war", "ear", "apk", "ipa", "aab",
            // Language-specific package archives
            "whl", "egg", "nupkg", "gem",
            // Binary / document formats with internal compression
            "pdf", "docx", "xlsx", "pptx", "odt", "ods", "odp", "epub", "cbz",
            // WebAssembly (binary bytecode, compresses poorly)
            "wasm",
        ]
    }

    fn compress(&self, input: &mut dyn Read, output: &mut dyn Write) -> Result<CompressionOutcome> {
        let mut counter = CountingWriter::new(output);
        std::io::copy(input, &mut counter)?;
        Ok(CompressionOutcome {
            bytes_written: counter.bytes_written,
            method: CompressionMethod::None,
        })
    }

    fn decompress_bytes(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }
}

pub struct BrotliCompressor;
impl Compressor for BrotliCompressor {
    fn get_best_extensions(&self) -> &[&str] {
        &[
            "html", "htm", "xhtml", "css", "scss", "sass", "less", "stylus", "js", "mjs", "json",
            "svg", "xml", "xsl", "xsd", "txt", "md", "markdown", "rst", "toml", "yaml", "yml",
            "woff2", "jsx", "ts", "tsx", "mts",
        ]
    }

    fn compress(&self, input: &mut dyn Read, output: &mut dyn Write) -> Result<CompressionOutcome> {
        let mut buf = Vec::new();
        input.read_to_end(&mut buf)?;
        let mut cursor = std::io::Cursor::new(buf);
        let params = brotli::enc::BrotliEncoderParams {
            quality: 6,
            lgwin: 22,
            ..Default::default()
        };
        let mut counter = CountingWriter::new(output);
        brotli::BrotliCompress(&mut cursor, &mut counter, &params)
            .map_err(|e| eyre!(t!("cli.common.errors.brotli_compression_failed", error = e)))?;
        Ok(CompressionOutcome {
            bytes_written: counter.bytes_written,
            method: CompressionMethod::Brotli,
        })
    }

    fn decompress_bytes(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        brotli::BrotliDecompress(&mut std::io::Cursor::new(data), &mut output)
            .map_err(|e| eyre!(t!("cli.common.errors.brotli_decompression_failed", error = e)))?;
        Ok(output)
    }
}

pub struct ZStandardCompressor;
impl Compressor for ZStandardCompressor {
    fn get_best_extensions(&self) -> &[&str] {
        &[
            "log", "tar", "csv", "tsv", "db", "sql", "bak", "rs", "go", "java", "kt", "py", "rb",
            "php", "pl", "pas", "c", "cpp", "c++", "h", "hpp", "cs", "fs", "vb", "vba", "sql",
            "sh", "bat", "ps1", "fish", "proto", "thrift",
        ]
    }

    fn compress(&self, input: &mut dyn Read, output: &mut dyn Write) -> Result<CompressionOutcome> {
        let mut buf = Vec::new();
        input.read_to_end(&mut buf)?;
        let mut counter = CountingWriter::new(output);
        counter.write_all(
            &zstd::encode_all(std::io::Cursor::new(buf), 3)
                .map_err(|e| eyre!(t!("cli.common.errors.zstd_compression_failed", error = e)))?,
        )?;
        Ok(CompressionOutcome {
            bytes_written: counter.bytes_written,
            method: CompressionMethod::Zstandard,
        })
    }

    fn decompress_bytes(&self, data: &[u8]) -> Result<Vec<u8>> {
        zstd::decode_all(std::io::Cursor::new(data))
            .map_err(|e| eyre!(t!("cli.common.errors.zstd_decompression_failed", error = e)))
    }
}

pub struct LzmaCompressor;
impl Compressor for LzmaCompressor {
    fn get_best_extensions(&self) -> &[&str] {
        &[
            "iso", "img", "bin", "deb", "rpm", "pkg", "vmdk", "patch", "diff", "fortran", "f90",
            "ada", "lisp", "scm", "hs", "erl", "cmake", "makefile", "mk", "tex", "bib",
        ]
    }

    fn compress(&self, input: &mut dyn Read, output: &mut dyn Write) -> Result<CompressionOutcome> {
        let mut counter = CountingWriter::new(output);
        {
            let mut encoder = xz2::write::XzEncoder::new(&mut counter, 9);
            std::io::copy(input, &mut encoder)?;
            encoder.finish()?;
        }
        Ok(CompressionOutcome {
            bytes_written: counter.bytes_written,
            method: CompressionMethod::Lzma,
        })
    }

    fn decompress_bytes(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = xz2::read::XzDecoder::new(std::io::Cursor::new(data));
        let mut output = Vec::new();
        decoder
            .read_to_end(&mut output)
            .map_err(|e| eyre!(t!("cli.common.errors.lzma_decompression_failed", error = e)))?;
        Ok(output)
    }
}

pub struct PngOxipngCompressor;
impl Compressor for PngOxipngCompressor {
    fn get_best_extensions(&self) -> &[&str] {
        &["png"]
    }

    fn compress(&self, input: &mut dyn Read, output: &mut dyn Write) -> Result<CompressionOutcome> {
        let mut original = Vec::new();
        input.read_to_end(&mut original)?;

        let opts = oxipng::Options::max_compression();
        let optimized = oxipng::optimize_from_memory(&original, &opts).ok();
        let chosen = match optimized {
            Some(bytes) if bytes.len() < original.len() => bytes,
            _ => original,
        };

        let mut counter = CountingWriter::new(output);
        counter.write_all(&chosen)?;
        Ok(CompressionOutcome {
            bytes_written: counter.bytes_written,
            method: CompressionMethod::None,
        })
    }

    /// PNG data is always stored with [`CompressionMethod::None`]; decompression is identity.
    fn decompress_bytes(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }
}

pub struct JpegLeptonCompressor;
impl Compressor for JpegLeptonCompressor {
    fn get_best_extensions(&self) -> &[&str] {
        &["jpg", "jpeg"]
    }

    fn compress(&self, input: &mut dyn Read, output: &mut dyn Write) -> Result<CompressionOutcome> {
        let mut original = Vec::new();
        input.read_to_end(&mut original)?;

        let features = lepton_jpeg::EnabledFeatures::compat_lepton_vector_write();
        let optimized = lepton_jpeg::encode_lepton_verify(
            &original,
            &features,
            &lepton_jpeg::DEFAULT_THREAD_POOL,
        )
        .ok()
        .map(|(bytes, _)| bytes);

        let (chosen, method) = match optimized {
            Some(bytes) if bytes.len() < original.len() => (bytes, CompressionMethod::LeptonJpeg),
            _ => (original, CompressionMethod::None),
        };

        let mut counter = CountingWriter::new(output);
        counter.write_all(&chosen)?;
        Ok(CompressionOutcome {
            bytes_written: counter.bytes_written,
            method,
        })
    }

    fn decompress_bytes(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut reader = BufReader::new(std::io::Cursor::new(data));
        let mut output = Vec::new();
        lepton_jpeg::decode_lepton(
            &mut reader,
            &mut output,
            &lepton_jpeg::EnabledFeatures::compat_lepton_vector_read(),
            &lepton_jpeg::DEFAULT_THREAD_POOL,
        )
        .map_err(|e| {
            eyre!(t!(
                "cli.common.errors.lepton_decompression_failed",
                error = e.message()
            ))
        })?;
        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// Static instances – zero-cost, usable as `&'static dyn Compressor`
// ---------------------------------------------------------------------------

pub static NONE_COMPRESSOR: NoneCompressor = NoneCompressor;
pub static BROTLI_COMPRESSOR: BrotliCompressor = BrotliCompressor;
pub static ZSTANDARD_COMPRESSOR: ZStandardCompressor = ZStandardCompressor;
pub static LZMA_COMPRESSOR: LzmaCompressor = LzmaCompressor;
pub static PNG_OXIPNG_COMPRESSOR: PngOxipngCompressor = PngOxipngCompressor;
pub static JPEG_LEPTON_COMPRESSOR: JpegLeptonCompressor = JpegLeptonCompressor;

/// Returns the best compressor for the given lowercase file extension.
///
/// Compressors are checked in priority order: None → Brotli → ZStandard → Lzma.
/// Unknown extensions fall back to [`ZSTANDARD_COMPRESSOR`].
pub fn compressor_for_extension(ext: &str, compress_images: bool) -> &'static dyn Compressor {
    if compress_images {
        if ext == "png" {
            return &PNG_OXIPNG_COMPRESSOR;
        }

        if ext == "jpg" || ext == "jpeg" {
            return &JPEG_LEPTON_COMPRESSOR;
        }
    }

    let candidates: &[&'static dyn Compressor] = &[
        &NONE_COMPRESSOR,
        &BROTLI_COMPRESSOR,
        &ZSTANDARD_COMPRESSOR,
        &LZMA_COMPRESSOR,
    ];

    for &compressor in candidates {
        if compressor.get_best_extensions().contains(&ext) {
            return compressor;
        }
    }

    &ZSTANDARD_COMPRESSOR // default for unknown extensions
}

/// Decompress `data` that was stored with the given [`CompressionMethod`].
///
/// Dispatches to the matching [`Compressor::decompress_bytes`] implementation.
/// [`CompressionMethod::None`] is a no-op (returns a copy of the input).
pub fn decompress_bytes(method: CompressionMethod, data: &[u8]) -> Result<Vec<u8>> {
    match method {
        CompressionMethod::None => NONE_COMPRESSOR.decompress_bytes(data),
        CompressionMethod::Brotli => BROTLI_COMPRESSOR.decompress_bytes(data),
        CompressionMethod::Zstandard => ZSTANDARD_COMPRESSOR.decompress_bytes(data),
        CompressionMethod::Lzma => LZMA_COMPRESSOR.decompress_bytes(data),
        CompressionMethod::LeptonJpeg => JPEG_LEPTON_COMPRESSOR.decompress_bytes(data),
    }
}

#[cfg(test)]
mod decompress_tests {
    use super::*;

    /// Compress `data` with `compressor`, return the raw compressed bytes.
    fn compress_to_vec(compressor: &dyn Compressor, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cursor = std::io::Cursor::new(data);
        compressor.compress(&mut cursor, &mut out).unwrap();
        out
    }

    #[test]
    fn test_none_decompress_is_identity() {
        let data = b"raw bytes, no compression";
        let result = NONE_COMPRESSOR.decompress_bytes(data).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_brotli_round_trip() {
        let data = b"hello dari, this is highly compressible data aaaaaaaaaaaaaaaaaaa";
        let compressed = compress_to_vec(&BROTLI_COMPRESSOR, data);
        let decompressed = BROTLI_COMPRESSOR.decompress_bytes(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_zstandard_round_trip() {
        let data = b"fn main() { println!(\"hello world\"); }";
        let compressed = compress_to_vec(&ZSTANDARD_COMPRESSOR, data);
        let decompressed = ZSTANDARD_COMPRESSOR.decompress_bytes(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_lzma_round_trip() {
        let data = b"iso image simulation data aaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let compressed = compress_to_vec(&LZMA_COMPRESSOR, data);
        let decompressed = LZMA_COMPRESSOR.decompress_bytes(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_free_decompress_bytes_none() {
        let data = b"passthrough";
        let result = decompress_bytes(CompressionMethod::None, data).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_free_decompress_bytes_brotli() {
        let data = b"web content aaaaaaaaaaaaaaaaaaaaaaaaa";
        let compressed = compress_to_vec(&BROTLI_COMPRESSOR, data);
        let result = decompress_bytes(CompressionMethod::Brotli, &compressed).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_free_decompress_bytes_zstandard() {
        let data = b"source code aaaaaaaaaaaaaaaaaaaaaaaaa";
        let compressed = compress_to_vec(&ZSTANDARD_COMPRESSOR, data);
        let result = decompress_bytes(CompressionMethod::Zstandard, &compressed).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_free_decompress_bytes_lzma() {
        let data = b"binary data aaaaaaaaaaaaaaaaaaaaaaaaa";
        let compressed = compress_to_vec(&LZMA_COMPRESSOR, data);
        let result = decompress_bytes(CompressionMethod::Lzma, &compressed).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_brotli_decompress_rejects_garbage() {
        assert!(BROTLI_COMPRESSOR.decompress_bytes(b"not brotli data").is_err());
    }

    #[test]
    fn test_zstandard_decompress_rejects_garbage() {
        assert!(ZSTANDARD_COMPRESSOR.decompress_bytes(b"not zstd data").is_err());
    }

    #[test]
    fn test_lzma_decompress_rejects_garbage() {
        assert!(LZMA_COMPRESSOR.decompress_bytes(b"not lzma data").is_err());
    }

    #[test]
    fn test_lepton_decompress_rejects_garbage() {
        assert!(JPEG_LEPTON_COMPRESSOR.decompress_bytes(b"not lepton data").is_err());
    }
}

