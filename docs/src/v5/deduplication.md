# v5 Deduplication

v5 deduplicates entries by original-content BLAKE3 checksum.

## Behavior

1. The first file with a given checksum becomes the primary entry.
2. Later matching files set the linked-data flag.
3. Linked entries reuse the primary entry's `offset` and stored bytes.

This keeps one data block per unique file payload while preserving distinct archive
paths in the index.
