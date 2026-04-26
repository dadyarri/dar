# incremental

Appends only files newer than a chosen cutoff.

## Synopsis

```sh
dari incremental -f <file> [options] [paths...]
```

## Options

| Flag                                     | Meaning                                                 |
|------------------------------------------|---------------------------------------------------------|
| `-f`, `--file <file>`                    | Existing archive path                                   |
| `--since <RFC3339\@unix-seconds>`        | Only include files newer than this timestamp            |
| `-v`, `--verbose`                        | Print one line per committed entry                      |
| `--compress-images`                      | Enable PNG/JPEG-specific image optimization             |
| `--encrypt`                              | Prompt interactively for the archive passphrase         |
| `--encrypt-passphrase <PASSPHRASE>`      | Provide the passphrase directly                         |
| `--chunked-encryption`                   | Use chunked encryption for new data; v6-only            |
| `--preserve-xattrs`                      | Preserve xattrs for new entries; v6-only                |
| `--dry-run`                              | Show the incremental plan without modifying the archive |
| `--format-version <5\6>`                 | Require a matching archive version                      |
| `--on-conflict <error\rename\overwrite>` | Control archive-path collisions                         |

## Notes

- When `--since` is omitted, the archive creation timestamp is used as the cutoff.
- If no files are newer than the cutoff, the command exits successfully without
  rewriting the archive.
- Encryption and version checks behave the same way as [`append`](append.md).
