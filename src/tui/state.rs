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

/// State for the fuzzy filename search bar (`/`).
pub struct SearchState {
    /// Current text typed into the search bar; empty when inactive.
    pub query: String,
    /// Whether the `/` search input box is currently open and receiving input.
    pub active: bool,
}

/// State for the metadata tag search bar (`s`).
pub struct MetaSearchState {
    /// Current text typed into the metadata search bar; empty when inactive.
    pub query: String,
    /// Whether the `s` metadata search input box is currently open and receiving input.
    pub active: bool,
    /// Parse error from the last metadata search input attempt, if any.
    pub error: Option<String>,
}

/// State for the extract-to-path floating dialog (`x`).
pub struct ExtractDialog {
    /// Whether the dialog is currently open.
    pub active: bool,
    /// Destination path typed in the dialog.
    pub path: String,
    /// Resolved absolute version of `path` (set when the user types a relative path
    /// that exists on disk; `None` when the path is already absolute or is invalid).
    pub resolved: Option<PathBuf>,
    /// Error produced by the last extraction attempt (displayed inside the dialog).
    pub error: Option<String>,
}

/// State for the floating preview window (metadata / content panels).
pub struct PreviewState {
    /// Which preview window (if any) is currently open.
    pub mode: PreviewMode,
    /// Which panel currently receives key input.
    pub focus: Focus,
    /// Vertical scroll offset (in lines) for the preview panel.
    pub scroll: u16,
    /// Total logical lines in the last-rendered preview content (updated each frame).
    pub line_count: u16,
    /// Usable height of the preview viewport in terminal rows (updated each frame).
    pub viewport_height: u16,
    /// Cached preview for the last opened file: `(entry_idx, preview)`.
    pub cache: Option<(usize, EntryPreview)>,
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
    /// Whether the terminal supports Powerline / Nerd Font glyphs (detected once at startup).
    pub powerline: bool,
    /// Root of the collapsible directory tree built from `entries`.
    pub tree_root: TreeNode,
    /// Currently visible (flattened) rows; rebuilt whenever a node is toggled.
    pub visible: Vec<FlatNode>,
    /// Drives the ratatui `Table` widget; cursor indexes into `visible`.
    pub table_state: TableState,
    /// Fuzzy filename search state.
    pub search: SearchState,
    /// Metadata tag search state.
    pub meta_search: MetaSearchState,
    /// Extract-to-path dialog state.
    pub extract: ExtractDialog,
    /// Floating preview window state (mode, scroll, cache).
    pub preview: PreviewState,
}
