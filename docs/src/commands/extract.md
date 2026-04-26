# extract

Extracts files from an archive into a directory.

## Synopsis

```sh
dari extract -f <file> [options] [paths...]
dari -x -f <file> [options] [paths...]
```

## Options

| Flag | Meaning |
|------|---------|
| `-f`, `--file <file>` | Archive path |
| `-d`, `--output-dir <directory>` | Destination directory; defaults to `.` |
| `--encrypt-passphrase <PASSPHRASE>` | Passphrase for encrypted entries |
| `--no-index` | Ignore a `.dari` sidecar and use the embedded index |

If `paths` are omitted, all entries are extracted.

## Notes

- Parent directories are created automatically.
- Linked deduplicated entries resolve back to the primary data block.
- Split v6 archives can be extracted by pointing at volume `.001`.

## Examples

```sh
dari extract -f out.dar
dari extract -f out.dar -d /tmp/out
dari extract -f out.dar src/main.rs src/lib.rs
dari extract -f split.dar.001 --no-index -d /tmp/out
```
