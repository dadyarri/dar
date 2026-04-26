# migrate

Converts a v5 archive into a v6 archive without recompressing or re-encrypting the
stored file data.

## Synopsis

```sh
dari migrate -f <file> [options]
```

## Options

| Flag | Meaning |
|------|---------|
| `-f`, `--file <file>` | Source v5 archive |
| `-o`, `--output <file>` | Output v6 archive path |
| `--split-size <N>` | Write split v6 volumes; requires `--output` |
| `--preserve-xattrs` | Accepted for roadmap compatibility; v5 entries have no xattr payloads to preserve |

## Notes

- Migration copies the stored byte blocks exactly as they appear in the v5 archive.
- The command recomputes v6 `stored_checksum` values over the copied bytes.
- v6 side artifacts are emitted as part of the result: `.dari` and `.b3`.
- Running `migrate` against an archive that is already v6 is rejected.

## Examples

```sh
dari migrate -f old.dar -o new.dar
dari migrate -f old.dar -o new.dar --split-size 4G
```
