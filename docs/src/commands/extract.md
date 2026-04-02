# extract

Extracts files from a `.dar` archive to a specified directory.

## Synopsis

```
dari extract -f <file> [options] [paths...]
dari -x -f <file> [options] [paths...]
```

## Arguments

| Argument | Description                                                              |
|----------|--------------------------------------------------------------------------|
| `paths`  | Optional list of archive-relative paths to extract selectively           |

If no paths are given, all files in the archive are extracted.

## Options

| Flag                                | Description                                                        |
|-------------------------------------|--------------------------------------------------------------------|
| `-f`, `--file <file>`               | Path to the archive (required)                                     |
| `-d`, `--output-dir <directory>`    | Destination directory (default: current directory)                 |
| `--encrypt-passphrase <PASSPHRASE>` | Passphrase for decrypting encrypted entries                        |
| `-h`, `--help`                      | Show help                                                          |

## Behaviour

- Parent directories are created automatically when extracting files with nested paths.
- When extracting an encrypted archive the passphrase must be supplied via
  `--encrypt-passphrase`. Without it the command exits with an error.
- Deduplicated entries (with `INDEX_FLAG_LINKED_DATA`) are extracted correctly: their
  data is read from the primary entry's data block.

## Examples

```sh
# Extract all files to the current directory
dari extract -f out.dar

# Extract all files to /tmp/out
dari extract -f out.dar -d /tmp/out

# Extract specific files only
dari extract -f out.dar src/main.rs src/lib.rs

# Extract from an encrypted archive
dari extract -f out.dar --encrypt-passphrase "secret"

# Selective extraction from an encrypted archive
dari extract -f out.dar -d /tmp/out --encrypt-passphrase "secret" src/main.rs
```


