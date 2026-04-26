# DAR Archive Format (v6)

This page documents the current v6 `.dar` layout and the related `.dari` external
index file.

## Primary Archive Layout

```text
ArchiveHeaderV6 (17 bytes)
Data blocks
Embedded v6 index
ArchiveFooterV6 (19 bytes)
```

## Archive Header

| Offset | Size | Field |
|--------|------|-------|
| 0 | 4 | `DARI` signature |
| 4 | 1 | version byte = `6` |
| 5 | 8 | archive creation timestamp (`u64`, LE) |
| 13 | 2 | `volume_number` (`u16`, LE) |
| 15 | 2 | `total_volumes` (`u16`, LE) |

## Archive Footer

| Offset | Size | Field |
|--------|------|-------|
| 0 | 7 | `DARIEND` signature |
| 7 | 8 | `index_offset` (`u64`, LE) |
| 15 | 4 | `amount_of_files` (`u32`, LE) |

## v6 Index Entry

Each embedded or external v6 entry begins with a 123-byte fixed header followed by:

```text
path[path_length]
extra[extra_length]
xattrs[xattr_length]
```

Fixed fields:

| Field | Type |
|-------|------|
| `offset` | `u64` |
| `bitflags` | `u16` |
| `compression_method` | `u8` |
| `modification_timestamp` | `u64` |
| `uid` | `u32` |
| `gid` | `u32` |
| `perm` | `u16` |
| `checksum` | `[u8; 32]` |
| `stored_checksum` | `[u8; 32]` |
| `original_size` | `u64` |
| `compressed_size` | `u64` |
| `path_length` | `u32` |
| `extra_length` | `u32` |
| `xattr_length` | `u32` |
| `volume_number` | `u16` |

## v6-Specific Fields

- `stored_checksum`: BLAKE3 of the bytes exactly as stored on disk after
  compression and optional encryption.
- `xattr_length`: byte length of the serialized xattr blob.
- `volume_number`: which archive volume contains the referenced data block.

## External `.dari` Index

The external index stores the same v6 entry layout outside the main archive.

### Header

| Offset | Size | Field |
|--------|------|-------|
| 0 | 6 | `DARIDX` signature |
| 6 | 1 | index version byte = `1` |
| 7 | 8 | archive timestamp (`u64`, LE) |
| 15 | 2 | `total_volumes` (`u16`, LE) |

### Body

- zero or more v6 index entries
- one snapshot marker byte (`0x01`)
- snapshot records used by incremental workflows

Each snapshot record is:

```text
path_length:u32
path:path_length bytes
checksum:[u8; 32]
modification_timestamp:u64
```

### Footer

| Offset | Size | Field |
|--------|------|-------|
| 0 | 9 | `DARIDXEND` signature |
| 9 | 4 | `entry_count` (`u32`, LE) |
| 13 | 32 | BLAKE3 checksum of all prior bytes in the `.dari` file |

## `.b3` Sidecars

Each archive volume may have a sibling `.b3` file containing the BLAKE3 hash of the
entire volume. `dari verify` treats this as layer 1 validation.
