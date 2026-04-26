# append

Adds files and directories to an existing archive.

## Synopsis

```sh
dari append -f <file> [options] [paths...]
dari -a -f <file> [options] [paths...]
```

## Options

| Flag                                     | Meaning                                                              |
|------------------------------------------|----------------------------------------------------------------------|
| `-f`, `--file <file>`                    | Existing archive path                                                |
| `-v`, `--verbose`                        | Print one line per committed entry                                   |
| `--compress-images`                      | Enable PNG/JPEG-specific image optimization                          |
| `--encrypt`                              | Prompt interactively for the archive passphrase                      |
| `--encrypt-passphrase <PASSPHRASE>`      | Provide the passphrase directly                                      |
| `--chunked-encryption`                   | Use chunked encryption for new data; requires a v6 encrypted archive |
| `--preserve-xattrs`                      | Preserve Unix xattrs for appended entries; v6-only                   |
| `--dry-run`                              | Show the append plan without modifying the archive                   |
| `--format-version <5\6>`                 | Require a matching archive version                                   |
| `--on-conflict <error\rename\overwrite>` | Control archive-path collisions                                      |

## Notes

- Encryption mode must match the existing archive. You cannot append encrypted
  data to a plaintext archive or plaintext data to an encrypted archive.
- Append reuses deduplication against both existing and newly-added entries.
- For v6 archives, append rewrites the `.dari` index and `.b3` sidecar state.

## Examples

```sh
dari append -f out.dar assets/
dari append -f out.dar --dry-run new-data/
dari append -f out.dar --on-conflict rename new-data/
dari append -f out.dar --encrypt-passphrase secret new-data/
```
