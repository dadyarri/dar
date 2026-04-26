# v5 Compression

v5 and v6 share the same compression method selection logic.

## Methods

| Method | Typical content |
|--------|-----------------|
| None | Already-compressed media and archives |
| Brotli | Web and markup text |
| Zstandard | Source code and structured text |
| LZMA | Large binary or specialized payloads |

## Selection Rules

- Selection is based on file extension.
- Unknown extensions default to Zstandard.
- If compression does not reduce size, the original bytes are stored and the
  method recorded as `None`.
- `--compress-images` enables in-memory PNG and JPEG optimization before the
  bytes are committed.
