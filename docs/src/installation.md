# Installation

## Building from Source

Building **dari** requires the Rust compiler (version 1.70 or higher) and Cargo.

```sh
git clone https://github.com/dadyarri/dar
cd dar
cargo build --release
```

After a successful build the binary is located at `target/release/dari`.

Copy it to any directory on your `PATH`:

```sh
cp target/release/dari ~/.local/bin/dari
```

## Shell Completion Scripts

Completion scripts are generated at runtime using the `dari completions <SHELL>` subcommand.

Supported shells: `bash`, `elvish`, `fish`, `powershell`, `zsh`.

### Bash

```sh
dari completions bash > ~/.bash_completion.d/dari
# or source directly in the current session:
source <(dari completions bash)
```

### Fish

```sh
dari completions fish > ~/.config/fish/completions/dari.fish
```

### Zsh

```sh
dari completions zsh > ~/.zsh/completions/_dari
# Make sure the directory is listed in fpath
```

### PowerShell

```sh
dari completions powershell | Out-String | Invoke-Expression
```

### Elvish

```sh
dari completions elvish >> ~/.config/elvish/rc.elv
```
