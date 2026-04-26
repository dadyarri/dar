# inspect

Launches the interactive TUI inspector for an archive.

## Synopsis

```sh
dari inspect -f <file> [options]
dari -i -f <file> [options]
```

## Options

| Flag | Meaning |
|------|---------|
| `-f`, `--file <file>` | Archive path |
| `--encrypt-passphrase <PASSPHRASE>` | Enable preview for encrypted entries |
| `--no-index` | Ignore a `.dari` sidecar and use the embedded index |

## Key Bindings

| Key | Action |
|-----|--------|
| `↑/↓`, `j/k` | Move selection |
| `Enter`, `Space` | Expand or collapse a directory |
| `m` | Toggle the metadata preview |
| `c` | Toggle the content preview |
| `Esc` | Close the active preview or search mode |
| `/` | Fuzzy filename search |
| `s` | Metadata search using `tag:value` queries |
| `PageUp`, `PageDown` | Scroll the active preview |
| `q`, `Ctrl-C` | Quit |

## Notes

- The inspector prefers a fresh v6 `.dari` sidecar when one is available.
- Content preview verifies v6 `stored_checksum` when present.
- Metadata search supports aliases such as `artist`, `album`, `make`, and `model`.
