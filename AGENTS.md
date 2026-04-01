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
                  → commands/extract.rs → reader.rs (parse header/footer/index)
                                        → extractor.rs (decrypt + decompress + write to disk)
                  → commands/inspect.rs → reader.rs (parse header/footer/index)
                                        → tui/ (ratatui TUI event loop)
                                            → tui/tree.rs (collapsible dir tree)
                                            → tui/preview.rs (file preview; uses extractor.rs)
                                            → tui/search.rs (nucleo_matcher fuzzy search)
                                            → tui/meta_search.rs (tag:value metadata filter)
                                            → tui/icons.rs (Powerline/Nerd Font glyph detection)
                                            → tui/state.rs (AppState, Focus, PreviewMode)
```

- **`src/cli.rs`** — also `include!`'d by `build.rs` to generate shell completions at build time; any CLI change must
  stay compilable in both contexts.
- **`src/commands/append.rs`** — opens an existing `.dar` with read/write access, parses header/footer/index to recover
  entries, ensures encryption mode consistency (encrypted archives demand the original passphrase; unencrypted archives
  reject new encryption), truncates back to `index_offset`, seeds `ArchiveBuilder::import_existing_entries`, then reruns
  `walker::scan_files` + the pipeline before rebuilding the index/footer.
- **`src/commands/extract.rs`** — extracts entries from a `.dar` to an output directory (`-d`, default `.`); accepts
  an optional list of archive-relative paths to extract selectively; dispatches to `extractor::extract_entry` (single)
  or `extractor::extract_entries` (multiple); supports `--encrypt-passphrase` for encrypted archives.
- **`src/commands/inspect.rs`** — parses the archive via `reader::load_archive`, builds the directory tree, and launches
  `App::run(AppState)` for the ratatui TUI. Accepts `--encrypt-passphrase` to enable preview of encrypted entries.
- **`src/pipeline.rs`** — active processing stage used by `ArchiveBuilder`: BLAKE3 checksum, extension-based compressor
  selection, optional ChaCha20-Poly1305 encryption, and `extra` metadata population (image/audio tags + encryption
  metadata).
- **`src/archive_builder.rs`** — generic over `W: Write + Seek`; tests pass `Cursor<Vec<u8>>` directly. Provides
  `import_existing_entries` so append can preload the dedup map/offsets before writing new data.
- **`src/reader.rs`** — shared archive parser used by both `append` and `inspect`; reads header, footer, and full index
  from a `.dar` file and returns `ArchiveState` (`entries`, `index_offset`, `encryption_mode`, `encryption_probe`).
- **`src/extractor.rs`** — extraction API used by both `commands/extract.rs` and the TUI preview:
  `extract_entry`/`extract_entries` decrypt + decompress + write files to disk; `read_raw_entry_bytes` returns raw bytes
  for the TUI preview; `try_decrypt_bytes` is a best-effort decrypt returning `None` on failure.
- **`src/i18n.rs`** — `Locale` newtype wrapping a `String` + `detect_locale()` (via `sys-locale`). Locale is resolved
  once in `main` and forwarded as `&Locale` to every command `call(matches, locale)` and `scan_files(paths, locale)`.
- **`src/tui/`** — ratatui/crossterm interactive inspector launched by `inspect`:
  - `mod.rs` — `App::run(AppState)` drives the event loop; keybindings: `↑/↓`/`j/k` navigate, `Enter`/`Space` toggle
    directories, `m` opens/switches to/closes the **Metadata** floating window, `c` opens/switches to/closes the
    **Content** floating window, `Esc` closes the active preview window, `↑/↓`/`PageUp`/`PageDown` scroll the preview
    when it has focus, `/` activates fuzzy filename search, `s` activates metadata search (`tag:value` syntax),
    `q`/`Ctrl-C` quit.
  - `state.rs` — `AppState` (entries, tree, visible rows, preview cache, search state, powerline flag),
    `Focus` enum (`List`/`Preview`), and `PreviewMode` enum (`Closed`/`Metadata`/`Content`).
  - `tree.rs` — `build_tree`, `flatten_visible`, `toggle_expanded`; `TreeNode`/`FlatNode` types.
  - `preview.rs` — `build_preview` decodes raw bytes (decrypt → decompress → charset-detect via `encoding_rs` →
    syntax-highlight via `syntect`); returns `EntryPreview { metadata: EntryMetadata, content: PreviewContent }`.
    `PreviewContent` variants: `HighlightedText`, `Text`, `Binary`, `EncryptedNoPassphrase`,
    `EncryptedWrongPassphrase`. `KNOWN_TAGS` maps short extra-field keys (e.g. `"aar"`) to i18n key paths.
  - `search.rs` — `apply_fuzzy_filter` scores file paths with `nucleo_matcher`; returns flat list sorted by score.
  - `meta_search.rs` — `parse_meta_query` / `apply_meta_filter` implement `tag:value` AND-logic metadata filtering.
    `TAG_ALIASES` / `TAG_ALIASES_FULL` map user-facing aliases (`artist`, `album`, `make`, …) to internal extra-field
    keys (`aar`, `aal`, `imk`, …). `resolve_alias` resolves an alias case-insensitively.
  - `icons.rs` — `detect_powerline()` auto-detects Powerline/Nerd Font support via env vars (`DARI_ICONS`,
    `WEZTERM_EXECUTABLE`, `KITTY_WINDOW_ID`, `TERM_PROGRAM`, `TERM`); `folder_icon(expanded, powerline)` returns
    the appropriate glyph string.
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
- **`BrotliCompressor`** (quality 6) — web/text: `html`, `css`, `js`, `ts`, `md`, `toml`, `yaml`, …
- **`ZStandardCompressor`** (level 3) — source code/data: `rs`, `go`, `py`, `log`, `csv`, `sql`, …
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
cargo run -- extract -f out.dar                                # extract all files to current directory
cargo run -- extract -f out.dar -d /tmp/out                    # extract to a specific output directory
cargo run -- extract -f out.dar src/main.rs src/lib.rs         # extract specific archive-relative paths
cargo run -- extract -f out.dar --encrypt-passphrase "secret"  # extract from an encrypted archive
```

Shell completions are written to `completions/` by `build.rs` at build time. That directory is not committed.

Program executions should be run in /tmp/test_dari. Make sure the directory exists.

## Security Scanning

**Do NOT run `codeql_checker`** — it always times out in this environment and never completes
successfully.

## Release Process

Releases are driven by `cargo-release` locally; the CI workflow fires only when a version tag is
pushed. **Never push a tag manually** — always go through `cargo release` to keep the tag and
`Cargo.toml` version in sync.

Install `cargo-release` once:

```sh
cargo install cargo-release
```

> **Note:** `cargo release` defaults to a **dry-run**. Pass `-x` / `--execute` to actually perform
> the release.

### Environment constraints

The sandbox environment has two constraints that require extra flags:

1. **GPG signing is broken** — disable it before running `cargo release`:
   ```sh
   git config --local commit.gpgsign false
   git config --local tag.gpgSign false
   ```

2. **`cargo release` cannot push or publish** — the package lacks `license`/`repository` fields
   required by crates.io, and `git push` is not available directly. Always pass:
   ```sh
   --no-push --no-publish
   ```
   After `cargo release` exits, `Cargo.toml` and `Cargo.lock` will have the new version but will be
   left as uncommitted changes. Commit them and create the annotated tag manually, then push via
   `report_progress`:
   ```sh
   git add Cargo.toml Cargo.lock
   git commit -m "chore: Release X.Y.Z(-pre.N)"
   git tag -a vX.Y.Z(-pre.N) -m "Release X.Y.Z(-pre.N)"
   # then call report_progress to push the branch + tag
   ```

### Stable release (on `master`)

```sh
git config --local commit.gpgsign false && git config --local tag.gpgSign false
cargo release patch --execute --no-push --no-publish   # 5.0.0 → 5.0.1 — bug fixes
cargo release minor --execute --no-push --no-publish   # 5.0.1 → 5.1.0 — new features
cargo release major --execute --no-push --no-publish   # 5.1.0 → 6.0.0 — breaking changes
# then manually commit + tag + report_progress (see above)
```

Each command:
1. Bumps the version in `Cargo.toml` and `Cargo.lock` (leaves them uncommitted).
2. The CI `release.yml` workflow triggers on the pushed tag, verifies `Cargo.toml` matches the tag,
   and publishes a **stable** GitHub release (no `-` in the tag name → `prerelease: false`).

### Pre-release (on any feature/dev branch)

```sh
git config --local commit.gpgsign false && git config --local tag.gpgSign false
cargo release beta --execute --no-push --no-publish   # first run:  5.0.0 → 5.0.1-beta.1
cargo release beta --execute --no-push --no-publish   # second run: 5.0.1-beta.1 → 5.0.1-beta.2
# then manually commit + tag + report_progress (see above)
```

`beta` (also `alpha`, `rc`) is a valid bump level that adds or increments the pre-release suffix.
The `release.yml` workflow detects the `-` in the tag name and marks the GitHub release as
`prerelease: true`.

### How `release.yml` enforces consistency

The workflow now triggers on `push: tags: 'v[0-9]*'` (tag-push, not branch-push). A guard step
aborts the workflow with a clear error when the pushed tag does not match the `version` field in
`Cargo.toml`, preventing accidentally published releases from manual tags.

### Automatic release after approved reviews

When a PR has received all required approvals and is ready to be merged, **automatically start the
release process** without waiting for an explicit instruction. The steps are:

1. Determine the appropriate bump level from the PR content:
   - Bug fixes → `patch`
   - New features → `minor`
   - Breaking changes → `major`
   - Pre-release on a feature/dev branch → `beta` (or `alpha`/`rc`)
2. Merge the PR into the target branch first, then switch to that branch.
3. Run the full release sequence (see the relevant stable/pre-release section above):
   ```sh
   git config --local commit.gpgsign false && git config --local tag.gpgSign false
   cargo release <level> --execute --no-push --no-publish
   git add Cargo.toml Cargo.lock
   git commit -m "chore: Release X.Y.Z(-pre.N)"
   git tag -a vX.Y.Z(-pre.N) -m "Release X.Y.Z(-pre.N)"
   # then call report_progress to push the branch + tag
   ```

## Adding a New Command

1. Add `Command::new("name")` block in `src/cli.rs`.
2. Create `src/commands/name.rs` with a `pub fn call(matches: &ArgMatches, locale: &Locale) -> Result<()>`.
3. Export it in `src/commands/mod.rs`.
4. Add the `Some(("name", sub_matches))` arm in `src/main.rs`.
