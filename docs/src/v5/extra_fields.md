# v5 Extra Fields

Each v5 entry may carry an `extra` string:

```text
key=value;key=value
```

It is used for:

- encryption metadata
- image metadata (for example EXIF camera fields)
- audio metadata (artist, album, title, genre)

The format is UTF-8 text stored immediately after the path bytes in the embedded index.
