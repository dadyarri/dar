# completions

Prints a shell completion script to stdout.

## Synopsis

```sh
dari completions <SHELL>
```

Supported values: `bash`, `elvish`, `fish`, `powershell`, `zsh`.

## Examples

```sh
source <(dari completions bash)
dari completions fish > ~/.config/fish/completions/dari.fish
dari completions zsh > ~/.zsh/completions/_dari
```
