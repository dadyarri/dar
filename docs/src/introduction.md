# Introduction

`dari` is a Rust archiver focused on source trees and long-lived project snapshots.
It combines extension-based compression selection with Git-style ignore handling and
format-aware archive metadata.

## What It Does

- Creates `.dar` archives from directories and individual files.
- Respects `.gitignore` and `.darignore` during recursive walking.
- Deduplicates identical file payloads by BLAKE3 checksum.
- Supports optional encryption for stored file data.
- Provides `list`, `inspect`, `verify`, `reindex`, `incremental`, and `migrate`
  workflows in addition to the base create/extract path.

## Format Generations

`dari` currently understands two archive generations:

- **v5**: single-file `.dar` archives with an embedded index.
- **v6**: adds external `.dari` indexes, `.b3` sidecars, split volumes,
  stored-byte checksums, chunked encryption support, xattr preservation, and
  migration from v5.

See [Format Versions](./versions/index.md) for the versioned reference material.
