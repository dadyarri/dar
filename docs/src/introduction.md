# Introduction

There are many archiving tools available. **dari** is built primarily for archiving source code
with automatic exclusion of build artifacts, along with a set of additional features designed
to reliably preserve data over the long term.

Key features:

- **Automatic compression selection** — the best algorithm is chosen based on file extension.
- **Encryption** — file data is encrypted with ChaCha20-Poly1305 using a passphrase.
- **Deduplication** — files with identical content are stored only once per archive.
- **`.gitignore` / `.darignore` support** — build artifacts and unwanted files are excluded
  automatically.
- **Interactive inspector** — a terminal TUI (`inspect`) lets you browse, preview, and search
  archive contents without extracting.
