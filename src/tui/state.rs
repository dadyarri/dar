use crate::i18n::Locale;
use crate::models::archive::ArchiveIndexEntryWrapper;
use crate::tui::preview::EntryPreview;
use crate::tui::tree::{FlatNode, TreeNode};
use ratatui::widgets::TableState;
use std::path::PathBuf;

/// Which panel currently owns keyboard focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Focus {
    List,
    Preview,
}

/// Which floating preview window (if any) is currently open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewMode {
    /// No preview window is open.
    Closed,
    /// Compression + extra-tag metadata window (opened with `m`).
    Metadata,
    /// File content window (opened with `c`).
    Content,
}

/// Application state passed into `App::run` for the inspect TUI.
pub struct AppState {
    /// Path to the `.dar` file being inspected (used for the title bar).
    pub archive_path: PathBuf,
    /// All parsed index entries from the archive (source of truth for metadata).
    pub entries: Vec<ArchiveIndexEntryWrapper>,
    /// Passphrase supplied on the CLI, forwarded to the extractor when needed.
    pub passphrase: Option<String>,
    /// Active locale for all TUI translations.
    pub locale: Locale,
    /// Root of the collapsible directory tree built from `entries`.
    pub tree_root: TreeNode,
    /// Currently visible (flattened) rows; rebuilt whenever a node is toggled.
    pub visible: Vec<FlatNode>,
    /// Drives the ratatui `Table` widget; cursor indexes into `visible`.
    pub table_state: TableState,
    /// Whether the preview panel is currently open.
    pub preview_mode: PreviewMode,
    /// Which panel currently receives key input.
    pub focus: Focus,
    /// Vertical scroll offset (in lines) for the preview panel.
    pub preview_scroll: u16,
    /// Total logical lines in the last-rendered preview content (updated each frame).
    pub preview_line_count: u16,
    /// Usable height of the preview viewport in terminal rows (updated each frame).
    pub preview_viewport_height: u16,
    /// Cached preview for the last opened file: `(entry_idx, preview)`.
    pub preview_cache: Option<(usize, EntryPreview)>,
    /// Current text typed into the fuzzy search bar; empty when inactive.
    pub search_query: String,
    /// Whether the `/` search input box is currently open and receiving input.
    pub search_active: bool,
    /// Whether the terminal supports Powerline / Nerd Font glyphs (detected once at startup).
    pub powerline: bool,
    /// Current text typed into the metadata search bar; empty when inactive.
    pub meta_search_query: String,
    /// Whether the `s` metadata search input box is currently open and receiving input.
    pub meta_search_active: bool,
    /// Parse error from the last metadata search input attempt, if any.
    pub meta_search_error: Option<String>,
    /// Whether the extract-to-path dialog is currently open.
    pub extract_active: bool,
    /// Destination path typed in the extract dialog.
    pub extract_path: String,
    /// Error produced by the last extraction attempt (displayed inside the dialog).
    pub extract_error: Option<String>,
}
