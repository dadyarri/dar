# verify

Checks archive integrity.

## Synopsis

```sh
dari verify -f <file> [options]
```

## Options

| Flag | Meaning |
|------|---------|
| `-f`, `--file <file>` | Archive path |
| `--encrypt-passphrase <PASSPHRASE>` | Required for `--full` verification of encrypted entries |
| `--full` | Decrypt and decompress each payload, then verify the original checksum |
| `--json` | Emit machine-readable verification rows |
| `--no-index` | Ignore a `.dari` sidecar and use the embedded index |

## Verification Layers

1. `.b3` sidecar verification for each archive volume.
2. v6 `stored_checksum` verification for the exact on-disk bytes of each entry.
3. Optional full payload verification after decrypt/decompress.

Layer 2 is skipped automatically for v5 entries because they do not store
`stored_checksum`.
