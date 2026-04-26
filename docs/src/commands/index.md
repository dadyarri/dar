# Commands

`dari` provides the following subcommands:

| Command | Short flag | Purpose |
|---------|------------|---------|
| [`create`](create.md) | `-c` | Create a new archive |
| [`append`](append.md) | `-a` | Add new content to an existing archive |
| [`incremental`](incremental.md) | — | Append only files newer than a cutoff |
| [`extract`](extract.md) | `-x` | Extract all files or a selected subset |
| [`list`](list.md) | `-l` | List archive contents in table or JSON form |
| [`inspect`](inspect.md) | `-i` | Browse archive contents in the TUI |
| [`verify`](verify.md) | — | Check sidecars, stored bytes, and payload integrity |
| [`encrypt`](encrypt.md) | `-e` | Rebuild an existing archive as encrypted |
| [`reindex`](reindex.md) | — | Rebuild a v6 `.dari` external index |
| [`migrate`](migrate.md) | — | Convert a v5 archive to v6 |
| [`completions`](completions.md) | — | Emit shell completion scripts |

## Global Flags

| Flag | Meaning |
|------|---------|
| `-h`, `--help` | Show help |
| `-V`, `--version` | Show the program version |
