# dari

CLI archiver that combines multiple compression algorithms with useful utilities. Respects `.gitignore` and `.darignore`, automatically selects the best compression algorithm per file extension, and supports optional encryption.

## Features

- **Smart compression** — automatically picks Brotli, Zstandard, LZMA, or no compression based on file extension
- **Image optimization** — lossless PNG (oxipng) and JPEG (Lepton) compression via `--compress-images`
- **Encryption** — ChaCha20-Poly1305 encryption with interactive or argument-based passphrase
- **Deduplication** — identical files (by BLAKE3 checksum) are stored only once
- **Interactive inspector** — ratatui TUI for browsing archive contents with fuzzy search and preview
- **Respects ignore rules** — honours `.gitignore` and `.darignore` files

## Installation

Requires Rust 1.85+ and Cargo:

```sh
git clone https://github.com/dadyarri/dar
cd dar
cargo build --release
# binary is at target/release/dari
```

Shell completions (Bash, Fish, Zsh, PowerShell, Elvish) can be generated at runtime using `dari completions <SHELL>`.

## Commands

### `create` — create a new archive

```sh
dari create -f out.dar src/
dari create -f out.dar -o src/              # overwrite if exists
dari create -f out.dar -v src/              # verbose output
dari create -f out.dar --compress-images src/  # optimize PNG/JPEG
dari create -f out.dar --encrypt src/          # prompt for passphrase
dari create -f out.dar --encrypt-passphrase "secret" src/
dari create -f out.dar --dry-run src/          # preview without writing
```

### `append` — add files to an existing archive

```sh
dari append -f out.dar assets/
dari append -f out.dar --compress-images new-data/
dari append -f out.dar --encrypt-passphrase "secret" new-data/
```

### `extract` — extract files from an archive

```sh
dari extract -f out.dar                         # extract all to current dir
dari extract -f out.dar -d /tmp/out             # extract to specific dir
dari extract -f out.dar src/main.rs src/lib.rs  # extract specific paths
dari extract -f out.dar --encrypt-passphrase "secret"
```

### `list` — list archive contents

```sh
dari list -f out.dar          # table output
dari list -f out.dar --json   # JSON output
```

### `inspect` — interactive TUI inspector

```sh
dari inspect -f out.dar
dari inspect -f out.dar --encrypt-passphrase "secret"
```

Keybindings: `↑/↓` or `j/k` navigate · `Enter/Space` toggle dirs · `m` metadata · `c` content · `Esc` close preview · `/` fuzzy search · `s` tag search · `q` quit

### `completions` — generate shell completions

```sh
dari completions bash    # Bash
dari completions zsh     # Zsh
dari completions fish    # Fish
dari completions powershell  # PowerShell
dari completions elvish  # Elvish
```

The completion script is written to stdout; redirect it or pipe it to your shell's completion setup.

```sh
dari encrypt -f out.dar --encrypt
dari encrypt -f out.dar --encrypt-passphrase "secret"
```

## Compression Selection

| Algorithm | Extensions |
|-----------|-----------|
| None (stored) | `jpg`, `png`, `mp4`, `zip`, `gz`, and other already-compressed formats |
| Brotli (quality 6) | `html`, `css`, `js`, `ts`, `md`, `toml`, `yaml`, … |
| Zstandard (level 3) | `rs`, `go`, `py`, `log`, `csv`, `sql`, … (default for unknown extensions) |
| LZMA (level 9) | `iso`, `deb`, `tex`, `patch`, … |

If compression does not reduce the file size, the original bytes are stored instead.

## Architecture

```
CLI args (cli.rs) → commands/create.rs  → walker.rs (collect files)
                                        → archive_builder.rs (stateful writer)
                                            → pipeline.rs (checksum, compression, optional encryption, extra metadata)
                                            → models/archive.rs (binary structs)
                                            → traits.rs (Compressor dispatch + CompressorRegistry)
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
                                            → tui/state.rs (AppState and sub-structs)
```

Key modules:

| Module | Responsibility |
|--------|---------------|
| `src/cli.rs` | CLI definition; all user-visible text uses i18n keys |
| `src/reader.rs` | Shared archive parser (`load_archive`) returning `ArchiveState` |
| `src/archive_builder.rs` | Generic writer (`ArchiveBuilder<W: Write+Seek>`); used by `create` and `append` |
| `src/pipeline.rs` | Per-file pipeline: BLAKE3 checksum → compression → optional encryption → extra metadata |
| `src/extractor.rs` | Extraction API shared by `extract` command and TUI preview |
| `src/traits.rs` | `Compressor` trait + `CompressorRegistry` for O(1) extension-based dispatch |
| `src/errors.rs` | `DariError` enum for structured, machine-consumable error variants |
| `src/walker.rs` | File discovery via `ignore` crate (respects `.gitignore` / `.darignore`) |


