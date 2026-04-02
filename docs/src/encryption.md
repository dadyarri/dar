# Encryption

**dari** supports file data encryption using the ChaCha20-Poly1305 AEAD algorithm (RFC 8439).

## Algorithm Parameters

| Parameter      | Value                                                                |
|----------------|----------------------------------------------------------------------|
| Algorithm      | ChaCha20-Poly1305                                                    |
| Key size       | 256 bits (32 bytes)                                                  |
| Nonce size     | 96 bits (12 bytes)                                                   |
| Auth tag size  | 128 bits (16 bytes)                                                  |
| Key derivation | BLAKE3 `derive_key("dari.v1.chacha20poly1305.key", passphrase_utf8)` |

## Encryption Process

1. A 256-bit key is derived from the passphrase using BLAKE3 `derive_key` with the fixed
   context string `"dari.v1.chacha20poly1305.key"`.
2. The nonce (12 bytes) is taken from the first 12 bytes of the file's BLAKE3 checksum.
   This makes the nonce deterministic: identical files encrypted with the same passphrase
   always produce the same nonce, which allows deduplication to work correctly.
3. The data (compressed bytes, or raw bytes if compression was skipped) is encrypted
   in-place. The 16-byte Poly1305 authentication tag is appended to the end of the
   ciphertext.
4. The `compressed_size` field in the index entry stores the full length of the encrypted
   block, including the 16-byte tag.

## Encryption Metadata Storage

Encryption information is stored in the `extra` field of the index entry:

| Key  | Value                                               |
|------|-----------------------------------------------------|
| `e`  | Algorithm name: `chacha20poly1305`                  |
| `en` | Nonce as lowercase hex (24 characters)              |
| `et` | Authentication tag as lowercase hex (32 characters) |

The bitflag `0x0002` (`INDEX_FLAG_ENCRYPTED_DATA`) is also set in the entry's `bitflags`
field to signal that the entry is encrypted.

## Usage

### Interactive passphrase prompt

```sh
dari create -f out.dar --encrypt src/
```

### Passphrase supplied directly

```sh
dari create -f out.dar --encrypt-passphrase "secret" src/
```

> ⚠️ `--encrypt` and `--encrypt-passphrase` are mutually exclusive.

### Extracting an encrypted archive

```sh
dari extract -f out.dar --encrypt-passphrase "secret"
```

### Browsing an encrypted archive in the TUI

```sh
dari inspect -f out.dar --encrypt-passphrase "secret"
```

## Passphrase Verification on Append

When adding files to an encrypted archive, **dari** verifies that the supplied passphrase
matches the one used at creation time. It does this via an `encryption_probe` — the first
encrypted entry's data bytes are trial-decrypted. If decryption fails the command exits
with an error.

## Limitations

- Only **file data** is encrypted. File paths, metadata (sizes, checksums) and extra
  fields (other than the encryption keys themselves) are stored in plaintext in the index.
- If an archive was created **without** encryption it is not possible to add encryption
  later using `append`.

