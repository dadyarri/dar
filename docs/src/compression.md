# Compression Algorithms

**dari** automatically selects a compression algorithm based on the file extension. The
selection happens at archive creation time and when adding files with `append`.

## Algorithms

### None (store raw)

The file is stored without modification. Used for formats that already contain built-in
compression or have high entropy.

Extensions: `jpg`, `jpeg`, `png`, `webp`, `gif`, `mp4`, `mp3`, `aac`, `ogg`, `flac`,
`wav`, `mkv`, `avi`, `mov`, `m4a`, `m4v`, `zip`, `gz`, `rar`, `7z`, `bz2`, `zst`,
`tar`, `bz`, `xz`, `lzma`, `lz4`, `lz`, `zlib`, `jar`, `war`, `ear`, `apk`, `ipa`,
`aab`, `whl`, `egg`, `nupkg`, `gem`, `pdf`, `docx`, `xlsx`, `pptx`, `odt`, `ods`,
`odp`, `epub`, `cbz`, `wasm`

### Brotli (quality 6)

Optimal for text-based web formats.

Extensions: `html`, `htm`, `xhtml`, `css`, `scss`, `sass`, `less`, `stylus`, `js`,
`mjs`, `json`, `svg`, `xml`, `xsl`, `xsd`, `txt`, `md`, `markdown`, `rst`, `toml`,
`yaml`, `yml`, `woff2`, `jsx`, `ts`, `tsx`, `mts`

### Zstandard (level 3)

Optimal for source code and structured data. Also used as the **default** for any
extension not listed elsewhere.

Extensions: `log`, `tar`, `csv`, `tsv`, `db`, `sql`, `bak`, `rs`, `go`, `java`, `kt`,
`py`, `rb`, `php`, `pl`, `pas`, `c`, `cpp`, `c++`, `h`, `hpp`, `cs`, `fs`, `vb`,
`vba`, `sh`, `bat`, `ps1`, `fish`, `proto`, `thrift`

### LZMA (level 9)

High compression ratio. Optimal for binary and specialised formats.

Extensions: `iso`, `img`, `bin`, `deb`, `rpm`, `pkg`, `vmdk`, `patch`, `diff`,
`fortran`, `f90`, `ada`, `lisp`, `scm`, `hs`, `erl`, `cmake`, `makefile`, `mk`,
`tex`, `bib`

## Image Optimisation (--compress-images)

The `--compress-images` flag enables additional compression for image files.
Without this flag images are stored without compression.

### PngOxipng

Optimises PNG files using the [oxipng](https://github.com/shssoichiro/oxipng) library.
The optimised file is stored only if it is smaller than the original; otherwise the
original bytes are kept. The stored `compression_method` is always `None` (PNG is its
own container format).

Extensions: `png`


## Fallback Rule

If the selected algorithm does not reduce the file size (e.g. for short or already-dense
files), **dari** automatically stores the original bytes with `compression_method = None`.
This guarantees the archive is never larger than the sum of the original files.

## Default Algorithm

For extensions not matched by any list, **Zstandard** (level 3) is used.
