use crate::i18n::Locale;
use crate::models::archive::ArchiveIndexEntryWrapper;
use crate::tui::tree::{FlatNode, TreeNode};
use ratatui::widgets::TableState;
use std::path::PathBuf;

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
}
