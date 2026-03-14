use crate::counting_writer::CountingWriter;
use eyre::{eyre, Result};
use std::io::{Read, Write};

/// A trait that defines how to read different integer types from bytes
pub trait FromLeBytes: Sized {
    const SIZE: usize;
    fn from_le_bytes(bytes: &[u8]) -> Self;
}

impl FromLeBytes for u8 {
    const SIZE: usize = 1;
    fn from_le_bytes(bytes: &[u8]) -> u8 {
        u8::from_le_bytes(bytes.try_into().unwrap())
    }
}

impl FromLeBytes for u16 {
    const SIZE: usize = 2;
    fn from_le_bytes(bytes: &[u8]) -> u16 {
        u16::from_le_bytes(bytes.try_into().unwrap())
    }
}

impl FromLeBytes for u32 {
    const SIZE: usize = 4;
    fn from_le_bytes(bytes: &[u8]) -> Self {
        u32::from_le_bytes(bytes.try_into().unwrap())
    }
}

impl FromLeBytes for u64 {
    const SIZE: usize = 8;
    fn from_le_bytes(bytes: &[u8]) -> Self {
        u64::from_le_bytes(bytes.try_into().unwrap())
    }
}

pub trait Compressor {
    fn get_best_extensions(&self) -> &[&str];
    fn compress<InputType, OutputType>(
        &self,
        input: &mut InputType,
        output: &mut OutputType,
    ) -> Result<u64>
    where
        InputType: Read,
        OutputType: Write;
}

pub struct NoneCompressor;
impl Compressor for NoneCompressor {
    fn get_best_extensions(&self) -> &[&str] {
        &[
            "jpg", "jpeg", "png", "webp", "gif", "mp4", "mp3", "zip", "gz", "rar", "7z", "pdf",
            "bz2", "zst", "tar", "tar.gz", "bz",
        ]
    }

    fn compress<InputType, OutputType>(
        &self,
        input: &mut InputType,
        output: &mut OutputType,
    ) -> Result<u64>
    where
        InputType: Read,
        OutputType: Write,
    {
        let mut counter = CountingWriter::new(output);
        std::io::copy(input, &mut counter)?;
        Ok(counter.bytes_written)
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

    fn compress<InputType, OutputType>(
        &self,
        input: &mut InputType,
        output: &mut OutputType,
    ) -> Result<u64>
    where
        InputType: Read,
        OutputType: Write,
    {
        let mut params = brotli::enc::BrotliEncoderParams::default();
        params.quality = 11;
        params.lgwin = 24;
        let mut counter = CountingWriter::new(output);
        brotli::BrotliCompress(input, &mut counter, &params)
            .map_err(|e| eyre!("Brotli compression error: {}", e))?;
        Ok(counter.bytes_written)
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

    fn compress<InputType, OutputType>(
        &self,
        input: &mut InputType,
        output: &mut OutputType,
    ) -> Result<u64>
    where
        InputType: Read,
        OutputType: Write,
    {
        let mut counter = CountingWriter::new(output);
        counter.write_all(
            &*zstd::encode_all(input, 19)
                .map_err(|e| eyre!("ZStandard compression error: {}", e))?,
        )?;
        Ok(counter.bytes_written)
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

    fn compress<InputType, OutputType>(
        &self,
        input: &mut InputType,
        output: &mut OutputType,
    ) -> Result<u64>
    where
        InputType: Read,
        OutputType: Write,
    {
        let mut counter = CountingWriter::new(output);
        {
            let mut encoder = xz2::write::XzEncoder::new(&mut counter, 9);
            std::io::copy(input, &mut encoder)?;
            encoder.finish()?;
        }
        Ok(counter.bytes_written)
    }
}
