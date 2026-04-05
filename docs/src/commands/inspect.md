# inspect

Opens an interactive TUI browser for a `.dar` archive's contents.

## Synopsis

```
dari inspect -f <file> [options]
dari -i -f <file> [options]
```

## Options

| Flag                                | Description                                      |
|-------------------------------------|--------------------------------------------------|
| `-f`, `--file <file>`               | Path to the archive (required)                   |
| `--encrypt-passphrase <PASSPHRASE>` | Passphrase for previewing encrypted file content |
| `-h`, `--help`                      | Show help                                        |

## TUI Interface

On launch an interactive screen opens showing the archive's file tree. Directories can
be expanded and collapsed; files can be previewed.

### Navigation and Controls

| Key                  | Action                                                 |
|----------------------|--------------------------------------------------------|
| `↑` / `k`            | Move cursor up                                         |
| `↓` / `j`            | Move cursor down                                       |
| `Enter` / `Space`    | Expand / collapse a directory                          |
| `m`                  | Open / switch to / close the **Metadata** panel        |
| `c`                  | Open / switch to / close the **Content** preview panel |
| `Esc`                | Close the active preview panel                         |
| `/`                  | Activate fuzzy filename search                         |
| `s`                  | Activate metadata search (`tag:value` syntax)          |
| `q` / `Q` / `Ctrl+C` | Quit                                                   |

### Scrolling the Preview Panel

When a preview panel has focus the following keys scroll its content:

| Key        | Action               |
|------------|----------------------|
| `↑`        | Scroll up one line   |
| `↓`        | Scroll down one line |
| `PageUp`   | Scroll up 10 lines   |
| `PageDown` | Scroll down 10 lines |

### Filename Search (`/`)

Pressing `/` activates a search bar at the bottom of the screen. As you type, the file
list is filtered using a fuzzy algorithm (nucleo_matcher). Pressing `Enter` locks the
filter; `Esc` clears it and restores the full list.

### Metadata Search (`s`)

Pressing `s` activates metadata field search. Query syntax:

```
tag:value
```

Multiple conditions separated by spaces are combined with AND logic:

```
artist:Pink Floyd album:Wall
```

#### Available Tags

| Alias    | Internal key | Description                |
|----------|--------------|----------------------------|
| `artist` | `aar`        | Artist (audio)             |
| `album`  | `aal`        | Album (audio)              |
| `title`  | `atl`        | Track title (audio)        |
| `genre`  | `agn`        | Genre (audio)              |
| `make`   | `imk`        | Camera manufacturer (EXIF) |
| `model`  | `imd`        | Camera model (EXIF)        |
| `date`   | `idt`        | Capture date (EXIF)        |

Pressing `Esc` clears the search query and restores the full list.

### Powerline / Nerd Font Icons

**dari** automatically detects whether the terminal supports Powerline / Nerd Font glyphs
and uses them for file and directory icons. Detection order (first match wins):

1. `DARI_ICONS=1` (or `true`, `yes`) — explicit enable; any other value disables.
2. `WEZTERM_EXECUTABLE` env var present → WezTerm detected.
3. `KITTY_WINDOW_ID` env var present → Kitty detected.
4. `TERM_PROGRAM` ∈ `{iTerm.app, WezTerm, Hyper}`.
5. `TERM` contains `kitty` or `alacritty`.

If none of the conditions match, plain ASCII fallback symbols (`▼` / `▶`) are used.

## Examples

```sh
# Open the TUI browser
dari inspect -f out.dar

# Open with support for previewing encrypted files
dari inspect -f out.dar --encrypt-passphrase "secret"
```


