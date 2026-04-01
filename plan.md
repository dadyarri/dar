# DARI cleanup roadmap

## Step 1. Handle bare file paths in `walker::scan_files`

AGENTS.md explicitly flags this as a known gap: passing a single file path is silently ignored
because `src/walker.rs` only branches on `absolute_path.is_dir()`. Add an
`else if absolute_path.is_file()` branch that adds the file directly, so
`dari create -f out.dar somefile.rs` works as expected.

Files: `src/walker.rs`

---

## Step 2. Eliminate `unwrap` / `expect` in non-test code

`src/models/archive.rs` calls `get_unix_timestamp().unwrap()` inside `ArchiveHeader::new()`, and
`src/commands/inspect.rs` calls `.expect("--file is required")` — both violate the project's own
rule. Change `ArchiveHeader::new()` to return `Result<Self>`, propagate the error with `?`, and
replace the `expect` in `inspect.rs` with a proper `eyre::bail!` or `?`.

Files: `src/models/archive.rs`, `src/commands/inspect.rs`, `src/archive_builder.rs` (call site)

---

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

## Step 6. Parallelise file preparation in `append`

`src/commands/append.rs` uses a serial `builder.add_file()` loop, while `src/commands/create.rs`
uses `rayon::par_iter` + `prepare_file_from_disk` / `commit_prepared`. Apply the same
prepare/commit split to `append` so large appends also benefit from parallel I/O and compression.

Files: `src/commands/append.rs`, `src/archive_builder.rs`

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

## Step 12. Address the 4 GiB per-file and per-archive limit

`ArchiveIndexEntry::offset`, `original_size`, and `compressed_size` are all `u32` in
`src/models/archive.rs`, capping both individual file sizes and the entire data section at 4 GiB.
Upgrade to `u64`, bump the format version is not required, since the software is not used anywhere, and update all
read/write sites.

Files: `src/models/archive.rs`, `src/archive_builder.rs`, `src/reader.rs`, `src/extractor.rs`,
`src/pipeline.rs`, `src/commands/create.rs`, `src/commands/append.rs`

---

## Step 13. Decompose `tui/mod.rs` into focused render sub-modules

At ~1 200 lines, `src/tui/mod.rs` mixes the event loop, state-mutation functions, and all
rendering (list table, content panel, metadata panel, meta-search help panel, status bar).
Extract rendering into `src/tui/render_list.rs`, `src/tui/render_preview.rs`, and
`src/tui/render_status.rs` to improve navigability and reduce merge conflicts.

Files: `src/tui/mod.rs`, `src/tui/render_list.rs` (new), `src/tui/render_preview.rs` (new),
`src/tui/render_status.rs` (new)

---

## Step 14. Add `--dry-run` flag to `create` and `append`

Allow users to preview which files would be added and what compression would be applied, without
writing any output file. Implement by short-circuiting after the parallel prepare phase and
skipping the serial write/build phase.

Files: `src/cli.rs`, `src/commands/create.rs`, `src/commands/append.rs`,
`locales/en.toml`, `locales/ru.toml`

## Step 15. Add extract functionality to the TUI (inspect command)

TUI should be capable of extracting single hovered file or entire directory to chosen path by pressing `x` key. This
should open small floating window with path input field, by default filled with PWD of current process (ensure this is
cross-platform between linux/macos and windows) 