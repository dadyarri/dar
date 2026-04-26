# DAR Archive Format (v5)

This page documents the legacy v5 `.dar` format.

## Layout

```text
ArchiveHeader (13 bytes)
Data blocks
Index entries (embedded)
ArchiveFooter (15 bytes)
```

## Header

| Offset | Size | Field |
|--------|------|-------|
| 0 | 4 | `DARI` signature |
| 4 | 1 | version byte = `5` |
| 5 | 8 | archive creation timestamp (`u64`, LE) |

## Footer

| Offset | Size | Field |
|--------|------|-------|
| 0 | 7 | `DARIEND` signature |
| 7 | 4 | `index_offset` (`u32`, LE) |
| 11 | 4 | `amount_of_files` (`u32`, LE) |

## Index Entry

Each v5 index entry has an 85-byte fixed header followed by:

```text
path[path_length]
extra[extra_length]
```

The fixed header fields are:

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
| `original_size` | `u64` |
| `compressed_size` | `u64` |
| `path_length` | `u32` |
| `extra_length` | `u32` |

## Flags

| Mask | Meaning |
|------|---------|
| `0x0001` | Deduplicated linked entry |
| `0x0002` | Encrypted payload |

## Compression Methods

| Value | Meaning |
|-------|---------|
| `0` | None |
| `1` | Brotli |
| `2` | Zstandard |
| `3` | LZMA |
