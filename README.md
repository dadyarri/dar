# dar {WIP}

CLI utility to compress files and folders respecting .gitignore and automatically choosing compression algorithm based on file's type

## Usage

- `cargo run -- create -f out.dar src/` — create a brand new archive
- `cargo run -- append -f out.dar assets/` — append additional files/directories into an existing archive

