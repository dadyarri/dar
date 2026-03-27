# AGENTS.md — dari

`dari` is a Rust CLI archiver that produces a custom binary format (`.dar`) with automatic compression algorithm
selection by file extension, while respecting `.gitignore` and `.darignore` rules.

## Architecture Overview

```
CLI args (cli.rs) → commands/create.rs → walker.rs (collect files)
                                       → archive_builder.rs (stateful writer)
                                           → pipeline.rs (checksum, compression, optional encryption, extra metadata)
                                           → models/archive.rs (binary structs)
                                           → traits.rs (Compressor dispatch + image-specific optimizers)
                                           → extra.rs (encoded key/value metadata for index extra field)
                                           → counting_writer.rs (byte counter)
```

- **`src/cli.rs`** — also `include!`'d by `build.rs` to generate shell completions at build time; any CLI change must
  stay compilable in both contexts.
- **`src/pipeline.rs`** — active processing stage used by `ArchiveBuilder`: BLAKE3 checksum, extension-based compressor
  selection, optional ChaCha20-Poly1305 encryption, and `extra` metadata population (image/audio tags + encryption
  metadata).
- **`src/archive_builder.rs`** — generic over `W: Write + Seek`; tests pass `Cursor<Vec<u8>>` directly.

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
`.wrap_err_with(|| format!("…"))`. Never use `unwrap` in non-test code.

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
cargo run -- create -f out.dar --encrypt-passphrase "secret" src/  # encrypt stored file data
```

Shell completions are written to `completions/` by `build.rs` at build time. That directory is not committed.

Program executions should be run in /tmp/test_dari. Make sure the directory exists.

## Adding a New Command

1. Add `Command::new("name")` block in `src/cli.rs`.
2. Create `src/commands/name.rs` with a `pub fn call(matches: &ArgMatches) -> Result<()>`.
3. Export it in `src/commands/mod.rs`.
4. Add the `Some(("name", sub_matches))` arm in `src/main.rs`.

