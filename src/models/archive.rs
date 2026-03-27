use crate::utils::get_unix_timestamp;
use bytemuck::{Pod, Zeroable};
use eyre::{eyre, Error};
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
    pub fn new() -> Self {
        Self {
            signature: *b"DARI",
            version: 5,
            timestamp: get_unix_timestamp().unwrap(),
        }
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
    LeptonJpeg,
}

impl TryFrom<u8> for CompressionMethod {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(CompressionMethod::None),
            1 => Ok(CompressionMethod::Brotli),
            2 => Ok(CompressionMethod::Zstandard),
            3 => Ok(CompressionMethod::Lzma),
            4 => Ok(CompressionMethod::LeptonJpeg),
            _ => Err(eyre!("Invalid value for CompressionMethod: {}", value)),
        }
    }
}

impl Into<u8> for CompressionMethod {
    fn into(self: CompressionMethod) -> u8 {
        match self {
            CompressionMethod::None => 0,
            CompressionMethod::Brotli => 1,
            CompressionMethod::Zstandard => 2,
            CompressionMethod::Lzma => 3,
            CompressionMethod::LeptonJpeg => 4,
        }
    }
}

impl CompressionMethod {
    pub fn as_byte(&self) -> u8 {
        *self as u8
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct ArchiveIndexEntry {
    pub offset: u32,
    pub bitflags: u16,
    pub compression_method: CompressionMethod,
    pub modification_timestamp: u64,
    pub uid: u32,
    pub gid: u32,
    pub perm: u16,
    pub checksum: [u8; 32],
    pub original_size: u32,
    pub compressed_size: u32,
    pub path_length: u32,
    pub extra_length: u32,
}

unsafe impl Pod for ArchiveIndexEntry {}
unsafe impl Zeroable for ArchiveIndexEntry {}

impl ArchiveIndexEntry {
    pub fn new(
        offset: u32,
        bitflags: u16,
        compression_method: CompressionMethod,
        modification_timestamp: u64,
        uid: u32,
        gid: u32,
        perm: u16,
        checksum: [u8; 32],
        original_size: u32,
        compressed_size: u32,
        path_length: u32,
        extra_length: u32,
    ) -> Self {
        Self {
            offset,
            bitflags,
            compression_method,
            modification_timestamp,
            uid,
            gid,
            perm,
            checksum,
            original_size,
            compressed_size,
            path_length,
            extra_length,
        }
    }

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
