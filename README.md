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

Shell completions (Bash, Fish, Zsh, PowerShell) are written to `completions/` on every build.

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

### `encrypt` — encrypt an existing unencrypted archive

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

