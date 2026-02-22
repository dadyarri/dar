/// UI symbols that adapt based on terminal capabilities
#[derive(Clone, Debug)]
pub struct TerminalSymbols {
    pub expanded_dir: String,
    pub collapsed_dir: String,
    pub file: String,
    pub selected: String,
    pub unselected: String,
    #[allow(dead_code)]
    pub has_nerd_font: bool,
}

impl TerminalSymbols {
    pub fn detect() -> Self {
        // Try to detect if terminal supports nerd fonts
        // For now, we check TERM environment variable
        let has_nerd_font = Self::has_nerd_font_support();

        if has_nerd_font {
            Self {
                expanded_dir: " ".to_string(),
                collapsed_dir: " ".to_string(),
                file: "󰈔 ".to_string(),
                selected: "󰱒 ".to_string(),
                unselected: "󰄱 ".to_string(),
                has_nerd_font: true,
            }
        } else {
            Self {
                expanded_dir: "▼ ".to_string(),
                collapsed_dir: "▶ ".to_string(),
                file: "  ".to_string(),
                selected: "■ ".to_string(),
                unselected: "□ ".to_string(),
                has_nerd_font: false,
            }
        }
    }

    fn has_nerd_font_support() -> bool {
        // Check common environment variables that indicate nerd font support
        let font_env = std::env::var("ITERM_PROFILE")
            .or_else(|_| std::env::var("VTE_VERSION"))
            .or_else(|_| std::env::var("KITTY_WINDOW_ID"))
            .or_else(|_| std::env::var("ALACRITTY_SOCKET"))
            .or_else(|_| std::env::var("TERM_PROGRAM"))
            .unwrap_or_default();

        // Check if the font name contains indicators of nerd font support
        let has_indicator = font_env.to_lowercase().contains("nerd")
            || font_env.to_lowercase().contains("powerline")
            || std::env::var("NERD_FONT").is_ok();

        // Also check TERM variable for common terminals known to support nerd fonts
        let term = std::env::var("TERM").unwrap_or_default().to_lowercase();
        let is_modern_terminal = term.contains("xterm-256")
            || term.contains("alacritty")
            || term.contains("kitty")
            || term.contains("iterm");

        has_indicator || is_modern_terminal
    }
}

impl Default for TerminalSymbols {
    fn default() -> Self {
        Self::detect()
    }
}
