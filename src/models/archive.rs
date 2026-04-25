use crate::constants::format;
use crate::utils::get_unix_timestamp;
use bytemuck::{Pod, Zeroable};
use eyre::{Error, Result, eyre};
use rust_i18n::t;
use std::io::Write;

// ---------------------------------------------------------------------------
// v5 structs (stable; used by load_v5 / build_v5)
// ---------------------------------------------------------------------------

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
            signature: *format::SIGNATURE,
            version: format::VERSION,
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
            signature: *format::FOOTER_SIGNATURE,
            index_offset,
            amount_of_files,
        }
    }

    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(bytemuck::bytes_of(self))
    }
}

// ---------------------------------------------------------------------------
// v6 structs (new in Phase 1)
// ---------------------------------------------------------------------------

/// v6 archive header — 17 bytes.
///
/// Extends the v5 header with `volume_number` and `total_volumes` for
/// multi-volume archive support (Phase 3).
#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct ArchiveHeaderV6 {
    pub signature: [u8; 4],
    pub version: u8,
    pub timestamp: u64,
    /// 0-based index of this volume; 0 for single-file archives.
    pub volume_number: u16,
    /// Total number of volumes in the set; 1 for single-file archives.
    pub total_volumes: u16,
}

unsafe impl Pod for ArchiveHeaderV6 {}
unsafe impl Zeroable for ArchiveHeaderV6 {}

impl ArchiveHeaderV6 {
    pub fn new() -> Result<Self> {
        Ok(Self {
            signature: *format::SIGNATURE,
            version: 6,
            timestamp: get_unix_timestamp()?,
            volume_number: 0,
            total_volumes: 1,
        })
    }

    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(bytemuck::bytes_of(self))
    }
}

/// v6 archive footer — 19 bytes.
///
/// Widens `index_offset` from `u32` to `u64`, removing the 4 GiB ceiling.
#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct ArchiveFooterV6 {
    pub signature: [u8; 7],
    /// Byte offset of the index section within this volume.
    pub index_offset: u64,
    pub amount_of_files: u32,
}

unsafe impl Pod for ArchiveFooterV6 {}
unsafe impl Zeroable for ArchiveFooterV6 {}

impl ArchiveFooterV6 {
    pub fn new(index_offset: u64, amount_of_files: u32) -> Self {
        Self {
            signature: *format::FOOTER_SIGNATURE,
            index_offset,
            amount_of_files,
        }
    }

    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(bytemuck::bytes_of(self))
    }
}

/// v6 index entry — 123 bytes.
///
/// Adds `stored_checksum` (BLAKE3 of on-disk bytes), `xattr_length` (Phase 6),
/// and `volume_number` (Phase 3) compared to the v5 layout.
///
/// On-disk field order (matches the roadmap spec):
/// offset(8) bitflags(2) compression_method(1) modification_timestamp(8)
/// uid(4) gid(4) perm(2) checksum(32) stored_checksum(32) original_size(8)
/// compressed_size(8) path_length(4) extra_length(4) xattr_length(4) volume_number(2)
#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct ArchiveIndexEntryV6 {
    pub offset: u64,
    pub bitflags: u16,
    pub compression_method: CompressionMethod,
    pub modification_timestamp: u64,
    pub uid: u32,
    pub gid: u32,
    pub perm: u16,
    /// BLAKE3 hash of the original (pre-compression, pre-encryption) content.
    pub checksum: [u8; 32],
    /// BLAKE3 hash of the bytes as stored on disk (post-compression, post-encryption).
    /// All-zero is the sentinel value meaning "not computed" (v5 entries imported into
    /// a v6 build context).
    pub stored_checksum: [u8; 32],
    pub original_size: u64,
    pub compressed_size: u64,
    pub path_length: u32,
    pub extra_length: u32,
    /// Byte length of the xattr blob following the extra string in the index tail.
    /// Zero when no extended attributes are stored (all current writes).
    pub xattr_length: u32,
    /// Which volume holds this entry's data block; 0 for single-file archives.
    pub volume_number: u16,
}

unsafe impl Pod for ArchiveIndexEntryV6 {}
unsafe impl Zeroable for ArchiveIndexEntryV6 {}

impl ArchiveIndexEntryV6 {
    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(bytemuck::bytes_of(self))
    }
}

// ---------------------------------------------------------------------------
// Compression method enum (shared by v5 and v6)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// v5 index entry (on-disk, 85 bytes)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// In-memory wrapper (version-agnostic; v6-only fields zero for v5 entries)
// ---------------------------------------------------------------------------

pub struct ArchiveIndexEntryWrapper {
    pub entry: ArchiveIndexEntry,
    pub path: String,
    pub extra: String,
    pub xattrs: Vec<crate::xattrs::XattrPair>,
    // v6-only in-memory fields.  Zero-initialized for v5 entries.
    // An all-zero `stored_checksum` is the sentinel meaning "not computed / not present".
    pub stored_checksum: [u8; 32],
    pub xattr_length: u32,
    pub volume_number: u16,
}

impl ArchiveIndexEntryWrapper {
    /// Construct a wrapper with v6-only fields set to their default (zero) values.
    ///
    /// Use this for v5 archives and when building entries before Phase 1 v6 paths
    /// are exercised.
    pub fn new(entry: ArchiveIndexEntry, path: String, extra: String) -> Self {
        Self {
            entry,
            path,
            extra,
            xattrs: Vec::new(),
            stored_checksum: [0u8; 32],
            xattr_length: 0,
            volume_number: 0,
        }
    }

    /// Construct a wrapper with all v6-only fields explicitly set.
    ///
    /// Used when building or reading v6 archives.
    pub fn new_v6(
        entry: ArchiveIndexEntry,
        path: String,
        extra: String,
        xattrs: Vec<crate::xattrs::XattrPair>,
        stored_checksum: [u8; 32],
        xattr_length: u32,
        volume_number: u16,
    ) -> Self {
        Self {
            entry,
            path,
            extra,
            xattrs,
            stored_checksum,
            xattr_length,
            volume_number,
        }
    }

    /// Returns the BLAKE3 hash of the bytes stored on disk (post-compression,
    /// post-encryption) for v6 entries.
    ///
    /// Returns `None` for v5 entries, which are identified by an all-zero
    /// `stored_checksum` field (a real all-zero hash is astronomically
    /// improbable and is treated as the sentinel "not present" value).
    pub fn stored_checksum_v6(&self) -> Option<&[u8; 32]> {
        if self.stored_checksum == [0u8; 32] {
            None
        } else {
            Some(&self.stored_checksum)
        }
    }
}
