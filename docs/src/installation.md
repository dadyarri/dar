# Installation

## Build From Source

Building `dari` requires Rust 1.85 or newer and Cargo.

```sh
git clone https://github.com/dadyarri/dar
cd dar
cargo build --release
```

The binary will be available at:

```text
target/release/dari
```

To make it available on your `PATH`:

```sh
install -Dm755 target/release/dari ~/.local/bin/dari
```

## Shell Completions

Completion scripts are generated at runtime:

```sh
dari completions <SHELL>
```

Supported shells: `bash`, `elvish`, `fish`, `powershell`, `zsh`.

Examples:

```sh
dari completions bash > ~/.bash_completion.d/dari
dari completions fish > ~/.config/fish/completions/dari.fish
dari completions zsh > ~/.zsh/completions/_dari
```
