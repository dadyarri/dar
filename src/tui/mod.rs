pub mod icons;
pub mod meta_search;
pub mod preview;
pub mod render_list;
pub mod render_preview;
pub mod render_status;
pub mod search;
pub mod state;
pub mod tree;

use crate::tui::{
    meta_search::{apply_meta_filter, parse_meta_query},
    preview::build_preview,
    search::apply_fuzzy_filter,
    state::{AppState, Focus, PreviewMode},
    tree as tui_tree,
};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use eyre::Result;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::panic;

pub struct App;

impl App {
    pub fn run(mut state: AppState) -> Result<()> {
        // Restore terminal even if the user's code panics.
        let original_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            original_hook(info);
        }));

        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = run_loop(&mut terminal, &mut state);

        // Always restore the terminal before returning.
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        result
    }
}

// ---------------------------------------------------------------------------
// Event loop
// ---------------------------------------------------------------------------

fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    state: &mut AppState,
) -> Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, state))?;

        if let Event::Key(key) = event::read()? {
            // Only react to key-press events; ignore repeat/release.
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // ── Meta-search mode ───────────────────────────────────────────
            if state.meta_search_active {
                match key.code {
                    KeyCode::Esc => {
                        state.meta_search_query.clear();
                        state.meta_search_active = false;
                        state.meta_search_error = None;
                        state.visible = tui_tree::flatten_visible(&state.tree_root);
                        state.table_state.select(if state.visible.is_empty() {
                            None
                        } else {
                            Some(0)
                        });
                    }
                    KeyCode::Enter => {
                        state.meta_search_active = false;
                        state.meta_search_error = None;
                        if state.table_state.selected().is_none() && !state.visible.is_empty() {
                            state.table_state.select(Some(0));
                        }
                    }
                    KeyCode::Backspace => {
                        state.meta_search_query.pop();
                        rebuild_visible_from_meta_search(state);
                    }
                    KeyCode::Char(c) => {
                        state.meta_search_query.push(c);
                        rebuild_visible_from_meta_search(state);
                    }
                    _ => {}
                }
                continue;
            }

            // ── Search mode ────────────────────────────────────────────────
            if state.search_active {
                match key.code {
                    KeyCode::Esc => {
                        // Clear query, deactivate search, restore full tree.
                        state.search_query.clear();
                        state.search_active = false;
                        state.visible = tui_tree::flatten_visible(&state.tree_root);
                        state.table_state.select(if state.visible.is_empty() {
                            None
                        } else {
                            Some(0)
                        });
                    }
                    KeyCode::Enter => {
                        // Keep current filtered view, close input box.
                        state.search_active = false;
                        if state.table_state.selected().is_none() && !state.visible.is_empty() {
                            state.table_state.select(Some(0));
                        }
                    }
                    KeyCode::Backspace => {
                        state.search_query.pop();
                        rebuild_visible_from_search(state);
                    }
                    KeyCode::Char(c) => {
                        state.search_query.push(c);
                        rebuild_visible_from_search(state);
                    }
                    _ => {}
                }
                continue;
            }

            match (key.code, key.modifiers) {
                (KeyCode::Char('q'), _)
                | (KeyCode::Char('Q'), _)
                | (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,

                // Preview navigation (scroll) when a preview window is focused.
                (KeyCode::Up, _) if state.focus == Focus::Preview => {
                    scroll_preview_up(state, 1);
                }
                (KeyCode::Down, _) if state.focus == Focus::Preview => {
                    scroll_preview_down(state, 1);
                }
                (KeyCode::PageUp, _) if state.focus == Focus::Preview => {
                    scroll_preview_up(state, 10);
                }
                (KeyCode::PageDown, _) if state.focus == Focus::Preview => {
                    scroll_preview_down(state, 10);
                }

                // List navigation (works even while a preview window is open).
                (KeyCode::Up, _) | (KeyCode::Char('k'), _) => move_up(state),
                (KeyCode::Down, _) | (KeyCode::Char('j'), _) => move_down(state),

                (KeyCode::Enter, _) | (KeyCode::Char(' '), _) => toggle_at_cursor(state),

                // 'm': open/switch-to/close the metadata floating window.
                (KeyCode::Char('m'), _) => open_or_switch_preview(state, PreviewMode::Metadata),

                // 'c': open/switch-to/close the content floating window.
                (KeyCode::Char('c'), _) => open_or_switch_preview(state, PreviewMode::Content),

                // Esc closes whichever preview window is open.
                (KeyCode::Esc, _) if state.preview_mode != PreviewMode::Closed => {
                    close_preview(state);
                }

                // '/' activates fuzzy search.
                (KeyCode::Char('/'), _) => {
                    state.search_active = true;
                    // Don't clear an existing query — let the user see the
                    // previous filter and extend/delete it.
                }

                // 's' activates metadata search.
                (KeyCode::Char('s'), _) => {
                    // Close filename search if it was open.
                    state.search_active = false;
                    state.meta_search_active = true;
                    state.meta_search_error = None;
                }

                _ => {}
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// State mutations
// ---------------------------------------------------------------------------

fn move_up(state: &mut AppState) {
    if state.visible.is_empty() {
        return;
    }
    let new = state
        .table_state
        .selected()
        .map(|i| i.saturating_sub(1))
        .unwrap_or(0);
    state.table_state.select(Some(new));
    if state.preview_mode != PreviewMode::Closed {
        refresh_preview(state);
    }
}

fn move_down(state: &mut AppState) {
    let n = state.visible.len();
    if n == 0 {
        return;
    }
    let new = state
        .table_state
        .selected()
        .map(|i| (i + 1).min(n - 1))
        .unwrap_or(0);
    state.table_state.select(Some(new));
    if state.preview_mode != PreviewMode::Closed {
        refresh_preview(state);
    }
}

/// Toggle expand/collapse on the currently selected directory node, then
/// rebuild `visible` and keep the cursor on the same node.
fn toggle_at_cursor(state: &mut AppState) {
    let Some(idx) = state.table_state.selected() else {
        return;
    };
    let Some(flat) = state.visible.get(idx) else {
        return;
    };
    if !flat.is_dir {
        return;
    }

    let full_path = flat.full_path.clone();
    tui_tree::toggle_expanded(&mut state.tree_root, &full_path);
    state.visible = tui_tree::flatten_visible(&state.tree_root);

    // The toggled dir node is still visible after the toggle — find it again.
    let new_idx = state
        .visible
        .iter()
        .position(|n| n.full_path == full_path)
        .unwrap_or(0)
        .min(state.visible.len().saturating_sub(1));
    state.table_state.select(Some(new_idx));
}

/// Open the requested preview mode, switch to it if another is open, or
/// close it if it is already active (toggle).  Pressing on a directory does
/// nothing.
fn open_or_switch_preview(state: &mut AppState, mode: PreviewMode) {
    // Toggle off if already in this mode.
    if state.preview_mode == mode {
        close_preview(state);
        return;
    }

    // Require a file entry to be selected.
    let Some(idx) = state.table_state.selected() else {
        return;
    };
    let Some(flat) = state.visible.get(idx).cloned() else {
        return;
    };
    if flat.is_dir || flat.entry_idx.is_none() {
        return;
    }
    let entry_idx = flat.entry_idx.unwrap();

    if state.preview_mode == PreviewMode::Closed {
        // Opening fresh: build cache and reset scroll.
        build_and_cache_preview(state, entry_idx);
    } else {
        // Switching between Metadata ↔ Content: cache is valid, just reset scroll.
        state.preview_scroll = 0;
        state.preview_line_count = 0;
        state.preview_viewport_height = 0;
    }

    state.preview_mode = mode;
    state.focus = Focus::Preview;
}

/// Close whichever preview window is open and return focus to the list.
fn close_preview(state: &mut AppState) {
    state.preview_mode = PreviewMode::Closed;
    state.focus = Focus::List;
    state.preview_scroll = 0;
    state.preview_line_count = 0;
    state.preview_viewport_height = 0;
    // Keep the cache so it can be reused if the user reopens the same entry.
}

/// Rebuild `visible` from the current `search_query` and reset the cursor.
fn rebuild_visible_from_search(state: &mut AppState) {
    state.visible = apply_fuzzy_filter(&state.search_query, &state.tree_root);
    state.table_state.select(if state.visible.is_empty() {
        None
    } else {
        Some(0)
    });
}

/// Parse `meta_search_query` and rebuild `visible`; store any parse error.
fn rebuild_visible_from_meta_search(state: &mut AppState) {
    match parse_meta_query(&state.meta_search_query) {
        Ok(predicates) => {
            state.meta_search_error = None;
            state.visible = apply_meta_filter(&predicates, &state.entries, &state.tree_root);
        }
        Err(e) => {
            state.meta_search_error = Some(e);
            // Keep the previous visible list so the display isn't jarring.
        }
    }
    state.table_state.select(if state.visible.is_empty() {
        None
    } else {
        Some(0)
    });
}

/// Rebuild the preview cache for the currently selected entry.
/// Automatically closes the preview window if a directory is selected.
fn refresh_preview(state: &mut AppState) {
    let Some(idx) = state.table_state.selected() else {
        return;
    };
    let Some(flat) = state.visible.get(idx).cloned() else {
        return;
    };
    if flat.is_dir || flat.entry_idx.is_none() {
        // Directories have no preview — close the floating window.
        close_preview(state);
        state.preview_cache = None;
        return;
    }
    let entry_idx = flat.entry_idx.unwrap();
    // Reuse the cache if it already holds this entry.
    if let Some((cached_idx, _)) = &state.preview_cache {
        if *cached_idx == entry_idx {
            return;
        }
    }
    build_and_cache_preview(state, entry_idx);
}

/// Read and decode the entry at `entry_idx`, storing the result in the cache.
fn build_and_cache_preview(state: &mut AppState, entry_idx: usize) {
    let preview = {
        let archive_path = state.archive_path.as_path();
        let passphrase = state.passphrase.as_deref();
        let locale = state.locale.as_str();
        let entry = &state.entries[entry_idx];
        let all_entries = state.entries.as_slice();
        build_preview(archive_path, entry, all_entries, passphrase, locale)
    };
    state.preview_cache = Some((entry_idx, preview));
    state.preview_scroll = 0;
    state.preview_line_count = 0;
    state.preview_viewport_height = 0;
}

fn scroll_preview_up(state: &mut AppState, lines: u16) {
    state.preview_scroll = state.preview_scroll.saturating_sub(lines);
}

fn scroll_preview_down(state: &mut AppState, lines: u16) {
    let max_scroll = state
        .preview_line_count
        .saturating_sub(state.preview_viewport_height);
    state.preview_scroll = state.preview_scroll.saturating_add(lines).min(max_scroll);
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn draw(frame: &mut ratatui::Frame, state: &mut AppState) {
    use ratatui::layout::{Constraint, Layout, Rect};
    use ratatui::widgets::Clear;

    // Top area = main content, bottom row = status bar.
    let vert = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).split(frame.area());
    let (main_area, status_area) = (vert[0], vert[1]);

    // The list always fills the full main area; a preview floats on top.
    // When meta search is active, split horizontally: list on the left,
    // tag-search help panel on the right.
    let (list_area, meta_help_area): (Rect, Option<Rect>) = if state.meta_search_active {
        let chunks =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(34)]).split(main_area);
        (chunks[0], Some(chunks[1]))
    } else {
        (main_area, None)
    };
    let preview_area: Option<Rect> = if state.preview_mode != PreviewMode::Closed {
        Some(render_preview::centered_popup_rect(96, 94, list_area))
    } else {
        None
    };

    // ── File-tree table ────────────────────────────────────────────────────
    render_list::draw_list(frame, list_area, state);

    // ── Meta-search help panel (right split) ───────────────────────────────
    if let Some(help_area) = meta_help_area {
        let locale = state.locale.as_str().to_owned();
        render_preview::render_meta_search_help_panel(
            frame,
            help_area,
            &state.meta_search_error,
            &locale,
        );
    }

    // ── Preview floating window ────────────────────────────────────────────
    if let Some(area) = preview_area {
        frame.render_widget(Clear, area);
        match state.preview_mode {
            PreviewMode::Metadata => render_preview::render_metadata_panel(frame, area, state),
            PreviewMode::Content => render_preview::render_content_panel(frame, area, state),
            PreviewMode::Closed => {}
        }
    }

    // ── Status bar ─────────────────────────────────────────────────────────
    render_status::draw_status_bar(frame, status_area, state);
}
