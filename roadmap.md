# dari — Refactor & Enhancement Roadmap

This document is a living technical plan produced after a full codebase audit.
Every item is categorised by **area**, labelled with a **priority** (`P1`–`P3`),
and accompanied by a clear rationale. Items within the same priority may be
worked in any order unless a dependency is noted.

Items marked ✅ have been implemented.

---

## Format v6 — Full Migration Plan

### Overview

Format v5 has several hard limits that cannot be resolved without a breaking
binary change:

- `ArchiveFooter::index_offset` is `u32` — archives larger than ~4 GB silently
  overflow this field, producing an unreadable archive.
- Per-entry `checksum` covers original file content only; bit-rot in the
  compressed/encrypted bytes sitting on disk is undetected until full
  decompression.
- No volume-awareness — split archives, external indexes, and per-volume
  integrity sidecar files are structurally impossible.
- Encryption requires buffering the entire compressed file in RAM before the
  AEAD tag can be appended (roadmap item 7.2).
- No xattr, ACL, SELinux, or hard-link data is stored.

The v6 migration is divided into **eight phases** that must be executed in the
order listed; each phase is a self-contained, reviewable PR boundary. Phases
0–1 are purely infrastructural and unlock all subsequent phases. The keyword
**"v6 flag"** refers to the `--format-version 6` CLI flag introduced in Phase 0.

---

### Phase 0 — Multi-version dispatch infrastructure ✅

**Goal:** Eliminate all hard-coded `VERSION = 5` assumptions and introduce a
clean dispatch layer so that subsequent phases can add v6 code paths without
touching existing v5 logic.

#### 0.1 — New file: `src/format_version.rs`

Introduce a `FormatVersion` enum parsed from the single version byte at header
offset 4:

```rust
pub enum FormatVersion { V5 = 5, V6 = 6 }

impl TryFrom<u8> for FormatVersion { … }   // returns DariError::UnsupportedVersion
impl From<FormatVersion> for u8 { … }
```

This file is the single source of truth for every supported version.
`src/constants.rs` keeps `format::VERSION: u8 = 5` as the default write
version and adds `format::MAX_SUPPORTED_VERSION: u8 = 6` once Phase 1 lands.

#### 0.2 — Update `DariError::UnsupportedVersion` in `src/errors.rs`

Change the variant from:

```rust
UnsupportedVersion { found: u8, expected: u8 }
```

to:

```rust
UnsupportedVersion { found: u8, max_supported: u8 }
```

Update the `Display` implementation and the one existing test accordingly.
The field rename makes the intent clearer: the archive holds `found`, the
binary supports up to `max_supported`.

#### 0.3 — Version-dispatching reader in `src/reader.rs`

`load_archive` is refactored into three functions:

1. `read_version(source) -> Result<FormatVersion>` — reads 5 bytes from offset
   0, checks the `DARI` signature, returns the `FormatVersion` or errors.
2. `load_v5(source, file_path, locale) -> Result<ArchiveState>` — contains the
   current body of `load_archive` verbatim; no behaviour change.
3. `load_archive(source, file_path, locale) -> Result<ArchiveState>` — calls
   `read_version`, then dispatches to `load_v5` or (from Phase 1) `load_v6`;
   returns `DariError::UnsupportedVersion` for unknown versions.

No behaviour changes for v5 archives; all existing tests continue to pass
without modification.

#### 0.4 — Versioned writer in `src/archive_builder.rs`

`ArchiveBuilder<W>` gains a field:

```rust
target_version: FormatVersion,   // default: FormatVersion::V5
```

Add a constructor `ArchiveBuilder::with_version(writer, config, version)`.
`write_header` and `build` branch on `self.target_version` to write the
appropriate struct layout (v5 paths are identical to today; v6 paths are added
in Phase 1).

#### 0.5 — CLI `--format-version` flag in `src/cli.rs`

Add `--format-version <5|6>` (default `5`) to the `create` and `append`
subcommands. The flag is **automatically forced to 6** when any v6-only flag
is present (`--compress-images` stays v5-compatible; `--chunked-encryption`,
`--preserve-xattrs`, `--split-size` all require v6 and set the version
implicitly, printing an info message).

New i18n keys in `locales/en.toml` and `locales/ru.toml`:

```toml
[cli.common.flags]
format_version = "target archive format version (5 or 6, default 5)"
format_version_auto = "format version forced to {v} because {reason} requires it"
```

#### Files touched in Phase 0

`src/format_version.rs` (new), `src/constants.rs`, `src/errors.rs`,
`src/reader.rs`, `src/archive_builder.rs`, `src/cli.rs`,
`locales/en.toml`, `locales/ru.toml`.

---

### Phase 1 — v6 binary format specification ✅

**Goal:** Define and implement the exact on-disk layout changes. All Phase 0
dispatch stubs call functions that `todo!()` until this phase lands.

#### 1.1 — Struct layout changes in `src/models/archive.rs`

All structs remain `#[repr(C, packed)]` with manual `unsafe impl Pod/Zeroable`.
Never derive `Pod`/`Zeroable` — always use `unsafe impl`.

**`ArchiveHeader` — 13 → 17 bytes**

| Field           | Type      | Bytes | Notes                                       |
|-----------------|-----------|-------|---------------------------------------------|
| `signature`     | `[u8; 4]` | 4     | unchanged — `b"DARI"`                       |
| `version`       | `u8`      | 1     | unchanged — `6` for v6                      |
| `timestamp`     | `u64`     | 8     | unchanged — Unix seconds                    |
| `volume_number` | `u16`     | 2     | **new** — 0-based; 0 = first or only volume |
| `total_volumes` | `u16`     | 2     | **new** — 1 for single-file archives        |

`ArchiveHeader::new()` sets `volume_number = 0` and `total_volumes = 1` by
default. `ArchiveBuilder::begin_volume(n, total)` overrides these when writing
split-archive volumes (Phase 3).

**`ArchiveFooter` — 15 → 19 bytes**

| Field             | Type      | Bytes | Notes                                             |
|-------------------|-----------|-------|---------------------------------------------------|
| `signature`       | `[u8; 7]` | 7     | unchanged — `b"DARIEND"`                          |
| `index_offset`    | `u64`     | 8     | **widened from `u32`** — removes the 4 GB ceiling |
| `amount_of_files` | `u32`     | 4     | unchanged                                         |

`ArchiveBuilder::build` already casts `stream_position()` to `u32` — this cast
is removed and `index_offset` is stored as `u64`. All callers in `reader.rs`
and `commands/append.rs` that read this field as `u32` must be updated.

**`ArchiveIndexEntry` — 85 → 123 bytes**

| Field                    | Type       | Bytes | Notes                                                                                            |
|--------------------------|------------|-------|--------------------------------------------------------------------------------------------------|
| `offset`                 | `u64`      | 8     | unchanged — byte offset within its volume                                                        |
| `bitflags`               | `u16`      | 2     | unchanged; new flag `CHUNKED_ENCRYPTION = 0x0004` added in Phase 5                               |
| `compression_method`     | `u8`       | 1     | unchanged                                                                                        |
| `modification_timestamp` | `u64`      | 8     | unchanged                                                                                        |
| `uid`                    | `u32`      | 4     | unchanged                                                                                        |
| `gid`                    | `u32`      | 4     | unchanged                                                                                        |
| `perm`                   | `u16`      | 2     | unchanged                                                                                        |
| `checksum`               | `[u8; 32]` | 32    | unchanged — BLAKE3 of **original** (pre-compression) content                                     |
| `stored_checksum`        | `[u8; 32]` | 32    | **new** — BLAKE3 of bytes **as stored on disk** (post-compression, post-encryption)              |
| `original_size`          | `u64`      | 8     | unchanged                                                                                        |
| `compressed_size`        | `u64`      | 8     | unchanged                                                                                        |
| `path_length`            | `u32`      | 4     | unchanged                                                                                        |
| `extra_length`           | `u32`      | 4     | unchanged                                                                                        |
| `xattr_length`           | `u32`      | 4     | **new** — byte length of xattr blob following the extra string in the index tail; 0 if no xattrs |
| `volume_number`          | `u16`      | 2     | **new** — which volume file holds this entry's data block; 0 for single-file archives            |

The variable-length tail of each v6 index entry is:

```
[path_bytes  : path_length ]
[extra_bytes : extra_length]
[xattr_bytes : xattr_length]   ← new in v6; absent in v5
```

#### 1.2 — `stored_checksum` computation in `src/archive_builder.rs`

In `commit_prepared`, immediately after `self.writer.write_all(bytes_to_write)`,
compute:

```rust
let stored_hash = blake3::hash(bytes_to_write);
// stored in ArchiveIndexEntry::stored_checksum
```

For deduplicated (linked) entries `stored_checksum` is copied from the primary
entry already in `dedup_index`; add `stored_checksum: [u8; 32]` to the private
`ExistingFileData` struct.

#### 1.3 — `load_v6` in `src/reader.rs`

Mirror of `load_v5` using the updated struct sizes. Key difference in the
parsing loop: after reading `extra_bytes`, an additional `xattr_bytes` read is
performed (`xattr_length` bytes). `stored_checksum` and `volume_number` are
loaded from the new fixed-size struct fields. The footer seek uses the new
19-byte footer size.

#### 1.4 — Version consistency check in `src/commands/append.rs`

`append` reads `ArchiveState::header.version`, converts to `FormatVersion`, and
passes it to `ArchiveBuilder::with_version`. Appending to a v5 archive always
writes v5; appending to a v6 archive always writes v6. Mixing is rejected:

```toml
[cli.append.errors]
version_mismatch = "cannot append: archive is format v{found}, requested format v{requested}"
```

#### Files touched in Phase 1

`src/models/archive.rs`, `src/constants.rs`, `src/archive_builder.rs`,
`src/reader.rs`, `src/commands/append.rs`, `src/extractor.rs`,
`locales/en.toml`, `locales/ru.toml`.

---

### Phase 2 — External index file (`.dari`) ✅

**Goal:** Separate the file index from the data volumes so that metadata
operations (`inspect`, `list`, selective `extract`) never need to open large
data files.

#### 2.1 — File format: `archive.dari`

The external index is a standalone binary file that lives alongside the base
archive and shares its stem: `archive.dar` → `archive.dari`,
`archive.dar.001` → `archive.dari` (one index covers all volumes in a set).

**`IndexFileHeader` — 17 bytes**

| Field               | Type      | Bytes | Notes                                                                                    |
|---------------------|-----------|-------|------------------------------------------------------------------------------------------|
| `signature`         | `[u8; 6]` | 6     | `b"DARIDX"`                                                                              |
| `version`           | `u8`      | 1     | index format version; currently `1`                                                      |
| `archive_timestamp` | `u64`     | 8     | copied from `ArchiveHeader::timestamp` of the first volume; used to detect a stale index |
| `total_volumes`     | `u16`     | 2     | number of data volumes this index covers                                                 |

**Index body:** the same sequence of `ArchiveIndexEntry v6` + variable-length
tail records as embedded in the archive (§1.1 above).

**`IndexFileFooter` — 45 bytes**

| Field         | Type       | Bytes | Notes                                                                                                                    |
|---------------|------------|-------|--------------------------------------------------------------------------------------------------------------------------|
| `signature`   | `[u8; 9]`  | 9     | `b"DARIDXEND"`                                                                                                           |
| `entry_count` | `u32`      | 4     | total number of index entries                                                                                            |
| `checksum`    | `[u8; 32]` | 32    | BLAKE3 of all bytes from offset 0 up to (but not including) this footer; gives the `.dari` file self-contained integrity |

#### 2.2 — Writer: `src/index_writer.rs` (new file)

```rust
pub struct IndexWriter {
    /* path, in-progress hasher, entry count */
}

impl IndexWriter {
    pub fn new(path: &Path, archive_timestamp: u64, total_volumes: u16) -> Result<Self>;
    pub fn write_entry(&mut self, wrapper: &ArchiveIndexEntryWrapper) -> Result<()>;
    pub fn finish(self) -> Result<()>;  // writes IndexFileFooter with BLAKE3 over all prior bytes
}
```

`ArchiveBuilder::build` calls `IndexWriter::finish` when
`target_version == V6`. `commands/create.rs` and `commands/append.rs`
construct the `IndexWriter` and inject it into `ArchiveBuilder`.

#### 2.3 — Reader: `load_index` in `src/reader.rs`

```rust
pub fn load_index(idx_path: &str, locale: &Locale) -> Result<ArchiveState>
```

Reads the `.dari` file in a single sequential pass:

1. Validate `IndexFileHeader` signature and version.
2. Parse all index entries identically to `load_v6`'s inner loop.
3. Read `IndexFileFooter`; verify `entry_count` matches entries parsed and the
   BLAKE3 checksum of the file matches the stored value.
4. Return `ArchiveState` with `entries`, a synthetic `header` (only `timestamp`
   and `version` are meaningful), and `index_offset = 0` (unused for external
   index).

#### 2.4 — Auto-discovery in all reader-side commands

`commands/inspect.rs`, `commands/extract.rs`, `commands/list.rs`:

1. Given `-f archive.dar` (or `-f archive.dar.001`), compute the candidate
   index path: strip any trailing `.NNN` volume suffix, then append `.dari`.
2. If the candidate exists and its `archive_timestamp` matches the opened
   archive volume's header `timestamp`, call `load_index` instead of
   `load_archive`.
3. If `archive_timestamp` mismatches, emit a warning:
   ```toml
   [cli.common.warnings]
   stale_index = "external index is stale (timestamps differ); re-run 'dari reindex -f {file}' or pass --no-index to skip"
   ```
4. If no candidate exists, fall back to reading the embedded index from the
   archive (v5 or v6).
5. A `--no-index` flag bypasses auto-discovery (useful for recovery and
   testing).

#### 2.5 — New subcommand: `dari reindex -f archive.dar`

Regenerates `.dari` by reading the embedded index already present in the
archive. Accepts `--encrypt-passphrase`. Does not rewrite data volumes.

Add `src/commands/reindex.rs`, export in `src/commands/mod.rs`, wire in
`src/main.rs`.

```toml
[cli.reindex]
messages.reindex_done = "index written to {path}"
errors.reindex_failed = "failed to rebuild index for {file}"
```

#### Files touched in Phase 2

`src/index_writer.rs` (new), `src/reader.rs`, `src/archive_builder.rs`,
`src/commands/create.rs`, `src/commands/append.rs`, `src/commands/inspect.rs`,
`src/commands/extract.rs`, `src/commands/list.rs`,
`src/commands/reindex.rs` (new), `src/commands/mod.rs`, `src/main.rs`,
`src/cli.rs`, `locales/en.toml`, `locales/ru.toml`.

---

### Phase 3 — Split archive + per-volume sidecar checksums ✅

**Goal:** Allow `dari create --split-size` to produce multi-volume archives
with a per-volume BLAKE3 sidecar file, eliminating the FAT32/optical-media
file-size barrier and enabling parallel transfer and verification.

#### 3.1 — Volume naming convention

Given `-f archive.dar --split-size 2G`:

- Volume 1: `archive.dar.001`
- Volume 2: `archive.dar.002`
- …up to `archive.dar.999` (zero-padded decimal, 3 digits)
- External index: `archive.dari` (one file covers the entire set)

For single-file archives (no `--split-size`), the file remains `archive.dar`
with the index still embedded. A `archive.dar.b3` sidecar is written
automatically when `target_version == V6` (see §3.3).

`total_volumes` in both `ArchiveHeader` and `IndexFileHeader` is set to the
final count once all volumes are sealed.

#### 3.2 — Writer changes in `src/archive_builder.rs`

`ArchiveBuilder` gains:

```rust
split_threshold: Option<u64>,      // bytes; None = unlimited (single-file)
current_volume: u16,               // 0-based, increments at each seal
volume_base_path: Option<PathBuf>, // set when split_threshold is Some
```

`commit_prepared` checks `self.writer.stream_position() + compressed_size >
split_threshold` **before** writing the data block. If the threshold would be
exceeded:

1. Seal the current volume: write the embedded v6 index (covering all entries
   so far for recovery), write the footer, flush and close.
2. Compute BLAKE3 of the sealed volume file; write its sidecar (§3.3).
3. Open the next volume file (`archive.dar.NNN+1`), write a fresh header with
   the updated `volume_number`.
4. Increment `current_volume`; update `total_volumes` in the live `IndexWriter`.

`ArchiveIndexEntry::volume_number` is set to `self.current_volume` for every
new entry.

`ArchiveBuilder::build` (final seal):

1. Writes the embedded index into the last volume (allows single-volume
   recovery without `.dari`).
2. Calls `IndexWriter::finish` to write the unified `.dari`.
3. Computes BLAKE3 of the last volume; writes its sidecar.
4. Patches `total_volumes` in the headers of all prior volumes by seeking to
   byte offset 14 (the `volume_number`/`total_volumes` fields of the v6 header)
   and writing the final `u16` value.

#### 3.3 — Per-volume sidecar checksum file

After sealing each volume `archive.dar.NNN`, write `archive.dar.NNN.b3`:

```
<64-hex-char BLAKE3 hash>  archive.dar.NNN\n
```

This is exactly the format produced and consumed by `b3sum`, enabling standard
tool interoperability:

```sh
b3sum --check archive.dar.001.b3
```

For single-file archives the sidecar is `archive.dar.b3`.

A new helper `src/sidecar.rs` exposes two functions:

```rust
pub fn write_b3_sidecar(volume_path: &Path) -> Result<()>;
pub fn verify_b3_sidecar(volume_path: &Path) -> Result<bool>;  // used by Phase 3.4
```

#### 3.4 — New subcommand: `dari verify -f archive.dar`

Performs layered integrity checks in order, short-circuiting on failure.

**Layer 1 — Volume file checksums (fast, no decompression)**

For each volume `archive.dar.NNN` (discovered via `total_volumes` in the index
or by globbing `archive.dar.[0-9][0-9][0-9]`):

1. Check that `archive.dar.NNN.b3` exists.
2. Stream the entire volume; compute BLAKE3; compare to sidecar.
3. Report `OK` / `FAIL` per volume.

**Layer 2 — Per-entry stored checksums (fast, no decompression)**

Load the index (`.dari` preferred, embedded fallback). For each entry:

1. Seek to `(volume_number, offset)`.
2. Read exactly `compressed_size` bytes.
3. BLAKE3-hash them; compare to `stored_checksum`.
4. Report per-entry result.

**Layer 3 — Original content checksums (slow; opt-in with `--full`)**

For each entry: Layer 2 seek + read → decrypt (if encrypted) → decompress →
BLAKE3 → compare to `checksum`. Reports per-entry result.

Output is one line per entry. A `--json` flag emits machine-readable results.

```toml
[cli.verify]
messages.layer1_ok = "volume {v}: OK ({bytes} bytes)"
messages.layer1_fail = "volume {v}: BLAKE3 MISMATCH"
messages.layer2_ok = "entry {path}: stored bytes OK"
messages.layer2_fail = "entry {path}: stored bytes CORRUPTED (volume {v} offset {off})"
messages.layer3_ok = "entry {path}: content OK"
messages.layer3_fail = "entry {path}: content CORRUPTED (decompressed hash mismatch)"
errors.verify_failed = "archive verification failed"
```

Add `src/commands/verify.rs`, export in `src/commands/mod.rs`, wire in
`src/main.rs`.

#### 3.5 — Extractor volume resolution in `src/extractor.rs`

Add a helper:

```rust
fn resolve_volume_path(base: &Path, volume: u16) -> PathBuf {
    // If base ends with ".NNN", strip the suffix.
    // For volume == 0 and single-file archives, return base unchanged.
    // Otherwise return base.with_extension(format!("{:03}", volume + 1)).
}
```

`extract_one` uses `entry.volume_number` to call `resolve_volume_path` and
open the correct file. For single-volume archives `volume_number == 0` and
`base` is returned as-is — fully backwards compatible with all v5 call sites.

#### Files touched in Phase 3

`src/archive_builder.rs`, `src/extractor.rs`, `src/sidecar.rs` (new),
`src/commands/create.rs`, `src/commands/append.rs`, `src/commands/extract.rs`,
`src/commands/verify.rs` (new), `src/commands/mod.rs`, `src/main.rs`,
`src/cli.rs`, `locales/en.toml`, `locales/ru.toml`.

**Depends on:** Phases 0, 1, 2.

---

### Phase 4 — Per-entry stored checksum (integrity gap closure) ✅

> The `stored_checksum` field is defined in the Phase 1 struct layout and
> computed in `ArchiveBuilder::commit_prepared`. This phase covers the
> reader-side and verifier-side consumption only.

#### 4.1 — `ArchiveIndexEntryWrapper` accessor

Add a method that returns `None` for v5 entries (identified by an all-zero
`stored_checksum`, which is treated as "not present" — a real all-zero BLAKE3
hash is astronomically improbable and is documented as a sentinel):

```rust
pub fn stored_checksum_v6(&self) -> Option<&[u8; 32]> {
    if self.entry.stored_checksum == [0u8; 32] { None } else { Some(&self.entry.stored_checksum) }
}
```

#### 4.2 — Layer 2 in `dari verify`

Phase 3 §3.4 already wires this field. This section documents the rationale:
`stored_checksum` closes the integrity gap for **unencrypted compressed
entries** where the existing per-entry `checksum` (covering original content)
can only be verified after full decompression, while `stored_checksum` allows
instant byte-level verification at rest without touching the decompressor.
Encrypted entries are already covered by the ChaCha20-Poly1305 authentication
tag stored in the `extra` field (`et=…`).

#### 4.3 — TUI preview integrity indicator in `src/tui/preview.rs`

When `build_preview` reads entry bytes, it verifies them against
`stored_checksum_v6()` if present. On success a shield glyph (`🛡` or `✓`
depending on `powerline` mode) is appended to the status bar string in
`render_status.rs`. On failure a new `PreviewContent::StoredChecksumMismatch`
variant is returned, which the content pane renders as a prominent red warning
instead of the file contents.

---

### Phase 5 — Chunked AEAD encryption (streaming) ✅

**Motivation:** Resolves roadmap item 7.2. Each file's compressed stream is
split into 1 MiB segments encrypted independently, bounding peak RAM usage to
one segment (≈ 1 MiB) regardless of file size.

#### 5.1 — Segment layout on disk

For each file with chunked encryption the stored data block is:

```
[segment_0_ciphertext : up to SEGMENT_SIZE bytes][tag_0 : 16 bytes]
[segment_1_ciphertext : up to SEGMENT_SIZE bytes][tag_1 : 16 bytes]
…
[segment_N_ciphertext : remainder              ][tag_N : 16 bytes]
```

`SEGMENT_SIZE = 1_048_576` (1 MiB) added to `src/constants.rs`.

`nonce_i` for segment `i`:

```
base_nonce XOR (i as u64).to_le_bytes() zero-padded to 12 bytes
```

where `base_nonce = nonce_from_checksum(&file_data.checksum)` (unchanged from
single-segment encryption).

#### 5.2 — New bitflag

Add to `src/constants.rs`:

```rust
pub mod flags {
    pub const CHUNKED_ENCRYPTION: u16 = 0b0000_0000_0000_0100;
}
```

Set in `pipeline.rs::encrypt_if_enabled` when chunked mode is active.

#### 5.3 — Segment count in `extra` field

New extra key in `src/constants.rs`:

```rust
pub mod extra_keys {
    pub const ENC_SEGMENTS: &str = "es";  // u64 decimal: number of AEAD segments
}
```

Old extractors that do not know `CHUNKED_ENCRYPTION` will fail to decrypt
(wrong tag position) and surface an error — the desired safe-fail behaviour.

#### 5.4 — `pipeline.rs` writer path

`encrypt_if_enabled` gains a chunked branch activated when
`config.chunked_encryption == true`:

1. Iterate the compressed buffer in `SEGMENT_SIZE` slices.
2. Encrypt each slice in-place with `nonce_i`; append the 16-byte tag.
3. Concatenate segments; set `bitflags |= CHUNKED_ENCRYPTION`.
4. Store segment count in `extra` via `upsert_extra_pair(…, ENC_SEGMENTS, …)`.

#### 5.5 — `extractor.rs` reader path

`extract_one` checks `bitflags & CHUNKED_ENCRYPTION`. If set:

1. Read `es` from `extra` to get segment count.
2. Loop: read `min(SEGMENT_SIZE, remaining) + TAG_LEN` bytes per segment;
   decrypt in-place; write plaintext directly to the output file.
3. Peak RAM per file = `SEGMENT_SIZE + TAG_LEN` = 1,048,592 bytes.

#### 5.6 — CLI flag

`--chunked-encryption` on `create` / `append` (requires `--encrypt` or
`--encrypt-passphrase`; implies `--format-version 6`).

**Depends on:** Phases 0, 1.

---

### Phase 6 — Unix inode and extended-attribute preservation

**Motivation:** `dari` currently stores only `uid`, `gid`, and permission bits
(via `get_mode` in `utils.rs`). Full backup fidelity on Linux and macOS
requires preserving hard-link identity, POSIX ACLs, SELinux labels, and macOS
resource forks.

#### 6.1 — xattr blob format

The `xattr_length` field (Phase 1 §1.1) enables a variable-length blob
appended after `extra_bytes` in each index entry:

```
xattr_blob  ::= xattr_entry*
xattr_entry ::= name_len  : u16
                name      : [u8; name_len]    (UTF-8 xattr name)
                value_len : u32
                value     : [u8; value_len]   (raw bytes)
```

`xattr_length = 0` means no xattr data (the fixed `u32` field is still present
in the on-disk struct and occupies 4 bytes).

Hard-link identity is stored as two synthetic entries with reserved names
`__dari_inode__` (8-byte LE `u64` inode number) and `__dari_device__` (8-byte
LE `u64` device ID). The extractor strips these and uses them to reconstruct
hard links rather than writing them as filesystem xattrs.

#### 6.2 — Collection in `src/archive_builder.rs`

After `pipeline.process_file`, call a platform-conditional helper:

```rust
#[cfg(unix)]
fn collect_xattrs(path: &Path) -> Vec<(String, Vec<u8>)> {
    xattr::list(path).into_iter().flatten()
        .filter_map(|name| xattr::get(path, &name).ok().flatten()
            .map(|v| (name.to_string_lossy().into_owned(), v)))
        .collect()
}

#[cfg(not(unix))]
fn collect_xattrs(_: &Path) -> Vec<(String, Vec<u8>)> { vec![] }
```

For hard-link detection on Unix: `std::fs::symlink_metadata(path)` provides
`st_ino` and `st_dev` via `std::os::unix::fs::MetadataExt`. If `(dev, ino)` is
already in a `HashMap<(u64, u64), String>` mapping to an existing archive path,
the new entry is stored with `xattr_length > 0` and a synthetic
`__dari_hardlink_target__` xattr naming the primary archive path. No file data
is stored again.

#### 6.3 — Restoration in `src/extractor.rs`

After writing file data, in a second pass over all extracted entries:

1. For entries with `__dari_hardlink_target__`: call `std::fs::hard_link(target,
   entry_path)` instead of writing any data. Hard-link entries must be
   processed after all data entries — sort them to the end of the extraction
   pass, or perform a two-pass extraction (data first, hard links second).
2. For all other entries, call `xattr::set(dest_path, name, value)` for each
   `xattr_entry`, skipping the `__dari_inode__` and `__dari_device__` synthetics.

**New dependency in `Cargo.toml`:** `xattr = "1"` under `[dependencies]`
(the crate is a no-op on Windows, so no `#[cfg]` guard is needed at the
`Cargo.toml` level).

#### 6.4 — CLI flag

`--preserve-xattrs` on `create` / `append` (implies `--format-version 6`).

**Depends on:** Phases 0, 1.

**Note:** Ensure this functionality will not break Windows support - both for creating archives on Windows and
extracting archives created on Unix. The `xattr` crate is a no-op on Windows, so it should not cause any issues, but
testing is essential.

---

### Phase 7 — Incremental backup

**Motivation:** Re-archiving a large directory tree after minor changes wastes
time. An incremental mode appends only files whose content has changed since
the last archive snapshot.

#### 7.1 — New subcommand: `dari incremental`

```
dari incremental -f archive.dar [--since <ISO-8601 | @unix_ts>] dir/ …
```

If `--since` is omitted, the reference timestamp is read from
`ArchiveHeader::timestamp` of the base archive (i.e. "changed since this
archive was created").

#### 7.2 — `walker::scan_files` extension

Add a `FileSource::Incremental { base_path: PathBuf, since: u64 }` variant.
The walker filters out files whose `mtime <= since`; all other behaviour
(`.gitignore`, `.darignore`, hidden files) is unchanged.

`since` is a Unix second value directly comparable to the
`modification_timestamp` already stored in every `ArchiveIndexEntry`.

#### 7.3 — Snapshot section in `.dari`

An optional snapshot section is appended to the `.dari` body immediately
before `IndexFileFooter`. A single `snapshot_present: u8` byte (`0` = absent,
`1` = present) marks its start.

Each snapshot entry:

```
path_length : u32
path        : [u8; path_length]
checksum    : [u8; 32]           (original-content BLAKE3)
mtime       : u64                (Unix seconds)
```

The snapshot allows `dari incremental` to detect content-identical renames
without re-reading file data: if a new file's BLAKE3 checksum already appears
in the snapshot, a `LINKED_DATA` dedup entry is created rather than a full
re-add.

#### 7.4 — `inspect` TUI marker

`FlatNode` in `src/tui/tree.rs` gains an `incremental: bool` field set when
the entry's `modification_timestamp` is strictly greater than the archive's
base `timestamp`. The status bar in `render_status.rs` renders a `Δ` prefix
for such entries.

**Open questions:**

- Conflict resolution when the same path appears in both the base archive and
  an incremental layer (current plan: most-recent wins, governed by
  `--on-conflict`).
- Whether multiple incremental layers should be merged into a single base
  (compaction via a future `dari compact` command) or kept as additive layers.

**Depends on:** Phases 0, 1, 2.

#### 7.5 new `compact` subcommand

Merge multiple incremental archive layers into a single base archive,
eliminating superseded entries and reclaiming dead data blocks. Analogous to
`dar`'s catalogue merge.

---

### Phase 8 — `dari migrate` command

**Goal:** Convert an existing v5 `.dar` archive to v6 in-place or to a new
output path, without re-compressing or re-encrypting any data.

```
dari migrate -f old.dar -o new.dar [--split-size <N>] [--preserve-xattrs]
```

#### 8.1 — Algorithm

1. Open `old.dar`; call `load_v5` to obtain all index entries.
2. Create a v6 `ArchiveBuilder` (`target_version = V6`) pointed at `new.dar`.
3. For each v5 entry in index order:
   a. Seek to `entry.offset`; read exactly `entry.compressed_size` bytes.
   b. Compute `stored_checksum = blake3::hash(bytes_read)`.
   c. Write the bytes verbatim to the new archive (no re-compression, no
   re-encryption).
   d. Construct a v6 `ArchiveIndexEntry` from the v5 fields, setting
   `stored_checksum`, `volume_number = 0`, and `xattr_length = 0`.
4. Call `build` to write the v6 embedded index, footer, `.dari`, and `.b3`
   sidecar.
5. On success, print the output path; optionally replace `old.dar` with
   `new.dar` when `-o` is omitted (in-place mode with atomic rename).

Migration is O(n) in bytes read/written with no decompression CPU cost; only
BLAKE3 is computed over the existing bytes.

#### 8.2 — Files

`src/commands/migrate.rs` (new), `src/commands/mod.rs`, `src/main.rs`,
`src/cli.rs`, `locales/en.toml`, `locales/ru.toml`.

```toml
[cli.migrate]
messages.migrating = "migrating {file} to format v{version}…"
messages.migrate_done = "migration complete: {entries} entries written to {out}"
errors.migrate_failed = "migration failed for {file}"
errors.already_v6 = "{file} is already format v6; nothing to do"
```

**Depends on:** All prior phases.

---

### Phase 9 - Updating docs

#### 9.1 - Clean up the docs

1. Check that everything is up-do-date with version 5
2. Move that to separate sub-folder V5

#### 9.2 Write docs about V6

1. Create separate folder for new archive format version
2. Write there format specification, differences with V5, migration guide, implementation details for library in another
   language

---

### Phase dependency graph

```
Phase 0 — version dispatch
  └── Phase 1 — v6 structs
        ├── Phase 2 — external index (.dari)
        │     └── Phase 3 — split volumes + per-volume .b3 sidecars
        │           └── Phase 4 — stored_checksum consumption (verify / TUI)
        ├── Phase 5 — chunked AEAD (independent of Phases 2–4)
        ├── Phase 6 — xattr preservation (independent of Phases 2–5)
        └── Phase 7 — incremental backup ── also depends on Phase 2
              └── Phase 8 — dari migrate ── depends on all phases
              └── Phase 9 — updating docs ── depends on all phases
```

Phases 5, 6, and 7 are independent of each other once Phases 0 and 1 are
complete and can be worked in parallel.

---
