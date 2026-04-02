# completions

The `completions` command generates a shell completion script for the specified shell
and writes it to stdout.

## Usage

```sh
dari completions <SHELL>
```

## Arguments

| Argument | Description                                    |
|----------|------------------------------------------------|
| `SHELL`  | The shell to generate completions for          |

Supported values: `bash`, `elvish`, `fish`, `powershell`, `zsh`.

## Examples

### Bash

```sh
# Source directly in the current session:
source <(dari completions bash)

# Save for permanent use:
dari completions bash > ~/.bash_completion.d/dari
```

### Fish

```sh
dari completions fish > ~/.config/fish/completions/dari.fish
```

### Zsh

```sh
dari completions zsh > ~/.zsh/completions/_dari
```

### PowerShell

```sh
dari completions powershell | Out-String | Invoke-Expression
```

### Elvish

```sh
dari completions elvish >> ~/.config/elvish/rc.elv
```


