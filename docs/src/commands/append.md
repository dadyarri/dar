# append

Adds files and directories to an existing `.dar` archive.

## Synopsis

```
dari append -f <file> [options] [paths...]
dari -a -f <file> [options] [paths...]
```

## Arguments

| Argument | Description                                           |
|----------|-------------------------------------------------------|
| `paths`  | Files and directories to add to the archive           |

## Options

| Flag                                | Description                                                                        |
|-------------------------------------|------------------------------------------------------------------------------------|
| `-f`, `--file <file>`               | Path to the existing archive (required)                                            |
| `-v`, `--verbose`                   | Print the path of each file as it is added                                         |
| `--compress-images`                 | Enable PNG optimisation (oxipng) and JPEG compression (Lepton)                     |
| `--encrypt`                         | Prompt for a passphrase interactively and encrypt file data                        |
| `--encrypt-passphrase <PASSPHRASE>` | Supply the passphrase directly (conflicts with `--encrypt`)                        |
| `--dry-run`                         | Show which files would be added without modifying the archive                      |
| `-h`, `--help`                      | Show help                                                                          |

## Behaviour

### Encryption Consistency

- If the existing archive is **encrypted**, the same passphrase must be supplied via
  `--encrypt-passphrase` or `--encrypt`. Without it the command exits with an error.
- If the existing archive is **not encrypted**, using encryption flags is not allowed.

### Directory Traversal and Deduplication

The same directory traversal, ignore rules, and deduplication behaviour as
[`create`](create.md) applies. Files already present in the archive are deduplicated
based on their BLAKE3 checksum.

### Parallel Processing

Reading and compressing new files runs in parallel. After verification the archive is
opened for writing and the index is rebuilt to include the new entries.

## Examples

```sh
# Add the assets/ directory to an existing archive
dari append -f out.dar assets/

# Add files with verbose output
dari append -f out.dar -v new-data/

# Add files to an encrypted archive
dari append -f out.dar --encrypt-passphrase "secret" new-data/

# Preview without modifying the archive
dari append -f out.dar --dry-run new-data/
```


