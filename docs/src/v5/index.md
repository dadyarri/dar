# v5 Overview

v5 is the legacy `.dar` format still readable and writable by `dari`.

## Characteristics

- Single archive file with embedded index and footer.
- Compression selected by file extension.
- Optional whole-entry ChaCha20-Poly1305 encryption.
- Deduplication by original-content BLAKE3 checksum.
- No `.dari` sidecar, no `.b3` sidecar, no split volumes.
- No stored-byte checksum and no xattr payload in the index.

## Recommended Use

Use v5 when you need compatibility with older archives or when the archive does not
need v6-only features such as sidecars, split volumes, chunked encryption support, or
metadata preservation.

For the current extended format, see [v6](../v6/index.md).
