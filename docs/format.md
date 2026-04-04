# Binary Format Reference

This document describes the on-disk layout of `.dar` archives and provides
guidelines for extending the format in future versions.

---

## Current Format: Version 5

All on-disk structs live in `src/models/archive.rs`.  They are `#[repr(C, packed)]`
and implement `bytemuck::Pod + Zeroable` for zero-copy serialisation.
All multi-byte integers are **little-endian**.

### Archive Layout

```
┌──────────────────────────────────┐
│  Header  (13 bytes)              │
│  "DARI" (4 B) + version (1 B)    │
│  + creation timestamp (8 B)      │
├──────────────────────────────────┤
│  File data blocks                │
│  (raw / compressed / encrypted   │
│   bytes, one block per entry)    │
├──────────────────────────────────┤
│  Index entries                   │
│  (one ArchiveIndexEntry per file │
│   + variable-length path string  │
│   + variable-length extra string)│
├──────────────────────────────────┤
│  Footer  (15 bytes)              │
│  "DARIEND" (7 B)                 │
│  + index_offset (4 B u32 LE)     │
│  + file_count   (4 B u32 LE)     │
└──────────────────────────────────┘
```

### Header (`ArchiveHeader`, 13 bytes)

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | `signature` | Magic bytes `DARI` (ASCII) |
| 4 | 1 | `version` | Format version byte (`5` for current) |
| 5 | 8 | `timestamp` | Archive creation time as Unix seconds (u64 LE) |

### Footer (`ArchiveFooter`, 15 bytes)

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 7 | `signature` | Magic bytes `DARIEND` (ASCII) |
| 7 | 4 | `index_offset` | Byte offset of the first index entry (u32 LE) |
| 11 | 4 | `amount_of_files` | Total number of index entries (u32 LE) |

### Index Entry (`ArchiveIndexEntry`, fixed header)

Each entry consists of a fixed-size header followed by two variable-length
UTF-8 strings (path and extra).

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 8 | `offset` | Byte offset of the file's data block (u64 LE) |
| 8 | 2 | `bitflags` | Feature flags; see table below |
| 10 | 1 | `compression_method` | Compression codec; see table below |
| 11 | 8 | `timestamp` | File modification time as Unix seconds (u64 LE) |
| 19 | 4 | `uid` | Owner user ID (u32 LE); `1000` on Windows |
| 23 | 4 | `gid` | Owner group ID (u32 LE); `1000` on Windows |
| 27 | 2 | `perm` | Unix permission bits (u16 LE); `644` on Windows |
| 29 | 32 | `checksum` | BLAKE3 checksum of the **original** file bytes |
| 61 | 8 | `original_size` | Uncompressed file size in bytes (u64 LE) |
| 69 | 8 | `compressed_size` | Stored (compressed / encrypted) size in bytes (u64 LE) |
| 77 | 4 | `path_length` | Byte length of the path string that follows (u32 LE) |
| 81 | 4 | `extra_length` | Byte length of the extra string that follows (u32 LE) |
| 85 | N | `path` | UTF-8 archive-relative path (`path_length` bytes) |
| 85+N | M | `extra` | UTF-8 key=value metadata string (`extra_length` bytes) |

#### `compression_method` values

| Value | Codec |
|-------|-------|
| `0` | None (stored as-is) |
| `1` | Brotli (quality 6) |
| `2` | Zstandard (level 3) |
| `3` | LZMA / XZ (level 9) |

#### `bitflags` bit positions

| Bit | Constant | Meaning |
|-----|----------|---------|
| 0 | `LINKED_DATA` | Entry is a dedup link; its `offset` points to a prior entry's data block |
| 1 | `ENCRYPTED_DATA` | Entry data is ChaCha20-Poly1305 encrypted |

### Extra Field Format

The `extra` string is a semicolon-delimited list of `key=value` pairs, for example:

```
aar=The Beatles;aal=Abbey Road;iw=1920;ih=1080
```

Known key prefixes are documented in `src/constants.rs` and `src/tui/preview.rs`
(`KNOWN_TAGS`).  Unknown keys are preserved and displayed verbatim in the TUI.

---

## Version History

| Version | Notes |
|---------|-------|
| 1 | Initial release |
| 2 | Added `extra` field to index entries |
| 3 | Added deduplication (`LINKED_DATA` bitflag) |
| 4 | Added encryption (`ENCRYPTED_DATA` bitflag + nonce/tag in `extra`) |
| 5 | Added `uid`, `gid`, `perm` fields; `path_length`/`extra_length` widened to u32; `modification_timestamp` widened to u64; current version |

---

## Guidelines for Version 6

Any breaking change to the binary format **must** bump the version byte in
`src/constants/format.rs` and be released as a new major version of `dari`.

Planned v6 changes (see roadmap item 7.2):

- **Chunked AEAD encryption:** split file data into 1 MB segments, each with its
  own nonce and authentication tag.  This removes the need to buffer the entire
  compressed file in RAM during encryption and enables random-access decryption.
- The `extra` field will gain `enc_chunk_count` to record how many chunks a
  large encrypted file was split into.

Backwards compatibility rules:

1. A v6 reader **must** reject v5 archives with an `UnsupportedVersion` error
   (or offer a migration path).
2. A v5 reader **must** reject v6 archives by checking the version byte in the
   header.
3. New bitflag bits **must** be added in a minor version bump only when the
   reader can safely ignore unknown bits (i.e. no behavioural change is implied).
