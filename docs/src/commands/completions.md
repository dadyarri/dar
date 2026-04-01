# completions

Команда `completions` генерирует скрипт автодополнения для указанной оболочки и выводит его в stdout.

## Использование

```sh
dari completions <SHELL>
```

## Аргументы

| Аргумент | Описание                                              |
|----------|-------------------------------------------------------|
| `SHELL`  | Оболочка для генерации автодополнения                 |

Поддерживаемые значения: `bash`, `elvish`, `fish`, `powershell`, `zsh`.

## Примеры

### Bash

```sh
# Подключить напрямую в текущей сессии:
source <(dari completions bash)

# Сохранить для постоянного использования:
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
