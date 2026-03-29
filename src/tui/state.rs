use crate::i18n::Locale;
use crate::models::archive::ArchiveIndexEntryWrapper;
use ratatui::widgets::TableState;
use std::path::PathBuf;

/// Application state passed into `App::run` for the inspect TUI.
pub struct AppState {
    /// Path to the `.dar` file being inspected (used for the title bar).
    pub archive_path: PathBuf,
    /// All parsed index entries from the archive.
    pub entries: Vec<ArchiveIndexEntryWrapper>,
    /// Passphrase supplied on the CLI, forwarded to the extractor when needed.
    pub passphrase: Option<String>,
    /// Active locale for all TUI translations.
    pub locale: Locale,
    /// Drives the ratatui `Table` widget — tracks the selected row (cursor)
    /// and the scroll offset so the highlighted row is always visible.
    pub table_state: TableState,
}
