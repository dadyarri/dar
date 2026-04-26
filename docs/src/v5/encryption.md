# v5 Encryption

v5 stores encrypted entries inline inside the `.dar` file.

## Algorithm

- ChaCha20-Poly1305
- 32-byte key derived from the passphrase with BLAKE3 `derive_key`
- deterministic nonce derived from the file checksum

## Storage

- The encrypted payload replaces the normal stored data block.
- The authentication tag is included in `compressed_size`.
- Encryption metadata is recorded in the `extra` field with keys such as `e`,
  `en`, and `et`.

## Limits

- Only file data is encrypted.
- Index metadata, paths, and archive structure remain plaintext.
- v5 does not support the v6 `stored_checksum` field or xattr blobs.
