# Extra Fields

Each index entry in a `.dar` archive can carry an extra fields block — a string of
`key=value` pairs separated by semicolons.

## Format

```
key1=value1;key2=value2;key3=value3
```

### Serialisation Rules

- Pairs with an **empty value** are skipped.
- If the same key appears more than once, only the **last** value is kept in the final
  string.
- If a key or value contains the character `;`, it is replaced by `%3B`
  (percent-encoding) before serialisation and restored when parsing.

### Storage

The byte length of the extra block is recorded in the `extra_length` field of the index
entry. The content itself is stored in the INDEX section immediately after the `path`
field.

## Known Keys

### Encryption

| Key  | Description                                    |
|------|------------------------------------------------|
| `e`  | Encryption algorithm (`chacha20poly1305`)      |
| `en` | Nonce as lowercase hex (24 characters)         |
| `et` | Authentication tag as lowercase hex (32 chars) |

These fields are set automatically when `--encrypt` or `--encrypt-passphrase` is used.
See [Encryption](encryption.md) for details.

### EXIF (images)

| Key   | EXIF Tag           | Description                      |
|-------|--------------------|----------------------------------|
| `imk` | `Make`             | Camera manufacturer              |
| `imd` | `Model`            | Camera model                     |
| `idt` | `DateTimeOriginal` | Original capture timestamp       |

These fields are extracted automatically from image EXIF data when files are added.

### Audio

| Key   | ID3/Vorbis field | Description  |
|-------|------------------|--------------|
| `atl` | `Title`          | Track title  |
| `aar` | `Artist`         | Artist name  |
| `aal` | `Album`          | Album name   |
| `agn` | `Genre`          | Genre        |

These fields are extracted automatically from audio tags when files are added.

## Use in TUI Search

The `inspect` TUI allows filtering files by extra fields using the `s` key and the
`alias:value` syntax. Available aliases:

| Alias    | Internal key |
|----------|--------------|
| `artist` | `aar`        |
| `album`  | `aal`        |
| `title`  | `atl`        |
| `genre`  | `agn`        |
| `make`   | `imk`        |
| `model`  | `imd`        |
| `date`   | `idt`        |

See [inspect command](commands/inspect.md) for details.

