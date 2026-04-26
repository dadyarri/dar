# v6 Migration Guide

Use `dari migrate` to convert a v5 archive into v6.

## What Migration Does

1. Reads the v5 archive index.
2. Copies each stored data block into a new v6 archive verbatim.
3. Computes `stored_checksum` over the copied bytes.
4. Writes a v6 embedded index, `.dari`, and `.b3`.

The operation does **not**:

- recompress files
- decrypt and re-encrypt files
- invent xattr data that did not exist in v5

## Basic Usage

```sh
dari migrate -f old.dar -o new.dar
```

## Split Output

```sh
dari migrate -f old.dar -o new.dar --split-size 4G
```

Split migration writes numbered archive volumes and a single shared `.dari` index.

## In-Place Mode

If `--output` is omitted, migration replaces the original archive path after the new
v6 archive has been written successfully.

Current limitation:

- in-place migration does not support `--split-size`

## When To Migrate

Migrate when you need any v6-only capability:

- `.dari` indexes
- `.b3` sidecars
- split volumes
- stronger verification coverage
- future append/extract workflows that depend on v6 metadata
