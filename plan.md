# DARI cleanup roadmap

## Step 3. Localize remaining hard-coded English user-facing strings

Several user-visible strings bypass `t!()`:

- Compression method labels in `src/commands/create.rs` verbose mode ("stored", "brotli", "zstd", …)
- Ratio labels in `src/tui/mod.rs` ("no change", "N% saved", "N% larger")
- Panel header strings in `src/tui/mod.rs` (" No entry selected", " Tag Search ", " Metadata ", " Content ")
- Error messages in `src/tui/meta_search.rs` from `parse_meta_query`

Add required keys to `locales/en.toml` and `locales/ru.toml`, then replace the literals with
`t!(...)` calls.

Files: `src/commands/create.rs`, `src/tui/mod.rs`, `src/tui/meta_search.rs`,
`locales/en.toml`, `locales/ru.toml`

---

## Step 5. Deduplicate ChaCha20-Poly1305 decryption logic

`verify_passphrase_matches` in `src/commands/append.rs` re-implements the exact same key
derivation + nonce + tag-split + `decrypt_in_place_detached` dance already present in
`extractor::try_decrypt_bytes`. Replace the ~20 duplicated lines with a call to the shared
helper.

Files: `src/commands/append.rs`, `src/extractor.rs`

---

## Step 10. Add a non-interactive `list` subcommand

Add a scriptable `dari list -f archive.dar` (alias `ls`) that prints archive contents — path,
original size, stored size, compression method, checksum — to stdout in plain-text or JSON
(`--json` flag). This enables scripting without launching the TUI.

Files: `src/cli.rs`, `src/commands/list.rs` (new), `src/commands/mod.rs`, `src/main.rs`,
`locales/en.toml`, `locales/ru.toml`

---

## Step 11. Add creation/append completion summary

After `create` or `append` succeeds, print a summary line: files added, total original size,
total stored size, overall ratio, and time elapsed. Requires accumulating `FileAddOutcome` values
during the serial commit phase.

Files: `src/commands/create.rs`, `src/commands/append.rs`, `src/archive_builder.rs`,
`locales/en.toml`, `locales/ru.toml`

---

## Step 13. Decompose `tui/mod.rs` into focused render sub-modules

At ~1 200 lines, `src/tui/mod.rs` mixes the event loop, state-mutation functions, and all
rendering (list table, content panel, metadata panel, meta-search help panel, status bar).
Extract rendering into `src/tui/render_list.rs`, `src/tui/render_preview.rs`, and
`src/tui/render_status.rs` to improve navigability and reduce merge conflicts.

Files: `src/tui/mod.rs`, `src/tui/render_list.rs` (new), `src/tui/render_preview.rs` (new),
`src/tui/render_status.rs` (new)

---

## Step 15. Add extract functionality to the TUI (inspect command)

TUI should be capable of extracting single hovered file or entire directory to chosen path by pressing `x` key. This
should open small floating window with path input field, by default filled with PWD of current process (ensure this is
cross-platform between linux/macos and windows)

---

## Step 17. Add `encrypt` subcommand to encrypt an existing archive

Add a new `dari encrypt -f archive.dar` subcommand that re-encrypts an existing **unencrypted**
archive in-place (or to a new file via `-o`).

Because `.dar` stores compressed data blocks followed by an index, re-encryption requires
rewriting every data block — the archive is effectively rebuilt. The implementation should:

1. Parse the existing archive via `reader::load_archive`.
2. Reject the operation if the archive is already encrypted (return a clear error).
3. Accept `--encrypt` (interactive prompt) or `--encrypt-passphrase <PASSPHRASE>`.
4. Read each entry's data block, run it through `CompressionPipeline` with the supplied
   passphrase (skip the compression step — data is already compressed; only the encrypt stage
   runs), update `extra` fields with nonce/tag, set `INDEX_FLAG_ENCRYPTED_DATA`.
5. Write the rebuilt archive to a temporary file alongside the original, then atomically rename
   it over the original (or write to `-o` destination directly).
6. Add locale keys (`cli.encrypt.*`) to `locales/en.toml` and `locales/ru.toml`.

Files: `src/cli.rs`, `src/commands/encrypt.rs` (new), `src/commands/mod.rs`, `src/main.rs`,
`locales/en.toml`, `locales/ru.toml`

---

## Step 18. Handle path conflicts on `append`

When `dari append` is called and one or more of the new files would produce an archive-relative
path that already exists in the archive, the current behaviour is silent duplication (two index
entries with the same path). This should be configurable via a `--on-conflict` flag:

- `--on-conflict error` (default) — abort the entire operation with a clear error listing the
  conflicting paths.
- `--on-conflict rename` — automatically suffix the conflicting path with `-1`, `-2`, … (e.g.
  `assets/logo.png` → `assets/logo-1.png`) until a free name is found.
- `--on-conflict overwrite` — keep only the new file; discard the existing entry from the index
  (the old data block is left as dead bytes, since the format has no compaction step yet).

The conflict check happens in the serial commit phase of `ArchiveBuilder`, before writing the new
entry. `--dry-run` should also report which files would conflict and what the resolved path would
be under `rename` mode.

Files: `src/cli.rs`, `src/commands/append.rs`, `src/archive_builder.rs`,
`locales/en.toml`, `locales/ru.toml` 