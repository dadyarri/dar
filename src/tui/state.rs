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
    pub preview_open: bool,
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
}
