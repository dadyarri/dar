# encrypt

Rebuilds an existing plaintext archive as an encrypted archive.

## Synopsis

```sh
dari encrypt -f <file> [options]
dari -e -f <file> [options]
```

## Options

| Flag | Meaning |
|------|---------|
| `-f`, `--file <file>` | Source archive path |
| `--encrypt` | Prompt interactively for a passphrase |
| `--encrypt-passphrase <PASSPHRASE>` | Provide the passphrase directly |
| `-o`, `--output <file>` | Write to a separate output path |
| `-i`, `--in-place` | Replace the source archive in place |

## Notes

- `encrypt` extracts the plaintext archive into a temporary directory and rebuilds
  it as a new encrypted archive.
- It does **not** preserve the original compression tuning choices beyond what can
  be inferred from file extensions.
- Running `encrypt` on an already encrypted archive is rejected.
