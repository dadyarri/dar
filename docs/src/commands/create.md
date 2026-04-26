# create

Creates a new `.dar` archive from files and directories.

## Synopsis

```sh
dari create -f <file> [options] [paths...]
dari -c -f <file> [options] [paths...]
```

## Options

| Flag                                | Meaning                                                              |
|-------------------------------------|----------------------------------------------------------------------|
| `-f`, `--file <file>`               | Output archive path                                                  |
| `-o`, `--overwrite`                 | Replace an existing archive                                          |
| `-v`, `--verbose`                   | Print one line per committed entry                                   |
| `--compress-images`                 | Enable PNG/JPEG-specific image optimization                          |
| `--encrypt`                         | Prompt interactively for a passphrase                                |
| `--encrypt-passphrase <PASSPHRASE>` | Provide the passphrase directly                                      |
| `--chunked-encryption`              | Use authenticated 1 MiB segments; requires encryption and implies v6 |
| `--preserve-xattrs`                 | Preserve Unix xattrs and hard-link metadata; v6-only                 |
| `--dry-run`                         | Show the planned write set without creating the archive              |
| `--split-size <N>`                  | Split v6 output into numbered volumes                                |
| `--format-version <5\6>`            | Force the target archive version                                     |

## Notes

- `--split-size`, `--chunked-encryption`, and `--preserve-xattrs` force v6 even
  if `--format-version 5` was omitted.
- Directory traversal respects `.gitignore` and `.darignore`.
- Compression and checksum work runs in parallel; archive writes remain serial.
- v6 creates an embedded index, a `.dari` external index, and `.b3` sidecars.

## Examples

```sh
dari create -f out.dar src/
dari create -f out.dar -o src/
dari create -f out.dar --encrypt-passphrase secret src/
dari create -f out.dar --chunked-encryption --encrypt-passphrase secret src/
dari create -f out.dar --split-size 2G src/
```
