# v6 Implementation Notes

This page is intended for anyone writing a reader, verifier, or tooling library for
v6 archives in another language.

## Parsing Order

For a `.dar` volume:

1. Read the first 5 bytes and confirm `DARI` + version `6`.
2. Parse the full 17-byte `ArchiveHeaderV6`.
3. Seek to `file_length - 19` and parse the footer.
4. Seek to `index_offset` and read `amount_of_files` v6 index entries.

For a `.dari` file:

1. Parse the 17-byte index header.
2. Read v6 index entries until the snapshot marker section.
3. Read snapshot records until the 45-byte footer.
4. Re-hash all bytes before the footer and compare them to the footer checksum.

## Endianness and Packing

- All integer fields are little-endian.
- There is no alignment padding between fields.
- Rust uses `#[repr(C, packed)]` for the on-disk structs.

## Stored Byte Semantics

`stored_checksum` always covers the exact bytes persisted in the archive:

- compressed bytes when compression won
- raw bytes when compression was skipped
- encrypted bytes when encryption was enabled

That means a verifier can validate stored bytes before any decompression or decryption.

## Xattr Blob Encoding

The xattr tail is a packed sequence of:

```text
name_len:u16
name:name_len bytes
value_len:u32
value:value_len bytes
```

This sequence repeats until `xattr_length` bytes have been consumed.

Synthetic names currently used by `dari`:

- `__dari_inode__`
- `__dari_device__`
- `__dari_hardlink_target__`

These are used to reconstruct hard links without depending on host inode values at
extract time.

## Snapshot Records In `.dari`

Snapshot records are not part of the embedded archive index. They live only inside the
external `.dari` file and are used by incremental workflows to compare path, checksum,
and modification time across runs.

Treat them as metadata side structures rather than as archive entries.
