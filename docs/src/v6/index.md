# v6 Overview

v6 is the current extended archive format.

## What v6 Adds

- embedded and external indexes
- per-volume `.b3` sidecar verification
- split archives with numbered volumes
- `stored_checksum` for the exact stored byte stream
- xattr blobs and hard-link reconstruction metadata
- chunked-encryption-capable metadata layout
- migration from legacy v5 archives

## Artifact Set

Depending on the command and options, a v6 archive may produce:

```text
archive.dar        primary archive
archive.dari       external index
archive.dar.b3     sidecar checksum
archive.dar.001    first split volume
archive.dar.002    later split volume
```

The `.dari` file is metadata-oriented. The `.b3` files protect entire archive volumes.
