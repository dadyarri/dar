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