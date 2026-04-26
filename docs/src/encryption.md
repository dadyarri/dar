# Encryption

Encryption behavior depends on the archive generation:

- v5 stores encrypted payloads inside a single `.dar` file. See
  [v5 Encryption](./v5/encryption.md).
- v6 keeps the same core payload model but adds stored-byte checksums, split
  volume support, and richer verification. See [v6 Binary Format](./v6/format.md)
  and [v6 Differences From v5](./v6/differences.md).
