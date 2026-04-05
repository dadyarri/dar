# create

Creates a new `.dar` archive from a set of files and directories.

## Synopsis

```
dari create -f <file> [options] [paths...]
dari -c -f <file> [options] [paths...]
```

## Arguments

| Argument | Description                                           |
|----------|-------------------------------------------------------|
| `paths`  | Files and directories to add to the archive           |

## Options

| Flag                                | Description                                                                        |
|-------------------------------------|------------------------------------------------------------------------------------|
| `-f`, `--file <file>`               | Path to the archive to create (required)                                           |
| `-o`, `--overwrite`                 | Overwrite the archive if it already exists                                         |
| `-v`, `--verbose`                   | Print the path of each file as it is added                                         |
| `--compress-images`                 | Enable PNG optimisation (oxipng) and JPEG compression (Lepton)                     |
| `--encrypt`                         | Prompt for a passphrase interactively and encrypt file data                        |
| `--encrypt-passphrase <PASSPHRASE>` | Supply the passphrase directly (conflicts with `--encrypt`)                        |
| `--dry-run`                         | Show which files would be added without writing the archive                        |
| `-h`, `--help`                      | Show help                                                                          |

## Behaviour

### Directory Traversal

When a directory path is given, **dari** recursively walks its contents while respecting
ignore rules from `.gitignore` and `.darignore`. Hidden files (starting with `.`) are
**included** by default.

When a path to an individual file is given, it is added to the archive using just the
filename (without parent directories).

See [Ignore Rules](../ignore_rules.md) for details.

### Deduplication

If two files have identical content (same BLAKE3 hash), the second and subsequent files
are stored as links (`INDEX_FLAG_LINKED_DATA`) without writing the data bytes again. See
[Deduplication](../deduplication.md) for details.

### Parallel Processing

File reading, checksum calculation and compression run in parallel. Writing to the archive
is sequential to maintain correct byte ordering.

### Encryption

Data is encrypted with ChaCha20-Poly1305. See [Encryption](../encryption.md) for details.

## Examples

```sh
# Create an archive from the src/ directory
dari create -f out.dar src/

# Overwrite an existing archive
dari create -f out.dar -o src/

# Create with verbose output
dari create -f out.dar -v src/

# Enable image optimisation
dari create -f out.dar --compress-images assets/

# Encrypt the archive (interactive passphrase prompt)
dari create -f out.dar --encrypt src/

# Encrypt the archive (passphrase supplied directly)
dari create -f out.dar --encrypt-passphrase "secret" src/

# Preview without writing to disk
dari create -f out.dar --dry-run src/
```

### --dry-run Output

```
Dry run — files that would be added to out.dar:
  src/main.rs          8.23 KB → 2.11 KB  [zstd, 26%]
  src/lib.rs           3.10 KB  [stored]
(dry run — no file written)
```


