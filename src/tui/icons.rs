/// Detect whether the current terminal likely supports Powerline / Nerd Font glyphs.
///
/// Detection order (first match wins):
/// 1. `DARI_ICONS=1|true|yes` — explicit opt-in; any other value is opt-out.
/// 2. `WEZTERM_EXECUTABLE` present → WezTerm detected.
/// 3. `KITTY_WINDOW_ID` present → kitty detected.
/// 4. `TERM_PROGRAM` ∈ `{iTerm.app, WezTerm, Hyper}`.
/// 5. `TERM` contains `"kitty"` or `"alacritty"`.
pub fn detect_powerline() -> bool {
    if let Ok(val) = std::env::var("DARI_ICONS") {
        return matches!(val.to_lowercase().as_str(), "1" | "true" | "yes");
    }
    if std::env::var("WEZTERM_EXECUTABLE").is_ok() {
        return true;
    }
    if std::env::var("KITTY_WINDOW_ID").is_ok() {
        return true;
    }
    if let Ok(tp) = std::env::var("TERM_PROGRAM") {
        if matches!(tp.as_str(), "iTerm.app" | "WezTerm" | "Hyper") {
            return true;
        }
    }
    if let Ok(term) = std::env::var("TERM") {
        if term.contains("kitty") || term.contains("alacritty") {
            return true;
        }
    }
    false
}

/// Icon string (glyph + trailing space) for a directory node.
///
/// Powerline/Nerd Font glyphs: `\u{F07C}` (open) / `\u{F07B}` (closed).
/// Fallback: `▼ ` / `▶ `.
pub fn folder_icon(expanded: bool, powerline: bool) -> &'static str {
    if powerline {
        if expanded { "\u{F07C} " } else { "\u{F07B} " }
    } else {
        if expanded { "▼ " } else { "▶ " }
    }
}

/// Icon string (glyph + trailing space) for a file node, chosen by extension.
///
/// Returns `"  "` (two spaces) when `powerline` is `false` so column widths are
/// preserved with the existing fallback symbols.
pub fn file_icon(ext: &str, powerline: bool) -> &'static str {
    if !powerline {
        return "  ";
    }
    match ext.to_ascii_lowercase().as_str() {
        // ── Systems / compiled ─────────────────────────────────────────────
        "rs" => "\u{E7A8} ",                                 // Rust
        "c" | "h" => "\u{E61E} ",                            // C
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => "\u{E61D} ", // C++
        "cs" => "\u{E7B2} ",                                 // C#
        "go" => "\u{E724} ",                                 // Go
        "java" | "class" | "jar" => "\u{E738} ",             // Java
        "kt" | "kts" => "\u{E634} ",                         // Kotlin
        "swift" => "\u{E755} ",                              // Swift
        "rb" => "\u{E739} ",                                 // Ruby
        "php" => "\u{E73D} ",                                // PHP

        // ── Scripted ───────────────────────────────────────────────────────
        "py" | "pyi" | "pyw" => "\u{E606} ", // Python
        "js" | "mjs" | "cjs" => "\u{E60C} ", // JavaScript
        "ts" | "mts" | "cts" => "\u{E628} ", // TypeScript
        "sh" | "bash" | "zsh" | "fish" | "ksh" => "\u{F489} ", // Shell

        // ── Web ────────────────────────────────────────────────────────────
        "html" | "htm" => "\u{F13B} ",  // HTML
        "css" => "\u{E749} ",           // CSS
        "scss" | "sass" => "\u{E603} ", // Sass

        // ── Data / config ──────────────────────────────────────────────────
        "json" | "jsonc" => "\u{E60B} ",                    // JSON
        "toml" => "\u{E615} ",                              // TOML
        "yaml" | "yml" => "\u{E8EB} ",                      // YAML
        "xml" => "\u{E619} ",                               // XML
        "sql" | "db" | "sqlite" | "sqlite3" => "\u{F1C0} ", // Database
        "csv" => "\u{F1C3} ",                               // Spreadsheet
        "log" => "\u{F4ED} ",                               // Log
        "lock" => "\u{F023} ",                              // Lock

        // ── Documents ─────────────────────────────────────────────────────
        "md" | "mdx" | "markdown" => "\u{F48A} ", // Markdown
        "txt" => "\u{F15C} ",                     // Text
        "pdf" => "\u{F1C1} ",                     // PDF
        "doc" | "docx" | "odt" => "\u{F1C2} ",    // Word
        "xls" | "xlsx" | "ods" => "\u{F1C3} ",    // Spreadsheet
        "ppt" | "pptx" | "odp" => "\u{F1C4} ",    // Presentation

        // ── Media ─────────────────────────────────────────────────────────
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico" | "webp" | "tiff" | "svg" => "\u{F1C5} ", // Image
        "mp3" | "flac" | "wav" | "ogg" | "aac" | "m4a" | "opus" => "\u{F1C7} ", // Audio
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" => "\u{F1C8} ",  // Video

        // ── Archives ──────────────────────────────────────────────────────
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "zst" | "dar" => "\u{F1C6} ", // Archive

        // ── Fallback ──────────────────────────────────────────────────────
        _ => "\u{F15B} ", // Generic file
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_fallback_symbols() {
        assert_eq!(folder_icon(true, false), "▼ ");
        assert_eq!(folder_icon(false, false), "▶ ");
    }

    #[test]
    fn folder_powerline_symbols() {
        assert_eq!(folder_icon(true, true), "\u{F07C} ");
        assert_eq!(folder_icon(false, true), "\u{F07B} ");
    }

    #[test]
    fn file_fallback_is_two_spaces() {
        assert_eq!(file_icon("rs", false), "  ");
        assert_eq!(file_icon("unknown", false), "  ");
    }

    #[test]
    fn known_extensions_have_icons() {
        let known = [
            "rs", "py", "js", "ts", "go", "md", "json", "toml", "png", "mp3", "mp4", "zip", "pdf",
            "html", "css", "sh",
        ];
        for ext in &known {
            let icon = file_icon(ext, true);
            assert_ne!(icon, "\u{F15B} ", "expected specific icon for .{ext}");
        }
    }

    #[test]
    fn unknown_extension_gets_generic_icon() {
        assert_eq!(file_icon("xyz123", true), "\u{F15B} ");
        assert_eq!(file_icon("", true), "\u{F15B} ");
    }
}
