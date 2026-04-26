# Format Versions

`dari` currently supports two on-disk archive generations.

## v5

v5 is the original single-file format:

- one `.dar` file
- embedded index only
- no sidecars
- no split-volume support
- no stored-byte checksum field
- no xattr blob in the index

See the [v5 overview](../v5/index.md) for the stable legacy reference.

## v6

v6 extends the format for integrity, scalability, and maintenance tooling:

- `.dari` external indexes
- `.b3` sidecar verification files
- split volumes (`.001`, `.002`, ...)
- `stored_checksum` in each index entry
- per-entry `volume_number`
- xattr blobs and hard-link reconstruction metadata
- migration from v5 with `dari migrate`

See the [v6 overview](../v6/index.md) for the current format reference.
