# v6 Differences From v5

## Structural Changes

| Area | v5 | v6 |
|------|----|----|
| Header | 13 bytes | 17 bytes with `volume_number` and `total_volumes` |
| Footer | `index_offset` is `u32` | `index_offset` is `u64` |
| Index entry | 85-byte fixed header | 123-byte fixed header |
| Index location | Embedded only | Embedded plus optional external `.dari` |

## Integrity and Recovery

- v6 adds `stored_checksum` so readers can verify the exact stored bytes before
  decompression.
- v6 adds `.b3` sidecars for whole-volume verification.
- v6 archives can be reindexed without touching payload blocks.

## Scale and Layout

- v6 supports split volumes (`.001`, `.002`, ...).
- `volume_number` identifies where each entry's primary bytes live.
- The wider `index_offset` allows larger archives.

## Metadata

- v6 stores an xattr blob per entry.
- Hard-link reconstruction metadata is carried through synthetic xattrs.

## Operations Enabled By v6

- `reindex`
- split archive creation and extraction
- richer `verify` behavior
- `migrate` from v5 without rewriting payload bytes
