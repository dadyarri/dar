use crate::counting_writer::CountingWriter;
use crate::models::archive::CompressionMethod;
use eyre::{eyre, Result};
use std::io::{Read, Write};

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
}

pub struct NoneCompressor;
impl Compressor for NoneCompressor {
    fn get_best_extensions(&self) -> &[&str] {
        &[
            "jpg", "jpeg", "png", "webp", "gif", "mp4", "mp3", "zip", "gz", "rar", "7z", "pdf",
            "bz2", "zst", "tar", "tar.gz", "bz",
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
        let mut params = brotli::enc::BrotliEncoderParams::default();
        params.quality = 11;
        params.lgwin = 24;
        let mut counter = CountingWriter::new(output);
        brotli::BrotliCompress(&mut cursor, &mut counter, &params)
            .map_err(|e| eyre!("Brotli compression error: {}", e))?;
        Ok(CompressionOutcome {
            bytes_written: counter.bytes_written,
            method: CompressionMethod::Brotli,
        })
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
            &*zstd::encode_all(std::io::Cursor::new(buf), 19)
                .map_err(|e| eyre!("ZStandard compression error: {}", e))?,
        )?;
        Ok(CompressionOutcome {
            bytes_written: counter.bytes_written,
            method: CompressionMethod::Zstandard,
        })
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
        if compressor.get_best_extensions().iter().any(|e| *e == ext) {
            return compressor;
        }
    }

    &ZSTANDARD_COMPRESSOR // default for unknown extensions
}
