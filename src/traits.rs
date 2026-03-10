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
