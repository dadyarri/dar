# DAR Archive Format — Implementation Reference (v5)

This document is a complete, self-contained reference for the `.dar` binary file format.
It contains everything required to write an independent reader or writer from scratch in any
language.

---

## 1. Primitive Types and Byte Order

| Notation    | Width   | Encoding                                   |
|-------------|---------|--------------------------------------------|
| `u8`        | 1 byte  | unsigned byte                              |
| `u16`       | 2 bytes | unsigned, **little-endian**                |
| `u32`       | 4 bytes | unsigned, **little-endian**                |
| `u64`       | 8 bytes | unsigned, **little-endian**                |
| `bytes[N]`  | N bytes | raw byte array, stored verbatim            |
| `string[N]` | N bytes | UTF-8 encoded text, **not** NUL-terminated |

All timestamps are Unix epoch values (seconds since 1970-01-01 00:00:00 UTC) stored as `u64`.

All multi-byte integers are little-endian. There is **no padding or alignment** between fields
(equivalent to C `__attribute__((packed))`).

---

## 2. High-Level File Layout

```
┌─────────────────────────────────────────────────────┐
│  HEADER   (13 bytes, always at offset 0)            │
├─────────────────────────────────────────────────────┤
│  DATA BLOCKS                                        │
│  ┌────────────────────────────────────────────────┐ │
│  │  File 0 data  (compressed / encrypted bytes)   │ │
│  ├────────────────────────────────────────────────┤ │
│  │  File 1 data                                   │ │
│  ├────────────────────────────────────────────────┤ │
│  │  …                                             │ │
│  └────────────────────────────────────────────────┘ │
│  (deduplicated files share one data block)          │
├─────────────────────────────────────────────────────┤
│  INDEX  (starts at offset stored in footer)         │
│  ┌────────────────────────────────────────────────┐ │
│  │  IndexEntry[0]  (85-byte struct + path + extra)│ │
│  ├────────────────────────────────────────────────┤ │
│  │  IndexEntry[1]                                 │ │
│  ├────────────────────────────────────────────────┤ │
│  │  …                                             │ │
│  └────────────────────────────────────────────────┘ │
├─────────────────────────────────────────────────────┤
│  FOOTER  (15 bytes, always at end of file)          │
└─────────────────────────────────────────────────────┘
```

The footer is **always** the last 15 bytes of the file. Its `index_offset` field is an absolute
byte offset from the beginning of the file pointing at the first `IndexEntry`.

---

## 3. HEADER (13 bytes)

Located at absolute offset **0**.

| Offset | Size | Type       | Field       | Value / Description                         |
|--------|------|------------|-------------|---------------------------------------------|
| 0      | 4    | `bytes[4]` | `signature` | Always `DARI` = `0x44 0x41 0x52 0x49`       |
| 4      | 1    | `u8`       | `version`   | Always `5` for this format version          |
| 5      | 8    | `u64`      | `timestamp` | Archive creation time (Unix epoch, seconds) |

**Total: 13 bytes.**

### Validation

A reader **must** reject the file if:

- `signature` ≠ `DARI`
- `version` ≠ `5`

---

## 4. DATA BLOCKS

Data blocks begin immediately after the header at absolute offset **13** and extend up to (but
not including) `index_offset`.

Each block contains the stored bytes for exactly one logical file. The size of a block is
`compressed_size` bytes as recorded in the corresponding index entry (see §6).

A block may contain:

1. **Raw bytes** (no compression, no encryption) — identical to the original file content.
2. **Compressed bytes** — output of one of the algorithms described in §8.
3. **Compressed-then-encrypted bytes** — cipher output as described in §9.
4. **Encrypted-only bytes** — when the compressor did not reduce size, the original bytes are
   encrypted directly.

The actual interpretation is determined by the `compression_method` and `bitflags` fields of
the corresponding index entry.

### Deduplication

When two or more archived files have identical content (same BLAKE3 checksum), only **one** data
block is written. The first file's index entry is the *primary*; subsequent files' index entries
set the `INDEX_FLAG_LINKED_DATA` bit (see §6) and point their `offset` field at the same data
block. No additional data bytes are stored for linked entries.

---

## 5. FOOTER (15 bytes)

Located at absolute offset `file_length − 15`.

| Offset | Size | Type       | Field             | Value / Description                                        |
|--------|------|------------|-------------------|------------------------------------------------------------|
| 0      | 7    | `bytes[7]` | `signature`       | Always `DARIEND` = `0x44 0x41 0x52 0x49 0x45 0x4E 0x44`    |
| 7      | 4    | `u32`      | `index_offset`    | Absolute byte offset of the first `IndexEntry` in the file |
| 11     | 4    | `u32`      | `amount_of_files` | Number of entries in the index                             |

**Total: 15 bytes.**

### Validation

A reader **must** reject the file if:

- The file is shorter than `13 + 15 = 28` bytes.
- `signature` ≠ `DARIEND`.
- `index_offset` < `13` (header size).
- `index_offset` > `file_length − 15` (would overlap the footer or lie outside the file).

---

## 6. INDEX

The index begins at absolute offset `index_offset` and contains exactly `amount_of_files`
entries written sequentially with no gaps or separators.

Each entry consists of a **fixed-size 85-byte struct** followed immediately by a **variable-length
path string** and a **variable-length extra string**.

### 6.1 Fixed-Size Struct (85 bytes)

| Offset | Size | Type        | Field                    | Description                                                                                                                                                                                                                                                            |
|--------|------|-------------|--------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| 0      | 8    | `u64`       | `offset`                 | Absolute byte offset in the archive of this entry's data block. For linked entries (dedup) this points to the **primary** entry's data block.                                                                                                                          |
| 8      | 2    | `u16`       | `bitflags`               | Bit-field; see §6.2                                                                                                                                                                                                                                                    |
| 10     | 1    | `u8`        | `compression_method`     | Compression algorithm; see §6.3                                                                                                                                                                                                                                        |
| 11     | 8    | `u64`       | `modification_timestamp` | File last-modified time (Unix epoch, seconds)                                                                                                                                                                                                                          |
| 19     | 4    | `u32`       | `uid`                    | Unix user ID of file owner (always `1000` on non-Unix)                                                                                                                                                                                                                 |
| 23     | 4    | `u32`       | `gid`                    | Unix group ID of file owner (always `1000` on non-Unix)                                                                                                                                                                                                                |
| 27     | 2    | `u16`       | `perm`                   | Unix permission bits (always `644` on non-Unix)                                                                                                                                                                                                                        |
| 29     | 32   | `bytes[32]` | `checksum`               | BLAKE3 hash of the **original** (uncompressed, unencrypted) file content                                                                                                                                                                                               |
| 61     | 8    | `u64`       | `original_size`          | File size in bytes **before** compression and encryption                                                                                                                                                                                                               |
| 69     | 8    | `u64`       | `compressed_size`        | Byte length of the stored data block (after compression and/or encryption). When `compression_method = None` **and** the entry is not encrypted, `compressed_size = original_size`. When encrypted, the 16-byte authentication tag is included in this count (see §9). |
| 77     | 4    | `u32`       | `path_length`            | Byte length of the UTF-8 path string that follows                                                                                                                                                                                                                      |
| 81     | 4    | `u32`       | `extra_length`           | Byte length of the UTF-8 extra string that follows                                                                                                                                                                                                                     |

**Total fixed part: 85 bytes.**

### 6.2 Bitflags

| Bit mask | Constant name               | Meaning                                                                                                                                                                                                                                                                      |
|----------|-----------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `0x0001` | `INDEX_FLAG_LINKED_DATA`    | This entry is a dedup reference. Its `offset` points to a primary entry's data block; no independent data block exists for this entry. The extractor must look up the primary entry by matching `checksum` across all entries where `INDEX_FLAG_LINKED_DATA` is **not** set. |
| `0x0002` | `INDEX_FLAG_ENCRYPTED_DATA` | The data block is encrypted with ChaCha20-Poly1305. See §9.                                                                                                                                                                                                                  |

All other bits are reserved and must be zero.

### 6.3 Compression Method

| Value | Name         | Algorithm                                   |
|-------|--------------|---------------------------------------------|
| `0`   | `None`       | No compression; raw bytes stored            |
| `1`   | `Brotli`     | RFC 7932 Brotli stream                      |
| `2`   | `Zstandard`  | Zstandard frame (RFC 8878 / zstd)           |
| `3`   | `Lzma`       | XZ container wrapping LZMA2                 |
| `4`   | `LeptonJpeg` | Mozilla Lepton lossless JPEG re-compression |

### 6.4 Variable Part

Immediately after the 85-byte fixed struct:

```
[path_length bytes: UTF-8 path string]
[extra_length bytes: UTF-8 extra string]
```

- The **path** is a relative, forward-slash-separated archive-internal path (e.g.
  `src/main.rs`). It has no leading slash and no trailing slash. It never contains `..`.
- The **extra** field is a semicolon-delimited list of `key=value` pairs. See §7.
- If `extra_length = 0`, there is no extra string (no bytes, not even a NUL).

### 6.5 Reading the Index Sequentially

```
seek to index_offset
for i in 0 .. amount_of_files:
    read 85 bytes → fixed struct
    read path_length bytes → path
    read extra_length bytes → extra
```

---

## 7. EXTRA Field Format

The extra string is a UTF-8 text field attached to each index entry. It encodes zero or more
metadata key/value pairs.

### Syntax

```
key1=value1;key2=value2;key3=value3
```

- Pairs are separated by **`;`** (semicolon U+003B).
- Each pair is `key=value` split on the **first** `=` (U+003D).
- Whitespace is **not** trimmed; keys and values are stored verbatim.
- An empty extra string means no pairs.

### Encoding Rules

| Rule                    | Description                                                                                                                                                     |
|-------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Semicolon escaping**  | Any literal `;` inside a key or value is replaced by `%3B` before serialisation and must be restored to `;` when parsing. No other percent-encoding is applied. |
| **Empty pairs skipped** | Pairs where the key or value is empty are not written to the archive.                                                                                           |
| **Last-write wins**     | If the same key is set multiple times during archive creation, only the final value is retained.                                                                |

### Parsing Algorithm

```python
def parse_extra(s):
    pairs = {}
    for segment in s.split(';'):
        if '=' not in segment:
            continue
        key, _, value = segment.partition('=')
        if key and value:
            pairs[key] = value.replace('%3B', ';')   # unescape after splitting
    return pairs
```

> **Note:** Split on `;` first, then unescape `%3B` in individual tokens. Unescaping before
> splitting would mis-split escaped semicolons.

### Known Keys

#### Encryption metadata (set automatically when `--encrypt` is used)

| Key  | Type       | Description                                                                                                                                      |
|------|------------|--------------------------------------------------------------------------------------------------------------------------------------------------|
| `e`  | string     | Encryption algorithm name. Currently always `chacha20poly1305`.                                                                                  |
| `en` | hex string | 12-byte nonce, lowercase hex (24 hex characters). Informational — the nonce is also derived deterministically from the checksum (see §9).        |
| `et` | hex string | 16-byte Poly1305 authentication tag, lowercase hex (32 hex characters). Informational — the tag is appended to the ciphertext in the data block. |

#### EXIF metadata (populated from image EXIF when available)

| Key   | EXIF Tag           | Description                                      |
|-------|--------------------|--------------------------------------------------|
| `imk` | `Make`             | Camera manufacturer                              |
| `imd` | `Model`            | Camera model                                     |
| `idt` | `DateTimeOriginal` | Original capture timestamp (EXIF display format) |

#### Audio metadata (populated from ID3/Vorbis tags when available)

| Key   | Tag field | Description |
|-------|-----------|-------------|
| `atl` | `Title`   | Track title |
| `aar` | `Artist`  | Artist name |
| `aal` | `Album`   | Album name  |
| `agn` | `Genre`   | Genre       |

---

## 8. Compression

### 8.1 Compressor Selection by File Extension

The compressor is selected at archive-creation time based on the lowercase file extension.
Extensions are matched in the priority order shown below. Unknown extensions default to
**Zstandard**.

**`None` (store raw):**

```
jpg  jpeg  png  webp  gif
mp4  mp3   aac  ogg   flac  wav  mkv  avi  mov  m4a  m4v
zip  gz    rar  7z    bz2   zst  tar  bz   xz   lzma lz4  lz  zlib
jar  war   ear  apk   ipa   aab
whl  egg   nupkg  gem
pdf  docx  xlsx  pptx  odt  ods  odp  epub  cbz
wasm
```

**`Brotli`:**

```
html  htm  xhtml  css  scss  sass  less  stylus
js  mjs  jsx  ts  tsx  mts
json  svg  xml  xsl  xsd
txt  md  markdown  rst  toml  yaml  yml
woff2
```

**`Zstandard` (also the default for any unlisted extension):**

```
log  csv  tsv  db  sql  bak  tar
rs  go  java  kt  py  rb  php  pl  pas
c  cpp  c++  h  hpp  cs  fs  vb  vba
sh  bat  ps1  fish
proto  thrift
```

**`LZMA`:**

```
iso  img  bin  deb  rpm  pkg  vmdk  patch  diff
fortran  f90  ada  lisp  scm  hs  erl
cmake  makefile  mk  tex  bib
```

**Optional image-specific compressors** (enabled only with `--compress-images` flag):

| Extension     | Compressor   | Notes                                                                                                                           |
|---------------|--------------|---------------------------------------------------------------------------------------------------------------------------------|
| `png`         | `PngOxipng`  | Lossless PNG re-optimisation via oxipng; result always stored with `compression_method = None` (PNG is its own container).      |
| `jpg`, `jpeg` | `JpegLepton` | Lossless JPEG re-compression via Lepton; uses `compression_method = LeptonJpeg` only if Lepton output is smaller than original. |

### 8.2 Compression Fallback Rule

**If the compressor output is ≥ the original file size, the original bytes are stored
instead, and `compression_method` is set to `None`.**

This means a reader must treat `compression_method = None` strictly as "stored bytes are the
raw original". Any other `compression_method` value guarantees the stored bytes are the output
of that algorithm.

### 8.3 Algorithm Parameters

| Algorithm  | Library / Standard           | Parameters Used                          |
|------------|------------------------------|------------------------------------------|
| Brotli     | RFC 7932 (`brotli` crate)    | quality = 6, lgwin = 22                  |
| Zstandard  | RFC 8878 (`zstd` crate)      | compression level = 3                    |
| LZMA       | XZ file format (`xz2` crate) | preset level = 9 (XZ container, LZMA2)   |
| LeptonJpeg | Mozilla Lepton               | `compat_lepton_vector_write` feature set |

All these formats use standard, interoperable stream formats. Any compliant library for the
respective algorithm can decompress the stored bytes.

---

## 9. Encryption

Encryption is optional and applied **after** compression. It uses **ChaCha20-Poly1305 AEAD**
(RFC 8439).

### 9.1 When Encryption Is Active

- Bit `0x0002` (`INDEX_FLAG_ENCRYPTED_DATA`) is set in the entry's `bitflags`.
- The extra field contains `e=chacha20poly1305` (and the informational `en` / `et` keys).

### 9.2 Key Derivation

The 32-byte encryption key is derived from the passphrase using **BLAKE3 key derivation** with
a fixed context string:

```
key = blake3_derive_key("dari.v1.chacha20poly1305.key", passphrase_utf8_bytes)
```

`blake3_derive_key` is the standard BLAKE3 KDF (see the BLAKE3 specification). The result is
32 bytes suitable as a ChaCha20-Poly1305 key.

The **same key** is used for all entries in an archive (the passphrase is per-archive).

### 9.3 Nonce Derivation

The 12-byte nonce is taken from the **first 12 bytes** of the entry's BLAKE3 checksum:

```
nonce[0..12] = entry.checksum[0..12]
```

This means the nonce is deterministic per unique file content. Deduplicated (linked) entries
share the nonce with the primary entry — which is correct, because they also reference the same
ciphertext block.

### 9.4 Encryption Process (writing)

```
plaintext  = compressed_bytes  (or original_bytes if compression was skipped/not beneficial)
nonce      = entry.checksum[0..12]
key        = blake3_derive_key("dari.v1.chacha20poly1305.key", passphrase_utf8)
cipher     = ChaCha20Poly1305(key)
ciphertext, tag = cipher.encrypt_detached(nonce, aad=b"", plaintext)
stored_bytes    = ciphertext || tag        # 16-byte Poly1305 tag appended
compressed_size = len(ciphertext) + 16
```

The 16-byte Poly1305 **authentication tag is appended** to the end of the ciphertext.
Associated data (`aad`) is always an **empty byte string**.

### 9.5 Decryption Process (reading)

```
raw = read compressed_size bytes starting at entry.offset
if len(raw) < 16:
    raise Error("data block too short to contain auth tag")
ciphertext = raw[0 : len(raw) - 16]
tag        = raw[len(raw) - 16 :]          # last 16 bytes
nonce      = entry.checksum[0..12]
key        = blake3_derive_key("dari.v1.chacha20poly1305.key", passphrase_utf8)
plaintext  = ChaCha20Poly1305(key).decrypt_detached(nonce, aad=b"", ciphertext, tag)
# then decompress plaintext according to compression_method
```

If the Poly1305 tag does not verify, the passphrase is wrong or the data is corrupt.

---

## 10. Struct Byte-Layout Quick Reference

### HEADER  (offset 0, total 13 bytes)

```
Byte  0    1    2    3    4         5  6  7  8  9  10 11 12
     ┌────┬────┬────┬────┬─────────┬────────────────────────┐
     │'D' │'A' │'R' │'I' │version=5│   timestamp (u64 LE)   │
     └────┴────┴────┴────┴─────────┴────────────────────────┘
```

### FOOTER  (last 15 bytes of file, total 15 bytes)

```
Byte  0    1    2    3    4    5    6    7-10            11-14
     ┌────┬────┬────┬────┬────┬────┬────┬──────────────┬──────────────┐
     │'D' │'A' │'R' │'I' │'E' │'N' │'D' │index_offset  │num_files     │
     │    │    │    │    │    │    │    │   (u32 LE)   │  (u32 LE)    │
     └────┴────┴────┴────┴────┴────┴────┴──────────────┴──────────────┘
```

### INDEX ENTRY FIXED PART  (total 85 bytes)

```
Bytes   0- 7   offset                (u64 LE)   — abs. data block offset
Bytes   8- 9   bitflags              (u16 LE)   — 0x0001=linked, 0x0002=encrypted
Byte   10      compression_method    (u8)       — 0=None,1=Brotli,2=Zstd,3=LZMA,4=Lepton
Bytes  11-18   modification_timestamp(u64 LE)   — Unix mtime
Bytes  19-22   uid                   (u32 LE)
Bytes  23-26   gid                   (u32 LE)
Bytes  27-28   perm                  (u16 LE)
Bytes  29-60   checksum              (bytes[32]) — BLAKE3 of original content
Bytes  61-68   original_size         (u64 LE)   — size before compression
Bytes  69-76   compressed_size       (u64 LE)   — size of stored data block
Bytes  77-80   path_length           (u32 LE)   — byte length of path string
Bytes  81-84   extra_length          (u32 LE)   — byte length of extra string
Bytes  85 ..   path                  (UTF-8, path_length bytes)
Bytes  85+path_length .. extra       (UTF-8, extra_length bytes)
```

---

## 11. Checksums and Integrity

- The **BLAKE3** checksum stored in each index entry is computed over the **original,
  uncompressed, unencrypted** file content.
- After decrypting and decompressing a data block during extraction, implementations
  **should** verify that `blake3(plaintext) == entry.checksum` to detect corruption.
- There is no archive-level checksum; integrity is verified per-entry only.

---

## 12. Extra Field Building (creation-time reference)

During archive creation the following extra pairs are set automatically:

1. **Encryption info** (when passphrase is set):
   ```
   e=chacha20poly1305;en=<nonce_hex>;et=<tag_hex>
   ```
   `<nonce_hex>` is `checksum[0..12]` as 24 lowercase hex characters.  
   `<tag_hex>` is the 16-byte Poly1305 tag as 32 lowercase hex characters.

2. **EXIF info** (when the file contains readable EXIF data):
   Keys `imk`, `imd`, `idt` are set to the string representation of the respective EXIF fields.

3. **Audio tags** (when the file contains readable ID3/Vorbis/etc. tags):
   Keys `atl`, `aar`, `aal`, `agn` are set.

Implementations that do not need to write metadata can output an empty extra string
(`extra_length = 0`).

---

## 13. Format Constants Summary

| Constant                     | Value                                                    |
|------------------------------|----------------------------------------------------------|
| Header magic                 | `DARI` = `[0x44, 0x41, 0x52, 0x49]`                      |
| Footer magic                 | `DARIEND` = `[0x44, 0x41, 0x52, 0x49, 0x45, 0x4E, 0x44]` |
| Format version               | `5`                                                      |
| Header size                  | `13` bytes                                               |
| Footer size                  | `15` bytes                                               |
| Index entry fixed size       | `85` bytes                                               |
| `INDEX_FLAG_LINKED_DATA`     | `0x0001`                                                 |
| `INDEX_FLAG_ENCRYPTED_DATA`  | `0x0002`                                                 |
| Encryption algorithm         | ChaCha20-Poly1305 AEAD (RFC 8439)                        |
| KDF context string           | `"dari.v1.chacha20poly1305.key"`                         |
| KDF algorithm                | BLAKE3 `derive_key`                                      |
| Nonce source                 | `checksum[0..12]` (first 12 bytes of BLAKE3 hash)        |
| Auth tag size                | `16` bytes, appended to the end of the ciphertext        |
| Checksum algorithm           | BLAKE3 (256-bit / 32-byte output)                        |
| Semicolon escape in extra    | `%3B`                                                    |
| Non-Unix uid/gid placeholder | `1000`                                                   |
| Non-Unix perm placeholder    | `644`                                                    |
