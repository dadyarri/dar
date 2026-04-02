use crate::utils::get_unix_timestamp;
use bytemuck::{Pod, Zeroable};
use eyre::{Error, Result, eyre};
use rust_i18n::t;
use std::io::Write;

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct ArchiveHeader {
    pub signature: [u8; 4],
    pub version: u8,
    pub timestamp: u64,
}

unsafe impl Pod for ArchiveHeader {}
unsafe impl Zeroable for ArchiveHeader {}

impl ArchiveHeader {
    pub fn new() -> Result<Self> {
        Ok(Self {
            signature: *b"DARI",
            version: 5,
            timestamp: get_unix_timestamp()?,
        })
    }

    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(bytemuck::bytes_of(self))
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct ArchiveFooter {
    pub signature: [u8; 7],
    pub index_offset: u32,
    pub amount_of_files: u32,
}

unsafe impl Pod for ArchiveFooter {}
unsafe impl Zeroable for ArchiveFooter {}

impl ArchiveFooter {
    pub fn new(index_offset: u32, amount_of_files: u32) -> Self {
        Self {
            signature: *b"DARIEND",
            index_offset,
            amount_of_files,
        }
    }

    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(bytemuck::bytes_of(self))
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum CompressionMethod {
    None,
    Brotli,
    Zstandard,
    Lzma,
}

impl TryFrom<u8> for CompressionMethod {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(CompressionMethod::None),
            1 => Ok(CompressionMethod::Brotli),
            2 => Ok(CompressionMethod::Zstandard),
            3 => Ok(CompressionMethod::Lzma),
            _ => Err(eyre!(t!(
                "cli.common.errors.invalid_compression_method",
                value = value
            ))),
        }
    }
}

impl From<CompressionMethod> for u8 {
    fn from(value: CompressionMethod) -> Self {
        match value {
            CompressionMethod::None => 0,
            CompressionMethod::Brotli => 1,
            CompressionMethod::Zstandard => 2,
            CompressionMethod::Lzma => 3,
        }
    }
}

impl CompressionMethod {
    /// Human-readable algorithm name used in display and metadata output.
    pub fn as_str(self) -> &'static str {
        match self {
            CompressionMethod::None => "None",
            CompressionMethod::Brotli => "Brotli",
            CompressionMethod::Zstandard => "Zstandard",
            CompressionMethod::Lzma => "LZMA",
        }
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct ArchiveIndexEntry {
    pub offset: u64,
    pub bitflags: u16,
    pub compression_method: CompressionMethod,
    pub modification_timestamp: u64,
    pub uid: u32,
    pub gid: u32,
    pub perm: u16,
    pub checksum: [u8; 32],
    pub original_size: u64,
    pub compressed_size: u64,
    pub path_length: u32,
    pub extra_length: u32,
}

unsafe impl Pod for ArchiveIndexEntry {}
unsafe impl Zeroable for ArchiveIndexEntry {}

impl ArchiveIndexEntry {
    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(bytemuck::bytes_of(self))
    }
}

pub struct ArchiveIndexEntryWrapper {
    pub entry: ArchiveIndexEntry,
    pub path: String,
    pub extra: String,
}

impl ArchiveIndexEntryWrapper {
    pub fn new(entry: ArchiveIndexEntry, path: String, extra: String) -> Self {
        Self { entry, path, extra }
    }
}
