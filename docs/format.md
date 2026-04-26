# Binary Format Reference

`dari` now maintains separate format references for each archive generation.

## Versioned References

- `docs/src/v5/archive_structure.md` — legacy v5 single-file `.dar` format
- `docs/src/v6/format.md` — current v6 archive, external index, and sidecar layout
- `docs/src/v6/differences.md` — field and behavior changes from v5
- `docs/src/v6/migration.md` — operational migration guide
- `docs/src/v6/implementation.md` — parser notes for third-party libraries

If you are implementing a new reader or tooling library, start with the v6 pages and
fall back to the v5 pages only when you need legacy archive compatibility.
