# dari — Refactor & Enhancement Roadmap

This document is a living technical plan produced after a full codebase audit.
Every item is categorised by **area**, labelled with a **priority** (`P1`–`P3`),
and accompanied by a clear rationale.  Items within the same priority may be
worked in any order unless a dependency is noted.

Items marked ✅ have been implemented.

---

## Table of Contents

1. [Error Handling & Observability](#2-error-handling--observability)
2. [Architecture — Reduce Coupling](#3-architecture--reduce-coupling)
3. [Testability Improvements](#4-testability-improvements)
4. [Test-Coverage Gaps](#5-test-coverage-gaps)
5. [TUI Maintainability](#6-tui-maintainability)
6. [Performance & Resource Usage](#7-performance--resource-usage)
7. [Dependency Hygiene](#8-dependency-hygiene)
8. [Documentation](#9-documentation)
9. [Future Features (out-of-scope for this refactor)](#10-future-features)
10. [Non-goals](#11-non-goals)

---

## 2. Error Handling & Observability

### ✅ 2.5 `P3` — Structured error types for validation failures

Implemented `DariError` enum in `src/errors.rs` with variants:
`CorruptArchive`, `EncryptionMismatch`, `PathConflict`, `UnsupportedVersion`.
Because `DariError: std::error::Error + Send + Sync`, eyre's blanket impl converts
it automatically — no call-site changes needed.

---

## 3. Architecture — Reduce Coupling

### ✅ 3.1 `P1` — Split `archive_builder.rs` into focused modules (partial)

Two responsibilities extracted into dedicated modules:

- `src/conflict.rs` — `ConflictMode` enum + `make_renamed_path` helper + unit tests
- `src/file_reader.rs` — `PreparedFile` struct + `read_file_content` +
  `prepare_file_from_disk` (the parallel-safe file reading step)

`archive_builder.rs` re-exports these via `pub use` to keep existing callers unchanged.
The dedup (`ExistingFileData`) and binary-serialisation logic remain in
`archive_builder.rs`; further extraction is deferred to a future PR.

---

### ✅ 3.2 `P2` — Introduce a `MetadataExtractor` trait in `pipeline.rs`

`MetadataExtractor: Send + Sync` trait added to `src/pipeline.rs` with three impls:

| Struct | Behaviour |
|--------|-----------|
| `ExifMetadataExtractor` | reads EXIF from image files via `kamadak-exif` |
| `AudioMetadataExtractor` | reads ID3/Vorbis/APEv2 tags via `lofty` |
| `NoOpMetadataExtractor` | returns empty vec (for tests / future feature-flag use) |

`CompressionPipeline::with_extractors(config, Vec<Box<dyn MetadataExtractor>>)` factory
lets tests inject no-op extractors without real media files.

---

### ✅ 3.3 `P2` — Reduce `commands/append.rs::call()` length

Three private helper functions extracted from `call()`:

- `run_dry_run(file, existing_entries, prepared, conflict_mode, locale)` — prints the
  dry-run conflict report without writing.
- `preflight_conflict_check(existing, prepared, mode, locale) -> Result<()>` — validates
  conflicts before truncating the archive (no partial writes on error).
- `execute_append_write(…) -> Result<()>` — performs the actual truncate-and-rewrite.

`call()` is now a thin orchestration layer (~40 lines) that delegates to these helpers.

---

### ✅ 3.4 `P3` — `CompressorRegistry` to replace scattered extension lists

Implemented `CompressorRegistry` in `src/traits.rs` with `default_registry()`
and `for_extension()`.  A `OnceLock<CompressorRegistry>` is initialised once at
startup so extension lookup is O(1).

---

## 4. Testability Improvements

### ✅ 4.1 `P1` — Filesystem abstraction trait for `reader.rs` and `extractor.rs`

`ReadSeek: Read + Seek` trait implemented in `src/reader.rs`; `load_archive`
now accepts `&mut dyn ReadSeek`.  All `reader.rs` tests use `Cursor<Vec<u8>>`
directly — no filesystem I/O required.

---

### ✅ 4.2 `P1` — Make `walker::scan_files` accept a `FileSource` trait

`FileSource` trait added to `src/walker.rs`:

```rust
pub trait FileSource: Send + Sync {
    fn walk(&self, root: &Path, locale: &Locale) -> Result<Vec<ScannedFile>>;
}
```

Two implementations provided:
- `IgnoreWalker` — the real backend (wraps `ignore::WalkBuilder`).
- `FixedFileSource(Vec<ScannedFile>)` — test-only synthetic source.

`scan_files(paths, locale)` keeps its original signature and delegates to `IgnoreWalker`.
`scan_files_with_source(paths, locale, source)` is the new injectable entry point.
`ScannedFile` gains `#[derive(Clone)]` to allow `FixedFileSource` to clone its entries.

---

### ✅ 4.3 `P2` — Use `Cursor<Vec<u8>>` in `reader.rs` tests instead of `tempfile`

All `reader.rs` tests use `Cursor<Vec<u8>>` via `build_archive_bytes` from
`src/test_utils.rs`.  No `tempfile` dependency in `reader.rs` tests.

---

### ✅ 4.4 `P2` — Add a `TestCompressionPipeline` in `pipeline.rs`

`CompressionPipeline::with_compressor(config, Box<dyn Compressor>)` factory added.
When a compressor override is set, `compressor_for_extension` is bypassed entirely,
giving test code fine-grained control over which codec is exercised.

---

### 4.5 `P3` — Expose pure render-data functions in TUI

The render functions in `tui/render_list.rs`, `tui/render_status.rs`, etc.
receive a `&mut ratatui::Frame` and immediately draw to it, making them
impossible to unit test.

**Strategy:** For each render function, extract the *data-preparation* step
into a pure function that returns plain Rust types (strings, booleans, widths).
Only the final "paint to frame" step touches ratatui.  The data functions
become fully testable without a terminal.

---

## 5. Test-Coverage Gaps

### ✅ 5.8 `P3` — `commands/list.rs` output formatting

`format_entry_row` extracted as a `pub(crate)` pure function and covered by
tests for all four compression methods plus Russian locale.

---

### 5.9 `P3` — `tui/preview.rs` pure logic

`build_preview` has several distinct code paths (text detection, binary
detection, encrypted-no-passphrase, wrong passphrase).  After separating the
data-preparation logic (see 4.5), add tests for each `PreviewContent` variant
against synthetic byte slices.

---

### ✅ 5.10 `P3` — `commands/shared.rs` locale variants

`compute_ratio`, `format_size`, and `compression_method_label` tested for
both English and Russian locales in `src/commands/shared.rs`.

---

### 5.11 `P3` — Benchmarks for hot paths

No benchmarks exist.  Add a `benches/` directory (using Criterion or the
built-in `libtest` bench harness) for:

- `CompressionPipeline::run` with Brotli, ZStd, LZMA on a 1 MB synthetic file.
- `reader::load_archive` on a large (1 000-entry) archive.
- `apply_fuzzy_filter` on a 10 000-entry tree.

Baselines prevent accidental performance regressions in future PRs.

---

## 6. TUI Maintainability

### ✅ 6.1 `P1` — Decompose `AppState` god-struct

`AppState` refactored into nested sub-structs:
- `SearchState` — fuzzy filename search (`/`)
- `MetaSearchState` — metadata tag search (`s`)
- `ExtractDialog` — extract-to-path dialog (`x`)
- `PreviewState` — floating preview window (mode, focus, scroll, cache)

---

### ✅ 6.2 `P2` — Separate event-dispatch from drawing in `tui/mod.rs`

`handle_event(state: &mut AppState, event: Event) -> std::ops::ControlFlow<()>`
extracted from `run_loop`.  The event handler returns `Break` to signal quit and
`Continue` for all other events.  `run_loop` is now a minimal 6-line loop.

---

### ✅ 6.3 `P2` — Replace magic key literals with named constants

Key bindings defined as `KeyCode` constants in `tui::keys` module:
`QUIT_LOWER`, `QUIT_UPPER`, `PREVIEW_METADATA`, `PREVIEW_CONTENT`,
`SEARCH_ACTIVATE`, `META_SEARCH_ACTIVATE`, `EXTRACT_ACTIVATE`,
`NAV_UP`, `NAV_DOWN`.

---

### ✅ 6.4 `P3` — Extract `build_and_cache_preview` side effects

`compute_preview_for_entry(state: &AppState, entry_idx: usize) -> EntryPreview`
extracted as a pure function that only reads state.
`build_and_cache_preview` is now a thin wrapper that calls `compute_preview_for_entry`
then stores the result and resets scroll counters.

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

### ✅ 7.3 `P3` — Cache `compressor_for_extension` result per extension

Implemented via `CompressorRegistry` (see 3.4).  A `OnceLock`-backed static
registry is built once at startup; `for_extension` is O(1) HashMap lookup.

---

## 8. Dependency Hygiene

### 8.2 `P2` — Introduce feature flags for optional compression/image backends

All compression and image-processing crates are unconditional dependencies,
producing a large binary even when features like `--compress-images` are never
used.  Introduce Cargo feature flags:

```toml
[features]
default        = ["compress-images"]
compress-images = ["dep:oxipng", "dep:lepton_jpeg"]
audio-meta     = ["dep:lofty"]
image-meta     = ["dep:kamadak-exif"]
```

Users who only need basic archiving can compile a smaller binary with
`--no-default-features`.  No user-facing CLI change is required (flags that
require the feature simply don't appear in `--help` when the feature is
disabled).

---

### 8.3 `P3` — Audit and update dependency versions

Run `cargo audit` and `cargo outdated` periodically.  Notable items currently:

- `xz2 = "0.1.7"` — wraps the C `liblzma` library; CVE exposure should be
  checked.
- `syntect = "5"` — large dependency; consider replacing with a smaller
  highlight crate if binary size matters.
- `lofty = "0.23.3"` — check for semantic-versioning bumps that break the
  metadata API.

---

## 9. Documentation

### ✅ 9.2 `P2` — Add `/// # Panics` where applicable

`toggle_at_cursor` in `tui/mod.rs` documented with `# Panics` explaining that
the `unwrap_or(0)` is a safe fallback invariant.

---

### ✅ 9.3 `P3` — Architecture diagram in `README.md`

Architecture diagram and module table added to `README.md`.

---

### ✅ 9.4 `P3` — Describe binary format version history in `docs/format.md`

`docs/format.md` created with full v5 binary layout, field tables,
version history, and v6 migration guidelines.

---

## 10. Future Features

*(Out of scope for the current refactor but recorded here for planning.)*

| Feature | Notes |
|---------|-------|
| Streaming encryption (chunked AEAD) | See 7.2; requires format v6 |
| `--json` output mode | Requires structured `DariError` (see 2.5) |
| Customisable TUI keybindings | See 6.3 |
| Windows path-length > 260 chars | Requires extended path API |
| Symlink preservation | Known walker gap; needs UX design |
| `dari diff archive1 archive2` | New subcommand |
| TUI render-data pure functions | See 4.5; enables `tui/preview.rs` tests (5.9) |
| Benchmarks for hot paths | See 5.11 |

---

## 11. Non-goals

The following changes are explicitly **not** in scope for this refactor to
preserve user experience:

- Changing the `.dar` binary format in a backwards-incompatible way (any such
  change must bump the version byte and release as a major version).
- Altering any CLI flag, subcommand name, or output format visible to end
  users.
- Changing the default compression algorithm or encryption cipher.
- Removing or replacing i18n support.
