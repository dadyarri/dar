# Установка

## Сборка из исходного кода

Для сборки **dari** необходим компилятор Rust (версия 1.70 или выше) и пакетный менеджер Cargo.

```sh
git clone https://github.com/dadyarri/dar
cd dar
cargo build --release
```

После успешной сборки бинарный файл будет находиться по пути `target/release/dari`.

Его можно скопировать в директорию, которая входит в `PATH`:

```sh
cp target/release/dari ~/.local/bin/dari
```

## Автодополнение командной строки

Скрипты автодополнения генерируются во время выполнения с помощью подкоманды `dari completions <SHELL>`.

Поддерживаемые оболочки: `bash`, `elvish`, `fish`, `powershell`, `zsh`.

### Bash

```sh
dari completions bash > ~/.bash_completion.d/dari
# или подключите напрямую:
source <(dari completions bash)
```

### Fish

```sh
dari completions fish > ~/.config/fish/completions/dari.fish
```

### Zsh

```sh
dari completions zsh > ~/.zsh/completions/_dari
# Убедитесь, что директория находится в fpath
```

### PowerShell

```sh
dari completions powershell | Out-String | Invoke-Expression
```

### Elvish

```sh
dari completions elvish >> ~/.config/elvish/rc.elv
```
