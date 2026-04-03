# dari — Refactor & Enhancement Roadmap

This document is a living technical plan produced after a full codebase audit.
Every item is categorised by **area**, labelled with a **priority** (`P1`–`P3`),
and accompanied by a clear rationale.  Items within the same priority may be
worked in any order unless a dependency is noted.

---

## Table of Contents

1. [Code-Quality Quick Wins](#1-code-quality-quick-wins)
2. [Error Handling & Observability](#2-error-handling--observability)
3. [Architecture — Reduce Coupling](#3-architecture--reduce-coupling)
4. [Testability Improvements](#4-testability-improvements)
5. [Test-Coverage Gaps](#5-test-coverage-gaps)
6. [TUI Maintainability](#6-tui-maintainability)
7. [Performance & Resource Usage](#7-performance--resource-usage)
8. [Dependency Hygiene](#8-dependency-hygiene)
9. [Documentation](#9-documentation)
10. [Future Features (out-of-scope for this refactor)](#10-future-features)
11. [Non-goals](#11-non-goals)

---

## 1. Code-Quality Quick Wins

These are low-risk, stand-alone changes that improve readability and reduce
future confusion.

### 1.1 `P2` — Remove duplicate `tempfile` dependency

`tempfile = "3"` appears in both `[dependencies]` and `[dev-dependencies]` in
`Cargo.toml`.  It should only be in `[dev-dependencies]` (it is only used in
tests).

---

### 1.2 `P3` — Add `#[must_use]` to builder and pure functions

Functions like `ArchiveBuilder::with_config`, `make_renamed_path`, and all
pure `compute_*` / `format_*` helpers in `commands/shared.rs` silently discard
their return values if misused.  Adding `#[must_use]` turns this into a
compile-time warning.

---

### 1.3 `P3` — Deduplicate index-entry construction in `archive_builder.rs`

`commit_prepared()` contains two nearly-identical blocks that construct an
`ArchiveIndexEntry` (one for the dedup hit path, one for the normal write
path).  Extract a private `fn make_index_entry(…) -> ArchiveIndexEntry` helper
to remove the duplication and the associated drift risk.

---

## 2. Error Handling & Observability

### 2.1 `P1` — Preserve inner error details in `reader.rs`

**Problem:** UTF-8 decode failures in `reader.rs` discard the original error:

```rust
String::from_utf8(path_bytes).map_err(|_| {
    eyre!(t!("cli.common.errors.utf8_failed", …))
})?;
```

**Proposed fix:** Chain the original error with `.wrap_err(...)` or use
`String::from_utf8_lossy` for paths (with a distinct warning to the user):

```rust
String::from_utf8(path_bytes)
    .wrap_err_with(|| t!("cli.common.errors.utf8_failed", …))?;
```

---

### 2.2 `P1` — Document the `Option`-returning pattern in `extractor.rs`

`read_raw_entry_bytes` returns `Option<Vec<u8>>` (silently converting all I/O
errors to `None`) while the rest of the extractor returns `eyre::Result`.
This is intentional for best-effort preview reads, but:

1. Add a `/// # Errors` or `/// Note: …` doc comment explaining *why* this
   returns `Option` instead of `Result`.
2. Consider adding a debug-level log (behind a `DARI_DEBUG` env-var flag)
   so developers can diagnose unexpected `None` returns without a full rewrite.

---

### 2.3 `P2` — Extract read-with-context helper in `reader.rs`

`reader.rs` repeats the same `file.read_exact(…).wrap_err(t!(…))` pattern
roughly 10 times.  Extract a private helper:

```rust
fn read_exact_ctx(
    file: &mut File,
    buf: &mut [u8],
    ctx_key: &str,
    locale: &Locale,
) -> eyre::Result<()> {
    file.read_exact(buf)
        .wrap_err(t!(ctx_key, locale = locale.as_str()))
}
```

This shortens `reader.rs` by ~60 lines and makes the control flow more
readable.

---

### 2.4 `P2` — Surface silent metadata-extraction failures in `pipeline.rs`

Image EXIF and audio tag extraction currently fails silently:

```rust
let Ok(exif) = ExifReader::new().read_from_container(&mut cursor) else {
    return metadata; // empty — no log, no warning
};
```

**Proposed fix:** Emit a debug-level message (guarded by an env-var or a
`--verbose` flag already present in the CLI) so operators can distinguish
"file has no metadata" from "metadata library failed to parse valid file."

---

### 2.5 `P3` — Structured error types for validation failures

All validation errors currently use `eyre!()` with a translated string.  For
machine-consumption (future `--json` output mode) or unit-testing that asserts
on error *kind*, introduce a `DariError` enum for the most common failure
categories:

```rust
pub enum DariError {
    CorruptArchive(String),
    EncryptionMismatch(String),
    PathConflict { existing: String },
    UnsupportedVersion { found: u8, expected: u8 },
}
```

This is an **additive, non-breaking** change if placed behind a `From<DariError>
for eyre::Report` impl.

---

## 3. Architecture — Reduce Coupling

### 3.1 `P1` — Split `archive_builder.rs` into focused modules

At 858 lines `archive_builder.rs` has at least four distinct responsibilities:

| Responsibility | Suggested module |
|----------------|-----------------|
| Reading file content from disk, chunking | `src/file_reader.rs` |
| Checksum → offset dedup map management | `src/dedup.rs` |
| Conflict mode logic (Error/Rename/Overwrite) | `src/conflict.rs` |
| Binary serialisation (header, entry, footer write) | stays in `archive_builder.rs` |

Each extracted module can then have its own unit tests without needing a full
archive fixture.

**Invariants to preserve:**
- `ArchiveBuilder<W: Write + Seek>` public API stays identical.
- The `import_existing_entries` method (used by `append`) keeps its current
  signature.

---

### 3.2 `P2` — Introduce a `MetadataExtractor` trait in `pipeline.rs`

Image EXIF extraction and audio-tag extraction in `pipeline.rs` are both
"attempt to read key-value pairs from bytes; return empty vec on failure."
Unifying them behind a trait:

```rust
pub trait MetadataExtractor: Send + Sync {
    /// Returns `(key, value)` pairs or an empty vec on failure.
    fn extract(&self, data: &[u8], extension: &str) -> Vec<(String, String)>;
}
```

allows:

- Injecting a no-op extractor in unit tests (`pipeline.rs` tests currently
  cannot test metadata population without real media files).
- Adding future extractors (PDF metadata, video streams) without modifying
  `pipeline.rs`.

---

### 3.3 `P2` — Reduce `commands/append.rs::call()` length (582 lines total, ~249-line function)

`call()` does argument parsing, archive loading, encryption validation,
passphrase verification, dry-run simulation, and archive writing — all in one
sequential block.  Extract into private functions:

```rust
fn validate_archive_state(…) -> eyre::Result<ArchiveState>
fn simulate_conflicts(…) -> Vec<ConflictReport>   // dry-run path
fn execute_append(…) -> eyre::Result<()>           // actual write path
```

The dry-run and execution paths currently duplicate conflict-checking logic;
consolidating them reduces the surface area for divergence bugs.

---

### 3.4 `P3` — `CompressorRegistry` to replace scattered extension lists

`traits.rs` has five compressor implementations each returning a
`get_best_extensions() -> &[&str]` list, plus `compressor_for_extension` that
iterates over a hardcoded slice of compressor instances.

A `CompressorRegistry` struct that owns the extension → compressor mapping
would:
- Make it trivial to add a new compressor without editing the dispatch logic.
- Enable injecting a test registry (e.g., "always use ZStd") without
  conditionals.

```rust
pub struct CompressorRegistry {
    map: HashMap<&'static str, Arc<dyn Compressor>>,
    default: Arc<dyn Compressor>,
}
impl CompressorRegistry {
    pub fn default_registry() -> Self { … }
    pub fn for_extension(&self, ext: &str) -> Arc<dyn Compressor> { … }
}
```

---

## 4. Testability Improvements

### 4.1 `P1` — Filesystem abstraction trait for `reader.rs` and `extractor.rs`

Both modules call `File::open` directly, making unit tests require real files
on disk (currently via `tempfile`).  Introducing a thin trait:

```rust
pub trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}
```

and changing function signatures to `fn load_archive(source: &mut dyn ReadSeek,
…)` (currently `File`) makes it possible to pass a `Cursor<Vec<u8>>` in tests
— the same pattern `ArchiveBuilder` already uses.

`reader.rs` tests already build in-memory archives; the only change is
removing the intermediate `File` write/re-read round-trip, which speeds up
tests.

---

### 4.2 `P1` — Make `walker::scan_files` accept an iterator source

`walker.rs` wraps `ignore::WalkBuilder` directly.  This means:

- Tests that check `.gitignore` / `.darignore` behaviour need real temporary
  directories with real files and ignore files.
- There is no way to inject a synthetic file list without touching the
  filesystem.

**Proposed fix:** Keep `scan_files(paths, locale)` as the public API but
extract the actual walk behind a `trait FileSource`:

```rust
pub trait FileSource {
    fn walk(&self, root: &Path) -> eyre::Result<Vec<ScannedFile>>;
}

pub struct IgnoreWalker;          // real implementation
pub struct FixedFileSource(Vec<ScannedFile>); // test implementation
```

`scan_files` can then accept `impl FileSource` (or default to `IgnoreWalker`).

---

### 4.3 `P2` — Use `Cursor<Vec<u8>>` in `reader.rs` tests instead of `tempfile`

After implementing 4.1, all 14 `reader.rs` tests can drop `tempfile` and use
`Cursor<Vec<u8>>`.  This removes filesystem I/O from the test hot path,
speeding them up and making them reproducible regardless of filesystem state.

---

### 4.4 `P2` — Add a `TestCompressionPipeline` in `pipeline.rs`

`CompressionPipeline::new(config)` currently always selects real compressors.
Adding a `CompressionPipeline::with_compressor(box dyn Compressor)` factory
used in tests gives fine-grained control over which codec is exercised,
turning integration-heavy tests into pure unit tests.

---

### 4.5 `P3` — Expose pure render-data functions in TUI

The render functions in `tui/render_list.rs`, `tui/render_status.rs`, etc.
receive a `&mut ratatui::Frame` and immediately draw to it, making them
impossible to unit test.

**Strategy:** For each render function, extract the *data-preparation* step
into a pure function that returns plain Rust types (strings, booleans, widths).
Only the final "paint to frame" step touches ratatui.  The data functions
become fully testable without a terminal.

Example:

```rust
// Testable
pub fn status_bar_text(state: &AppState) -> String { … }

// Not unit-tested, but trivially thin
pub fn draw_status(frame: &mut Frame, state: &AppState) {
    let text = status_bar_text(state);
    // … render text into frame …
}
```

---

## 5. Test-Coverage Gaps

### 5.1 `P1` — End-to-end integration test: create → append → extract → verify

There is currently no test that exercises the full user workflow.  Add a test
(in a `tests/integration.rs` file) that:

1. Creates an archive with `create::call()` (or directly with `ArchiveBuilder`)
2. Appends additional files with `append::call()`
3. Extracts all files with `extract::call()`
4. Compares extracted byte contents against the originals

This single test would catch serialisation/deserialisation regressions that
exist in no individual unit test today.

---

### 5.2 `P1` — Encryption end-to-end: create encrypted → extract with correct/wrong passphrase

Extend the integration test above to cover the encrypted path:

- `--encrypt-passphrase "secret"` on create → extract succeeds with `"secret"`,
  fails with `"wrong"`, fails with no passphrase.
- Append to encrypted archive without passphrase → should fail with clear error.

---

### 5.3 `P2` — Deduplication with three or more identical files

Current tests only verify the two-file dedup case.  Add a test that adds the
same content three times and asserts:

- Only one data block is written (offset reuse).
- All three index entries carry `INDEX_FLAG_LINKED_DATA` except the first.
- Extraction of any of the three paths returns the correct content.

---

### 5.4 `P2` — Path sanitisation adversarial cases in `extractor.rs`

`calculate_archive_path` in `utils.rs` already strips `../` sequences, but the
extractor's final `fs::create_dir_all` / write path is not tested against:

- `../../etc/passwd`
- Absolute paths (`/tmp/evil`)
- Windows-style drive prefixes (`C:\Windows\`)
- Null bytes in path (`foo\0bar`)

Add dedicated tests asserting that these all result in a sanitised, relative,
safe path.

---

### 5.5 `P2` — Corrupt/truncated archive handling in `reader.rs`

Current negative tests cover wrong signatures and wrong versions.  Add tests
for:

- Footer present but `index_offset` points beyond EOF.
- Valid header + footer but index bytes are all zeros.
- Archive truncated mid-index-entry.
- Archive with zero entries (edge case for empty archive creation).

---

### 5.6 `P2` — Empty-file and single-byte-file round-trip

No test verifies that zero-length files survive a create/extract cycle.  The
compression path may behave differently (some codecs refuse empty input).  Add
a parameterised test across all compression methods.

---

### 5.7 `P2` — `.gitignore` / `.darignore` respecting in `walker.rs`

The current walker tests only verify recursive traversal.  Add tests (using
`tempfile` directories with actual ignore files) that:

- Files matched by `.gitignore` are excluded.
- Files matched by `.darignore` are excluded.
- Files matched by both are excluded.
- Dotfiles are **included** (current default).

---

### 5.8 `P3` — `commands/list.rs` output formatting

`commands/list.rs` (113 lines) has no tests.  At minimum, test the table
header construction and that each entry column is formatted correctly for known
inputs.

---

### 5.9 `P3` — `tui/preview.rs` pure logic

`build_preview` has several distinct code paths (text detection, binary
detection, encrypted-no-passphrase, wrong passphrase).  After separating the
data-preparation logic (see 4.5), add tests for each `PreviewContent` variant
against synthetic byte slices.

---

### 5.10 `P3` — `commands/shared.rs` locale variants

`compute_ratio`, `format_size`, and `compression_method_label` are tested only
for English.  Add Russian locale variants to the existing tests to catch i18n
regressions.

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

### 6.1 `P1` — Decompose `AppState` god-struct

`tui/state.rs` `AppState` has ~24 fields covering five unrelated concerns.
Splitting into nested sub-structs improves locality and makes state
transitions self-documenting:

```rust
pub struct SearchState {
    pub query: String,
    pub active: bool,
}

pub struct MetaSearchState {
    pub query: String,
    pub active: bool,
    pub error: Option<eyre::Report>,
}

pub struct ExtractDialog {
    pub active: bool,
    pub path: String,
    pub resolved: Option<PathBuf>,
    pub error: Option<eyre::Report>,
}

pub struct PreviewState {
    pub mode: PreviewMode,
    pub focus: Focus,
    pub scroll: usize,
    pub line_count: usize,
    pub viewport_height: usize,
    pub cache: Option<(usize, EntryPreview)>,
}

pub struct AppState {
    pub archive_path: PathBuf,
    pub entries: Vec<ArchiveEntry>,
    pub passphrase: Option<String>,
    pub locale: Locale,
    pub powerline: bool,
    pub tree_root: TreeNode,
    pub visible: Vec<FlatNode>,
    pub table_state: TableState,
    pub search: SearchState,
    pub meta_search: MetaSearchState,
    pub extract: ExtractDialog,
    pub preview: PreviewState,
}
```

This change is safe to make gradually: introduce the sub-structs and update
field access sites file-by-file.

---

### 6.2 `P2` — Separate event-dispatch from drawing in `tui/mod.rs`

`tui/mod.rs` (601 lines) interleaves key-event handling and drawing.  Separate
the event handler (`fn handle_event(state: &mut AppState, event: Event) ->
ControlFlow<()>`) from the draw function (`fn draw(frame: &mut Frame, state:
&AppState)`).  The event handler becomes testable with synthetic key sequences.

---

### 6.3 `P2` — Replace magic key literals with named constants

Key bindings are inline `KeyCode::Char('m')` literals scattered through
`tui/mod.rs`.  Define a `Keybindings` struct or module-level constants so
documentation, code, and any future "customisable keys" feature all share one
source of truth.

---

### 6.4 `P3` — Extract `build_and_cache_preview` side effects

`build_and_cache_preview` mutates `AppState` directly.  Splitting the pure
"build preview data" step from the "store into state" step would allow the
data-building step to be tested in isolation (no `AppState` construction
required).

---

## 7. Performance & Resource Usage

### 7.1 `P2` — Avoid cloning compressed content in `pipeline.rs`

`pipeline.rs` lines ~151-152 clone either `compressed_content` or
`original_content` into a new `Vec<u8>` before encryption.  With large files
this doubles peak memory usage.

**Proposed fix:** Consume the inner `Vec` using `Option::take` or by
restructuring `PipelineFileData` so the encryption step takes ownership
instead of borrowing:

```rust
let mut to_encrypt = file_data
    .compressed_content
    .take()
    .unwrap_or(file_data.original_content);
```

---

### 7.2 `P2` — Stream large-file encryption instead of buffering

Currently the entire (potentially multi-GB) compressed buffer is held in RAM
before being encrypted.  Since ChaCha20-Poly1305 requires the tag to be
appended *after* all ciphertext, full buffering is necessary for the current
"tag at end" format.

Two long-term options (format changes — v6):

a. **Chunked AEAD:** Split the file into 1 MB segments, each with its own
   nonce/tag.  Streaming is possible; random access supported.
b. **Encrypt-then-Compress swap:** Compress first, buffer is already the
   compressed (smaller) bytes, then encrypt.  Current code already does this
   but the clone on line 151 can be removed as above.

Option (a) is a breaking format change and belongs in a major version bump.
Option (b) is a code-quality fix (7.1) independent of the format.

---

### 7.3 `P3` — Cache `compressor_for_extension` result per extension

Each file's extension is looked up in `traits.rs::compressor_for_extension`
by iterating all compressors.  When archiving thousands of files with the same
extension this is repeated work.  A `HashMap<&str, &dyn Compressor>` cache
built once at startup makes the lookup O(1).

---

## 8. Dependency Hygiene

### 8.1 `P1` — Move `tempfile` to `[dev-dependencies]` only

`tempfile = "3"` is listed in both `[dependencies]` and `[dev-dependencies]`.
It is only used in tests; it should only appear under `[dev-dependencies]`.

---

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

### 9.1 `P2` — Add `/// # Errors` sections to all public `Result`-returning functions

No public function currently has a `/// # Errors` section, making it
impossible for downstream callers (or contributors) to know what failure modes
to handle.

**Scope:** `reader::load_archive`, `extractor::extract_entry`,
`archive_builder::ArchiveBuilder::add_file`, `pipeline::run`, and all
`commands::*/call` functions.

---

### 9.2 `P2` — Add `/// # Panics` where applicable

Two `unwrap()` calls in `tui/mod.rs` (lines 316, 386) are safe by invariant
but should be documented.  Once replaced (see 1.3) this item is resolved
automatically.

---

### 9.3 `P3` — Architecture diagram in `README.md` / `docs/`

The `AGENTS.md` has an excellent ASCII architecture diagram.  Port it (or an
updated version) to `README.md` or a dedicated `docs/architecture.md` so
contributors who do not read `AGENTS.md` understand the module layout.

---

### 9.4 `P3` — Describe binary format version history in `docs/format.md`

The current format description in `AGENTS.md` is good but embedded in
tooling-instructions.  Extract a standalone `docs/format.md` that documents:

- The v5 binary layout (current).
- Which fields changed between versions.
- Guidelines for introducing v6 (e.g., streaming AEAD in 7.2).

---

## 10. Future Features

*(Out of scope for the current refactor but recorded here for planning.)*

| Feature | Notes |
|---------|-------|
| `dari completions <SHELL>` subcommand | Tracked in `plan.md` Step 19 |
| `encrypt` command: `-o` / `-i` flags | Tracked in `plan.md` Step 20 |
| Streaming encryption (chunked AEAD) | See 7.2; requires format v6 |
| `--json` output mode | Requires structured `DariError` (see 2.5) |
| Customisable TUI keybindings | See 6.3 |
| Windows path-length > 260 chars | Requires extended path API |
| Symlink preservation | Known walker gap; needs UX design |
| `dari diff archive1 archive2` | New subcommand |

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

---

## Suggested Execution Order

| Sprint | Items | Goal |
|--------|-------|------|
| 1 | 1.1, 1.2, 1.4, 8.1 | Quick wins: constants, dedup nonce, dep cleanup |
| 2 | 2.1, 2.3, 3.2 (split `archive_builder`) | Error clarity + module split |
| 3 | 4.1, 4.2 (filesystem abstraction) | Core testability unlock |
| 4 | 5.1, 5.2 (integration tests) | E2E safety net before further refactor |
| 5 | 5.3–5.7 (gap coverage) | Targeted unit-test additions |
| 6 | 3.3, 3.4, 4.4 (injection points) | Dependency injection for pipeline/command |
| 7 | 6.1, 6.2 (TUI decompose) | TUI maintainability |
| 8 | 4.5, 5.9 (TUI pure functions + tests) | TUI testability |
| 9 | 7.1, 7.3, 3.5 (performance) | Performance pass |
| 10 | 8.2, 8.3 (features + audit) | Binary size + security |
| 11 | 9.1–9.4 (docs) | Documentation completion |
