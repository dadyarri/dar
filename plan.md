# DARI cleanup roadmap

## Step 15. Add extract functionality to the TUI (inspect command)

TUI should be capable of extracting single hovered file or entire directory to chosen path by pressing `x` key. This
should open small floating window with path input field, by default filled with PWD of current process (ensure this is
cross-platform between linux/macos and windows)

--

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
