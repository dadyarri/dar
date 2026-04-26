# list

Lists archive entries in a table or JSON form.

## Synopsis

```sh
dari list -f <file> [options]
dari ls -f <file> [options]
dari -l -f <file> [options]
```

## Options

| Flag | Meaning |
|------|---------|
| `-f`, `--file <file>` | Archive path |
| `--json` | Emit JSON instead of a human-readable table |
| `--no-index` | Ignore a `.dari` sidecar and use the embedded index |

## Notes

- JSON output includes `path`, `original_size`, `compressed_size`,
  `compression_method`, `checksum`, `encrypted`, and `linked`.
- For v6 archives, `list` prefers the `.dari` sidecar when it is present and fresh.
