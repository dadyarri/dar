# dar {WIP}

CLI utility to compress files and folders respecting .gitignore and automatically choosing compression algorithm based on file's type

- [x] Create: -cf <FILE> v (verbose) p (progress) h (help) --exclude <PATTERN> --exclude-from <FILE> <CONTENT>
- [x] Extract: -xf <FILE> v (verbose) p (progress) h (help) -C <output dir>
- [x] List: -tf <FILE> h (help)
- [ ] Append (new files to existing archive): -rf <FILE> <DIRECTORY/FILE> -v (verbose)
- [ ] Defragment (remove old indexes): -df <FILE>
- [x] Validate (check if existing archive is valid): -Vf <FILE> -v (verbose) -s (slow, validating CRC of all files)


ARCHIVE FORMAT (v0004)
======================
Designed for efficient random access parsing, validation, and listing.

STRUCTURE:
[HEADER: 512 bytes fixed size]
  Magic (4 bytes):                "DAR\0"
  Version (4 bytes):              0004 (padded)
  Data Section Start (8 bytes):   u64 big-endian offset
  Index Section Start (8 bytes):  u64 big-endian offset
  Total Files (4 bytes):          u32 big-endian count
  Created Timestamp (8 bytes):    u64 big-endian UNIX time
  Archive Checksum (32 bytes):    BLAKE3 of entire archive (computed last)
  Flags (1 byte):                 reserved bits
  [Padding: remaining to 512 bytes]

[DATA SECTION]
  For each file entry:
    Entry Length (8 bytes):       u64 big-endian (excludes this 8-byte field)
    Compressed Data:              [entry_length bytes]

[INDEX SECTION]
  Entry Count (4 bytes):          u32 big-endian count (matches header total_files)
  For each index entry:
    Entry Length (4 bytes):       u32 big-endian (excludes this 4-byte field)
    Path Length (4 bytes):        u32 big-endian
    Path:                         [path_length UTF-8 bytes]
    Data Offset (8 bytes):        u64 big-endian offset into data section
    Uncompressed Size (8 bytes):  u64 big-endian
    Compressed Size (8 bytes):    u64 big-endian
    Compression Algo (1 byte):    0=None, 1=Brotli, 2=Zstandard, 3=LZMA2
    Modification Time (8 bytes):  u64 big-endian UNIX timestamp
    UID (1 byte):                 user ID (Unix only)
    GID (1 byte):                 group ID (Unix only)
    Permissions (2 bytes):        u16 big-endian mode bits
    Checksum (32 bytes):          BLAKE3 of uncompressed data

[END RECORD: 64 bytes fixed size]
  Magic (4 bytes):                "DEND"
  Index Offset (8 bytes):         u64 big-endian
  Index Length (8 bytes):         u64 big-endian
  Archive Checksum (32 bytes):    BLAKE3 of entire archive
  Flags (1 byte):                 reserved
  [Padding: 11 bytes]

ADVANTAGES:
- Fixed header/end records enable quick seeks
- Length-prefixed entries allow safe skipping
- Index completely separate from data for parallel access
- End record checksum enables integrity verification
- Structured format makes validation straightforward
