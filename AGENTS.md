# AGENTS.md — dari

`dari` is a Rust CLI archiver that produces a custom binary format (`.dar`) with automatic compression algorithm
selection by file extension, while respecting `.gitignore` and `.darignore` rules.

## Architecture Overview

```
CLI args (cli.rs) → commands/create.rs  → walker.rs (collect files)
                                        → archive_builder.rs (stateful writer)
                                            → pipeline.rs (checksum, compression, optional encryption, extra metadata)
                                            → models/archive.rs (binary structs)
                                            → traits.rs (Compressor dispatch + image-specific optimizers)
                                            → extra.rs (encoded key/value metadata for index extra field)
                                            → counting_writer.rs (byte counter)
                  → commands/append.rs  → reader.rs (parse header/footer/index)
                                        → walker.rs (collect files)
                                        → archive_builder.rs (reuse pipeline, rebuild index)
                  → commands/inspect.rs → reader.rs (parse header/footer/index)
                                        → tui/ (ratatui TUI event loop)
                                            → tui/tree.rs (collapsible dir tree)
                                            → tui/preview.rs (file preview; uses extractor.rs)
                                            → tui/search.rs (nucleo_matcher fuzzy search)
                                            → tui/state.rs (AppState, Focus)
```

- **`src/cli.rs`** — also `include!`'d by `build.rs` to generate shell completions at build time; any CLI change must
  stay compilable in both contexts.
- **`src/commands/append.rs`** — opens an existing `.dar` with read/write access, parses header/footer/index to recover
  entries, ensures encryption mode consistency (encrypted archives demand the original passphrase; unencrypted archives
  reject new encryption), truncates back to `index_offset`, seeds `ArchiveBuilder::import_existing_entries`, then reruns
  `walker::scan_files` + the pipeline before rebuilding the index/footer.
- **`src/commands/inspect.rs`** — parses the archive via `reader::load_archive`, builds the directory tree, and launches
  `App::run(AppState)` for the ratatui TUI. Accepts `--encrypt-passphrase` to enable preview of encrypted entries.
- **`src/pipeline.rs`** — active processing stage used by `ArchiveBuilder`: BLAKE3 checksum, extension-based compressor
  selection, optional ChaCha20-Poly1305 encryption, and `extra` metadata population (image/audio tags + encryption
  metadata).
- **`src/archive_builder.rs`** — generic over `W: Write + Seek`; tests pass `Cursor<Vec<u8>>` directly. Provides
  `import_existing_entries` so append can preload the dedup map/offsets before writing new data.
- **`src/reader.rs`** — shared archive parser used by both `append` and `inspect`; reads header, footer, and full index
  from a `.dar` file and returns `ArchiveState` (`entries`, `index_offset`, `encryption_mode`, `encryption_probe`).
- **`src/extractor.rs`** — extraction API (marked `#![allow(dead_code)]` pending the `extract` subcommand):
  `extract_entry`/`extract_entries` decrypt + decompress + write files to disk; `read_raw_entry_bytes` returns raw bytes
  for the TUI preview; `try_decrypt_bytes` is a best-effort decrypt returning `None` on failure.
- **`src/i18n.rs`** — `Locale` newtype wrapping a `String` + `detect_locale()` (via `sys-locale`). Locale is resolved
  once in `main` and forwarded as `&Locale` to every command `call(matches, locale)` and `scan_files(paths, locale)`.
- **`src/tui/`** — ratatui/crossterm interactive inspector launched by `inspect`:
  - `mod.rs` — `App::run(AppState)` drives the event loop; keybindings: `↑/↓`/`j/k` navigate, `Enter`/`Space` toggle
    directories, `Tab` opens/closes/focuses the preview panel, `/` activates fuzzy search, `q`/`Ctrl-C` quit.
  - `state.rs` — `AppState` (entries, tree, visible rows, preview cache, search state) and `Focus` enum.
  - `tree.rs` — `build_tree`, `flatten_visible`, `toggle_expanded`; `TreeNode`/`FlatNode` types.
  - `preview.rs` — `build_preview` decodes raw bytes (decrypt → decompress → charset-detect via `encoding_rs` →
    syntax-highlight via `syntect`); `PreviewContent` variants: `HighlightedText`, `Text`, `Binary`,
    `EncryptedNoPassphrase`, `EncryptedWrongPassphrase`.
  - `search.rs` — `apply_fuzzy_filter` scores file paths with `nucleo_matcher`; returns flat list sorted by score.
- **`src/main.rs` + `locales/*.toml`** — `rust-i18n` is initialized in `main`; CLI text and user-facing runtime errors are
  translated via `t!(...)` keys from `locales/en.toml` and `locales/ru.toml`.

## Binary Format

All on-disk structs live in `src/models/archive.rs`, are `#[repr(C, packed)]`, and implement `bytemuck::Pod + Zeroable`
for zero-copy serialization via `bytemuck::bytes_of(self)`. **Never derive `Pod`/`Zeroable` — always use `unsafe impl`.
** Current format (v5):

| Section          | Marker                                                      |
|------------------|-------------------------------------------------------------|
| Header           | `DARI` (4 B) + version `5` (1 B) + creation timestamp (8 B) |
| File data blocks | raw/compressed/encrypted bytes written by `add_file`        |
| Index entries    | `ArchiveIndexEntry` + path string + extra string            |
| Footer           | `DARIEND` (7 B) + index_offset (4 B) + file count (4 B)     |

## Compression Selection (`src/traits.rs`)

`Compressor` trait has `get_best_extensions() -> &[&str]` used to route files (`compressor_for_extension` falls back to
`ZStandardCompressor` for unknown extensions):

- **`NoneCompressor`** — already-compressed: `jpg`, `png`, `mp4`, `zip`, `gz`, …
- **`BrotliCompressor`** (quality 11) — web/text: `html`, `css`, `js`, `ts`, `md`, `toml`, `yaml`, …
- **`ZStandardCompressor`** (level 19) — source code/data: `rs`, `go`, `py`, `log`, `csv`, `sql`, …
- **`LzmaCompressor`** (level 9) — binary/specialized: `iso`, `deb`, `tex`, `patch`, …
- **`PngOxipngCompressor`** — enabled only with `--compress-images`; optimizes PNG in-memory and keeps original bytes
  when optimization is not smaller.
- **`JpegLeptonCompressor`** — enabled only with `--compress-images`; uses Lepton for JPEG and stores original bytes on
  failure/non-improvement.

> **Note:** `ArchiveBuilder::add_file` now runs the pipeline and writes file data before the index. Duplicate files are
> deduplicated by checksum: later entries reuse the first data offset and set `INDEX_FLAG_LINKED_DATA`.

## File Walking (`src/walker.rs`)

Uses the `ignore` crate (`WalkBuilder`) with:

- `.git_ignore(true)` — respects `.gitignore`
- `.add_custom_ignore_filename(".darignore")` — project-specific exclude file
- `.hidden(false)` — includes dotfiles

Only files (not directories) are collected. Only directory inputs are walked; bare file paths are **not** added (
currently unhandled — a known gap).

## Large File Handling

`archive_builder.rs` uses `CHUNK_SIZE = 512 * 1024` (512 KB). Files larger than this threshold are read in a streaming
loop; smaller files use `std::fs::read`.

## Error Handling

All fallible functions return `eyre::Result<T>`. Chain context with `.wrap_err("…")` or
`.wrap_err_with(|| format!("…"))`. User-facing/runtime error text should come from `rust_i18n::t!("cli.errors.…")`
instead of hard-coded English strings; keep literal error/assert messages only in tests. Never use `unwrap` in non-test
code.

## Utility Helpers (`src/utils.rs`)

- `read_bytes_as::<T>(bytes, offset)` — generic LE-byte reader backed by `traits::FromLeBytes`
- `read_string(bytes, offset, len)` — reads UTF-8 string slice
- `calculate_archive_path(dir_root, file_path)` — strips prefix, sanitizes (removes `..`, `/`, Windows prefixes)
- `get_mode(metadata)` — returns `(uid, gid, perm)` on Unix; returns `(1000, 1000, 644)` placeholder on Windows

## Developer Workflows

```sh
cargo build                          # build; also generates shell completions into completions/
cargo test                           # run all tests (unit tests are inline in the same file)
cargo run -- create -f out.dar src/  # create archive from src/ directory
cargo run -- create -f out.dar -o src/  # overwrite if out.dar exists
cargo run -- create -f out.dar -v src/  # verbose (prints each added file path)
cargo run -- create -f out.dar --compress-images src/  # enable PNG/JPEG optimization
cargo run -- create -f out.dar --encrypt src/  # prompt for passphrase interactively
cargo run -- create -f out.dar --encrypt-passphrase "secret" src/  # encrypt stored file data
cargo run -- append -f out.dar assets/  # append directories/files into an existing archive
cargo run -- append -f out.dar --encrypt-passphrase "secret" new-data/  # passphrase must match prior encryption
cargo run -- inspect -f out.dar                                # browse archive in the ratatui TUI
cargo run -- inspect -f out.dar --encrypt-passphrase "secret"  # inspect an encrypted archive
```

Shell completions are written to `completions/` by `build.rs` at build time. That directory is not committed.

Program executions should be run in /tmp/test_dari. Make sure the directory exists.

## Adding a New Command

1. Add `Command::new("name")` block in `src/cli.rs`.
2. Create `src/commands/name.rs` with a `pub fn call(matches: &ArgMatches, locale: &Locale) -> Result<()>`.
3. Export it in `src/commands/mod.rs`.
4. Add the `Some(("name", sub_matches))` arm in `src/main.rs`.
