# reindex

Rebuilds the external `.dari` index for an existing v6 archive.

## Synopsis

```sh
dari reindex -f <file>
```

## Options

| Flag | Meaning |
|------|---------|
| `-f`, `--file <file>` | Archive path |
| `--encrypt-passphrase <PASSPHRASE>` | Accepted for interface symmetry; index rebuild does not decrypt payloads |

## Notes

- `reindex` is only valid for v6 archives.
- The resulting `.dari` file reuses the archive timestamp stored in the v6 header.
