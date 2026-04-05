# Deduplication

**dari** automatically deduplicates files with identical content within a single archive.
This avoids storing the same bytes more than once when copies exist.

## How It Works

1. A **BLAKE3** hash of the original (pre-compression) content is calculated for each file.
2. `ArchiveBuilder` keeps an internal map of `checksum → data_offset` for all files written
   so far.
3. If a new file's checksum matches an already-written entry:
   - The data bytes are **not written again** to the DATA section.
   - A new index entry is added, but its `offset` field points to the data block of the
     original (primary) entry.
   - The bit `0x0001` (`INDEX_FLAG_LINKED_DATA`) is set in the new entry's `bitflags`.

## Extracting Deduplicated Files

When extracting a file that has the `INDEX_FLAG_LINKED_DATA` flag set, the extractor
finds the primary entry that has the same checksum (without the flag), reads the data
at that entry's offset, and decompresses it.

Both files are restored with identical content.

## Deduplication on Append

The `append` command loads the existing archive's entries (offsets and checksums) before
processing new files. This allows new files to be deduplicated against each other **and**
against files already present in the archive.

## Deduplication and Encryption

When encryption is enabled the nonce is derived deterministically from the file's BLAKE3
checksum. This means deduplicated linked entries can reuse the encryption metadata
(nonce and tag) from the primary entry without re-encrypting.

## Limitations

- Deduplication operates only **within a single archive**. It does not work across
  different archives.
- Metadata (filename, path, permissions) is **not deduplicated**: every file has its
  own index entry.

