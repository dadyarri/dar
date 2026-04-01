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

При каждой сборке (`cargo build`) файлы автодополнения для оболочек автоматически
записываются в директорию `completions/`:

| Файл                          | Оболочка    |
|-------------------------------|-------------|
| `completions/dari.bash`       | Bash        |
| `completions/dari.fish`       | Fish        |
| `completions/dari.zsh`        | Zsh         |
| `completions/_dari` (PowerShell) | PowerShell |

### Bash

```sh
source completions/dari.bash
# или скопируйте в /etc/bash_completion.d/dari
```

### Fish

```sh
cp completions/dari.fish ~/.config/fish/completions/
```

### Zsh

```sh
# Добавьте директорию completions/ в fpath или скопируйте файл:
cp completions/dari.zsh ~/.zsh/completions/_dari
```
