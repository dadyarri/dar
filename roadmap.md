# dari — Refactor & Enhancement Roadmap

This document is a living technical plan produced after a full codebase audit.
Every item is categorised by **area**, labelled with a **priority** (`P1`–`P3`),
and accompanied by a clear rationale.  Items within the same priority may be
worked in any order unless a dependency is noted.

Items marked ✅ have been implemented.

---

## Table of Contents

1. [Testability Improvements](#4-testability-improvements)
2. [Test-Coverage Gaps](#5-test-coverage-gaps)
3. [Performance & Resource Usage](#7-performance--resource-usage)
4. [Dependency Hygiene](#8-dependency-hygiene)
5. [Future Features](#future-features)

---

## 4. Testability Improvements

### ✅ 4.5 `P3` — Expose pure render-data functions in TUI

For each render function in `tui/render_list.rs`, `tui/render_status.rs`, and
`tui/render_preview.rs`, the *data-preparation* step has been extracted into a
pure function that returns plain Rust types (strings, booleans, key pairs).
Only the final "paint to frame" step touches ratatui.

Extracted functions and their test coverage:

| Module | Pure function | Tests |
|--------|--------------|-------|
| `render_status.rs` | `selection_flags()`, `normal_hint_keys()` | 8 tests |
| `render_list.rs` | `row_file_label()`, `row_size_and_algo()` | 6 tests |
| `render_preview.rs` | `ratio_label()`, `build_metadata_rows_data()`, `human_size()` | 9 tests |

---

## 5. Test-Coverage Gaps

### ✅ 5.9 `P3` — `tui/preview.rs` pure logic

`classify_bytes` promoted to `pub(crate)`.  Tests added for each
`PreviewContent` variant:

- `PreviewContent::Binary` — null-byte detection, high control-char ratio
- `PreviewContent::Text { encoding: "UTF-8" }` — plain ASCII, no known extension
- `PreviewContent::HighlightedText` — Rust source routed through syntect
- `PreviewContent::Text { encoding: "Windows-1251" }` — Cyrillic legacy encoding
- Truncation flag — set when bytes exceed 1 MiB, clear otherwise
- Encrypted-entry helper (`is_entry_encrypted`) verified against synthetic extra string

---

### ✅ 5.11 `P3` — Benchmarks for hot paths

`criterion = "0.8"` added as a dev-dependency.  The project was converted from
a pure binary crate to a **lib + bin** crate (new `src/lib.rs`) so that the
Criterion benchmark binaries can link against the library.

Three benchmark suites in `benches/`:

| File | Benchmarks |
|------|-----------|
| `benches/compression.rs` | `pipeline/brotli 1MiB`, `pipeline/zstd 1MiB`, `pipeline/lzma 1MiB` |
| `benches/reader.rs` | `reader/load_archive 1000 entries` |
| `benches/search.rs` | `fuzzy_filter/10k entries — matching query`, `…no match`, `…empty query (flatten)` |

Run with `cargo bench`.

---

## 7. Performance & Resource Usage

### 7.2 `P2` — Stream large-file encryption instead of buffering

Currently the entire (potentially multi-GB) compressed buffer is held in RAM
before being encrypted.  Since ChaCha20-Poly1305 requires the tag to be
appended *after* all ciphertext, full buffering is necessary for the current
"tag at end" format.

Two long-term options (format changes — v6):

a. **Chunked AEAD:** Split the file into 1 MB segments, each with its own
   nonce/tag.  Streaming is possible; random access supported.
b. **Encrypt-then-Compress swap:** Compress first, buffer is already the
   compressed (smaller) bytes, then encrypt.  The unnecessary clone has already
   been removed (7.1 — done); option (a) remains a format-v6 work item.

Option (a) is a breaking format change and belongs in a major version bump.

---

## 8. Dependency Hygiene

### ✅ 8.3 `P3` — Audit and update dependency versions

`cargo update` applied; all transitive dependencies updated to their latest
semver-compatible versions (e.g. `blake3 1.8.3 → 1.8.4`,
`clap 4.5.60 → 4.6.0`, `clap_complete 4.5.66 → 4.6.0`).

Notable items to monitor:

- **`xz2 = "0.1.7"`** — wraps the C `liblzma` library; CVE-2024-3094 affected
  the upstream `xz-utils` 5.6.0/5.6.1 tarballs (not the Rust crate itself).
  No current advisory in the Rust security database.  Continue monitoring via
  `cargo audit`.
- **`syntect = "5"`** — large dependency; consider replacing with a smaller
  highlight crate if binary size becomes a concern in the future.
- **`lofty = "0.23.3"`** — at the latest minor release; watch for semantic-
  versioning bumps that break the metadata API.

---

## Future Features

*(Out of scope for the current refactor but recorded here for planning.)*

---

### Unix inode and extended-attribute preservation

**Motivation:** `dari` currently stores only `uid`, `gid`, and permission bits
(via `get_mode` in `utils.rs`).  Full backup fidelity on Linux and macOS
requires preserving:

- **Hard-link identity** — detected via inode number + device number; entries
  sharing the same `(dev, ino)` pair should be stored once and restored with
  hard links.
- **POSIX ACLs** — Linux `getxattr("system.posix_acl_access")` and
  `getxattr("system.posix_acl_default")`; restore via `setxattr`.
- **SELinux security labels** — `getxattr("security.selinux")`; restore via
  `setxattr`.
- **macOS resource forks** — stored in the `com.apple.ResourceFork` extended
  attribute; also present implicitly in the `._` AppleDouble sidecar files that
  `IgnoreWalker` may pick up.

**Implementation sketch:**

1. Extend `ArchiveIndexEntry` (or its `extra` field) to carry:
   - `inode_id: u64` + `device_id: u64` (for hard-link dedup)
   - A variable-length xattr blob: `[(name_len: u16, name: [u8], value_len: u32, value: [u8])*]`
2. In `archive_builder.rs`, collect xattrs after reading file content (Linux:
   `xattr` crate; macOS: same crate; Windows: no-op).
3. In `extractor.rs`, after writing the file data, restore xattrs and hard links
   in a second pass (hard links require all data to be written first).
4. This is a **format-v6 change** — new index fields are added; a version
   check in `reader.rs` prevents old binaries from silently misreading new
   archives.

**Dependencies to add:** `xattr = "1"` (cross-platform xattr access).

---

### Incremental backup

**Motivation:** Re-archiving a large directory tree after minor changes wastes
time.  An incremental mode should append only files whose content has changed
since the last archive was created.

**Implementation sketch:**

1. A new `dari incremental -f archive.dar [--since <timestamp|snapshot>] dir/`
   subcommand (or `--incremental` flag on `append`).
2. The reference point is the archive's own creation timestamp (stored in
   `ArchiveHeader`) or an explicit ISO-8601 timestamp / snapshot label.
3. `walker::scan_files` is extended (or a new `IncrementalFileSource` impl of
   `FileSource` is added) to filter files whose `mtime > reference_time`.
4. A **snapshot index** (format-v6 extension) maps each archive-relative path
   to its BLAKE3 checksum + mtime so that content-identical renames are
   detected without re-reading file data.
5. The `inspect` TUI shows a `Δ` marker on entries added incrementally.

**Open questions:** conflict resolution between base and incremental layers;
whether multiple incremental layers should be merged or kept separate (like
`dar`'s catalogue approach).

---

### Streaming encryption by chunks (chunked AEAD)

**Motivation:** Item 7.2 documents the current limitation: the full compressed
buffer must reside in RAM before ChaCha20-Poly1305 encryption because the
authentication tag is appended at the end.  For multi-GB files this is
untenable.

**Design (format-v6):**

1. Split each file's compressed stream into **1 MiB segments**.
2. Each segment is independently encrypted:
   `Encrypt(key, nonce_i, plaintext_segment) → (ciphertext_segment, tag_i)`
   where `nonce_i` is derived from the base nonce and the segment index
   (e.g. `nonce XOR i.to_le_bytes()[..12]`).
3. The segment count and per-segment tag list are written into the `extra` field
   (or a new fixed-size trailer after the data block).
4. `extractor.rs` reads and decrypts one segment at a time, writing directly to
   the output file — peak RAM usage is bounded by one segment (1 MiB).
5. The index entry carries a `chunked_encryption = true` flag so that old
   extractors fail with `UnsupportedVersion` rather than silently producing
   corrupt output.

**Relationship to 7.2:** This *is* option (a) from item 7.2.  Scheduling it
for format v6 aligns with the existing `UnsupportedVersion` error variant in
`src/errors.rs`.

---

### Preparation for multiple archive version support

**Motivation:** The codebase currently hard-codes format version `5` in several
places (`src/constants/format.rs`, `src/models/archive.rs`,
`src/reader.rs`).  As v6 features (chunked AEAD, xattr preservation,
incremental snapshots) are introduced, the version-dispatch logic will grow
unwieldy without a deliberate abstraction.

**Implementation sketch:**

1. **Version registry** — introduce a `FormatVersion` enum
   (`V5`, `V6`, …`) in `src/models/archive.rs` (or a new
   `src/format_version.rs`) parsed from the single version byte in the header.
2. **Versioned reader** — `load_archive` dispatches to a version-specific
   `load_v5` / `load_v6` function; shared parsing helpers are extracted into a
   `src/reader/common.rs` module.
3. **Versioned writer** — `ArchiveBuilder` grows a `target_version: FormatVersion`
   field.  Writing v5 archives remains the default; v6 is opt-in via a CLI flag
   (`--format-version 6`) or automatically selected when a v6 feature
   (`--chunked-encryption`, `--preserve-xattrs`) is requested.
4. **Migration helper** — a future `dari migrate -f old.dar -o new.dar --to-version 6`
   subcommand reads a v5 archive and rewrites it in v6 format.
5. **Compatibility error** — the existing `DariError::UnsupportedVersion` variant
   in `src/errors.rs` is already the correct error path; it should be populated
   with both the found version and the maximum supported version so users see a
   clear message.

**Files to touch:** `src/constants/format.rs`, `src/models/archive.rs`,
`src/reader.rs`, `src/archive_builder.rs`, `src/errors.rs`, `src/cli.rs`,
`locales/en.toml`, `locales/ru.toml`.
