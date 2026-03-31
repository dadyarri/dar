pub mod meta_search;
pub mod preview;
pub mod search;
pub mod state;
pub mod tree;

use crate::tui::{
    meta_search::{apply_meta_filter, parse_meta_query, TAG_ALIASES_FULL},
    preview::{build_preview, PreviewContent},
    search::apply_fuzzy_filter,
    state::{AppState, Focus, PreviewMode},
    tree as tui_tree,
};
use crate::models::archive::CompressionMethod;
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
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

    let locale = state.locale.as_str().to_owned();

    // Top area = main content, bottom row = status bar.
    let vert = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).split(frame.area());
    let (main_area, status_area) = (vert[0], vert[1]);

    // The list always fills the full main area; a preview floats on top.
    // When meta search is active, split horizontally: list on the left,
    // tag-search help panel on the right.
    let (list_area, meta_help_area): (Rect, Option<Rect>) = if state.meta_search_active {
        let chunks = Layout::horizontal([Constraint::Fill(1), Constraint::Length(34)]).split(main_area);
        (chunks[0], Some(chunks[1]))
    } else {
        (main_area, None)
    };
    let preview_area: Option<Rect> = if state.preview_mode != PreviewMode::Closed {
        Some(centered_popup_rect(96, 94, list_area))
    } else {
        None
    };

    // ── File-tree table ────────────────────────────────────────────────────
    let col_file = rust_i18n::t!("tui.inspect.col_file", locale = locale);
    let col_size = rust_i18n::t!("tui.inspect.col_size", locale = locale);
    let col_compression = rust_i18n::t!("tui.inspect.col_compression", locale = locale);

    let header = Row::new(vec![
        Cell::from(col_file.as_ref()),
        Cell::from(col_size.as_ref()),
        Cell::from(col_compression.as_ref()),
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = state
        .visible
        .iter()
        .map(|flat| {
            let icon = if flat.is_dir {
                if flat.expanded { "▼ " } else { "▶ " }
            } else {
                "  "
            };
            let indent = "  ".repeat(flat.depth);

            let file_cell: Line = if flat.match_indices.is_empty() {
                Line::raw(format!("{}{}{}", indent, icon, flat.display_name))
            } else {
                // Build spans: prefix (indent + icon) plain, then walk display_name
                // char-by-char and highlight positions listed in match_indices.
                let highlight = Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD);
                let mut spans: Vec<Span> = vec![Span::raw(format!("{}{}", indent, icon))];
                // match_indices are already sorted (done in search.rs).
                let mut idx_iter = flat.match_indices.iter().peekable();
                let mut current_text = String::new();
                let mut in_match = false;
                for (char_pos, ch) in flat.display_name.chars().enumerate() {
                    let is_match = idx_iter.peek().copied() == Some(&(char_pos as u32));
                    if is_match {
                        idx_iter.next();
                    }
                    if is_match != in_match {
                        if !current_text.is_empty() {
                            spans.push(if in_match {
                                Span::styled(std::mem::take(&mut current_text), highlight)
                            } else {
                                Span::raw(std::mem::take(&mut current_text))
                            });
                        }
                        in_match = is_match;
                    }
                    current_text.push(ch);
                }
                if !current_text.is_empty() {
                    spans.push(if in_match {
                        Span::styled(current_text, highlight)
                    } else {
                        Span::raw(current_text)
                    });
                }
                Line::from(spans)
            };

            let (size_str, algo_str) = if let Some(idx) = flat.entry_idx {
                let e = &state.entries[idx];
                (
                    human_size(e.entry.compressed_size),
                    e.entry.compression_method.as_str(),
                )
            } else {
                (String::new(), "")
            };

            Row::new(vec![
                Cell::from(file_cell),
                Cell::from(size_str),
                Cell::from(algo_str),
            ])
        })
        .collect();

    let widths = [
        Constraint::Fill(1),
        Constraint::Length(10),
        Constraint::Length(11),
    ];

    let filename = state
        .archive_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?");

    // Dim the list border when focus is on the preview panel.
    let list_border_style = if state.focus == Focus::Preview {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(format!(" {} ", filename))
                .borders(Borders::ALL)
                .border_style(list_border_style),
        )
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD));

    frame.render_stateful_widget(table, list_area, &mut state.table_state);

    // ── Meta-search help panel (right split) ───────────────────────────────
    if let Some(help_area) = meta_help_area {
        render_meta_search_help_panel(frame, help_area, &state.meta_search_error, &locale);
    }

    // ── Preview floating window ────────────────────────────────────────────
    if let Some(area) = preview_area {
        use ratatui::widgets::Clear;
        frame.render_widget(Clear, area);
        match state.preview_mode {
            PreviewMode::Metadata => render_metadata_panel(frame, area, state),
            PreviewMode::Content => render_content_panel(frame, area, state),
            PreviewMode::Closed => {}
        }
    }

    // ── Status bar ─────────────────────────────────────────────────────────
    let total = state.entries.len();
    let total_key = crate::utils::plural_key(total, "tui.inspect.status_total", &locale);
    let total_text = rust_i18n::t!(&total_key, locale = locale, total = total);

    let key_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Gray);
    let count_style = Style::default().fg(Color::LightCyan);
    let bar_bg = Style::default().bg(Color::Black);

    let nav_hint = rust_i18n::t!("tui.inspect.hint_navigate", locale = locale);
    let toggle_hint = rust_i18n::t!("tui.inspect.hint_toggle", locale = locale);
    let quit_hint = rust_i18n::t!("tui.inspect.hint_quit", locale = locale);
    let metadata_hint = rust_i18n::t!("tui.inspect.hint_metadata", locale = locale);
    let content_hint = rust_i18n::t!("tui.inspect.hint_content", locale = locale);
    let close_hint = rust_i18n::t!("tui.inspect.hint_close_preview", locale = locale);
    let scroll_hint = rust_i18n::t!("tui.inspect.hint_scroll", locale = locale);
    let search_hint = rust_i18n::t!("tui.inspect.hint_search", locale = locale);
    let meta_search_hint = rust_i18n::t!("tui.inspect.hint_meta_search", locale = locale);
    let search_type_hint = rust_i18n::t!("tui.inspect.hint_search_type", locale = locale);
    let search_keep_hint = rust_i18n::t!("tui.inspect.hint_search_keep", locale = locale);
    let search_restore_hint = rust_i18n::t!("tui.inspect.hint_search_restore", locale = locale);
    let meta_search_keep_hint =
        rust_i18n::t!("tui.inspect.hint_meta_search_keep", locale = locale);
    let meta_search_restore_hint =
        rust_i18n::t!("tui.inspect.hint_meta_search_restore", locale = locale);

    if state.meta_search_active {
        // ── Metadata search input box ──────────────────────────────────────
        let prompt_style = Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD);
        let cursor_style = Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD);
        let input_spans = vec![
            Span::styled(" s ", prompt_style),
            Span::styled(
                state.meta_search_query.clone(),
                Style::default().fg(Color::White),
            ),
            Span::styled("█", cursor_style),
        ];

        // Right: keybinding hints only (error is shown in the help panel).
        let mut hint_spans: Vec<Span> = Vec::new();
        for (i, (key, desc)) in [
            ("Enter", meta_search_keep_hint.as_ref()),
            ("Esc", meta_search_restore_hint.as_ref()),
        ]
        .iter()
        .enumerate()
        {
            if i > 0 {
                hint_spans.push(Span::raw("   "));
            }
            hint_spans.push(Span::styled(*key, key_style));
            hint_spans.push(Span::styled(format!(" {desc}"), desc_style));
        }
        hint_spans.push(Span::raw("  "));

        let status_chunks =
            Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).split(status_area);
        frame.render_widget(
            Paragraph::new(Line::from(input_spans)).style(bar_bg),
            status_chunks[0],
        );
        frame.render_widget(
            Paragraph::new(Line::from(hint_spans))
                .style(bar_bg)
                .alignment(ratatui::layout::Alignment::Right),
            status_chunks[1],
        );
    } else if state.search_active {
        // ── Filename search input box ──────────────────────────────────────
        let prompt_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        let cursor_style = Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD);
        let input_spans = vec![
            Span::styled(" / ", prompt_style),
            Span::styled(
                state.search_query.clone(),
                Style::default().fg(Color::White),
            ),
            Span::styled("█", cursor_style),
        ];

        // Right: "type to filter   Enter keep   Esc restore"
        let mut search_hint_spans: Vec<Span> = Vec::new();
        for (i, (key, desc)) in [
            ("Enter", search_keep_hint.as_ref()),
            ("Esc", search_restore_hint.as_ref()),
        ]
        .iter()
        .enumerate()
        {
            if i > 0 {
                search_hint_spans.push(Span::raw("   "));
            }
            search_hint_spans.push(Span::styled(*key, key_style));
            search_hint_spans.push(Span::styled(format!(" {desc}"), desc_style));
        }
        search_hint_spans.push(Span::raw("   "));
        search_hint_spans.push(Span::styled(format!("{search_type_hint}  "), desc_style));

        let status_chunks =
            Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).split(status_area);
        frame.render_widget(
            Paragraph::new(Line::from(input_spans)).style(bar_bg),
            status_chunks[0],
        );
        frame.render_widget(
            Paragraph::new(Line::from(search_hint_spans))
                .style(bar_bg)
                .alignment(ratatui::layout::Alignment::Right),
            status_chunks[1],
        );
    } else {
        let selected_is_dir = state
            .table_state
            .selected()
            .and_then(|i| state.visible.get(i))
            .map(|flat| flat.is_dir)
            .unwrap_or(false);

        let selected_is_file = state
            .table_state
            .selected()
            .and_then(|i| state.visible.get(i))
            .map(|flat| !flat.is_dir && flat.entry_idx.is_some())
            .unwrap_or(false);

        // Files stored with CompressionMethod::None (NoneCompressor: images, archives,
        // video, …) or LeptonJpeg are always binary — content preview is pointless.
        let selected_is_binary = state
            .table_state
            .selected()
            .and_then(|i| state.visible.get(i))
            .and_then(|flat| flat.entry_idx)
            .map(|ei| {
                matches!(
                    state.entries[ei].entry.compression_method,
                    CompressionMethod::None | CompressionMethod::LeptonJpeg
                )
            })
            .unwrap_or(false);

        let hints_vec: Vec<(&str, &str)> = match state.preview_mode {
            PreviewMode::Metadata => {
                let mut hints = vec![("↑↓/PgUp/PgDn", scroll_hint.as_ref())];
                if !selected_is_binary {
                    hints.push(("c", content_hint.as_ref()));
                }
                hints.push(("Esc", close_hint.as_ref()));
                hints.push(("q", quit_hint.as_ref()));
                hints
            }
            PreviewMode::Content => vec![
                ("↑↓/PgUp/PgDn", scroll_hint.as_ref()),
                ("m", metadata_hint.as_ref()),
                ("Esc", close_hint.as_ref()),
                ("q", quit_hint.as_ref()),
            ],
            PreviewMode::Closed => {
                let mut hints = vec![("↑↓/jk", nav_hint.as_ref())];
                if selected_is_dir {
                    hints.push(("Enter/Space", toggle_hint.as_ref()));
                }
                if selected_is_file {
                    hints.push(("m", metadata_hint.as_ref()));
                    if !selected_is_binary {
                        hints.push(("c", content_hint.as_ref()));
                    }
                }
                hints.push(("/", search_hint.as_ref()));
                hints.push(("s", meta_search_hint.as_ref()));
                hints.push(("q", quit_hint.as_ref()));
                hints
            }
        };

        let mut hint_spans: Vec<Span> = vec![Span::raw(" ")];
        for (i, (key, desc)) in hints_vec.iter().enumerate() {
            if i > 0 {
                hint_spans.push(Span::raw("   "));
            }
            hint_spans.push(Span::styled(*key, key_style));
            hint_spans.push(Span::styled(format!(" {desc}"), desc_style));
        }

        let right_text = format!(" {} ", total_text);
        let right_width = right_text.chars().count() as u16;
        let status_chunks =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(right_width)])
                .split(status_area);

        frame.render_widget(
            Paragraph::new(Line::from(hint_spans)).style(bar_bg),
            status_chunks[0],
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(right_text, count_style)])).style(bar_bg),
            status_chunks[1],
        );
    }
}

// ---------------------------------------------------------------------------
// Preview panel rendering
// ---------------------------------------------------------------------------

/// Render the **tag-search help** panel shown as a right split when meta search is active.
///
/// Shows usage examples and a table of all available tag aliases with their
/// localised descriptions.  If there is a current parse error it is displayed
/// prominently at the top.
fn render_meta_search_help_panel(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    error: &Option<String>,
    locale: &str,
) {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Paragraph};

    let block = Block::default()
        .title(" Tag Search ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let heading_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let alias_style = Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Gray);
    let example_style = Style::default().fg(Color::White);
    let error_style = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);

    let max_alias = TAG_ALIASES_FULL
        .iter()
        .map(|(a, _, _)| a.len())
        .max()
        .unwrap_or(6);

    let mut lines: Vec<Line> = vec![Line::from("")];

    // ── Parse-error callout ───────────────────────────────────────────────
    if let Some(err) = error {
        // Wrap error text to fit inside the panel (content width = area - 2 borders - 2 padding).
        let wrap_width = area.width.saturating_sub(4) as usize;
        let mut word_buf = String::new();
        let mut row = String::new();
        for word in err.split_whitespace() {
            if !row.is_empty() && row.len() + 1 + word.len() > wrap_width {
                lines.push(Line::from(Span::styled(format!(" {row}"), error_style)));
                row.clear();
            }
            if !row.is_empty() {
                row.push(' ');
            }
            row.push_str(word);
            word_buf.clear();
            let _ = word_buf;
        }
        if !row.is_empty() {
            lines.push(Line::from(Span::styled(format!(" {row}"), error_style)));
        }
        lines.push(Line::from(""));
    }

    // ── Examples ─────────────────────────────────────────────────────────
    lines.push(Line::from(Span::styled(" Examples", heading_style)));
    for example in &[
        " artist:beatles",
        " album:abbey road",
        " make:canon model:5D",
    ] {
        lines.push(Line::from(Span::styled(*example, example_style)));
    }
    lines.push(Line::from(""));

    // ── Tag table ─────────────────────────────────────────────────────────
    lines.push(Line::from(Span::styled(" Tags", heading_style)));
    for &(alias, _key, i18n_key) in TAG_ALIASES_FULL {
        let desc = rust_i18n::t!(i18n_key, locale = locale).into_owned();
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {:<width$}  ", alias, width = max_alias),
                alias_style,
            ),
            Span::styled(desc, desc_style),
        ]));
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// Render the **metadata** floating window (compression stats + extra tags).
fn render_metadata_panel(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    state: &mut AppState,
) {
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

    let locale = state.locale.as_str();

    let block = Block::default()
        .title(" Metadata ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let Some((_, ref entry_preview)) = state.preview_cache else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " No entry selected",
                Style::default().fg(Color::DarkGray),
            ))
            .block(block),
            area,
        );
        return;
    };

    let key_style = Style::default().fg(Color::Cyan);
    let val_style = Style::default().fg(Color::White);
    let dim_style = Style::default().fg(Color::DarkGray);

    let meta = &entry_preview.metadata;

    // Extract encoding upfront to show in the compression section.
    let encoding_opt = match &entry_preview.content {
        PreviewContent::Text { encoding, .. } => Some(*encoding),
        PreviewContent::HighlightedText { encoding, .. } => Some(*encoding),
        _ => None,
    };

    // ── Compression metadata ──────────────────────────────────────────────
    let label_method =
        rust_i18n::t!("tui.inspect.preview.label_method", locale = locale).into_owned();
    let label_original =
        rust_i18n::t!("tui.inspect.preview.label_original", locale = locale).into_owned();
    let label_checksum =
        rust_i18n::t!("tui.inspect.preview.label_checksum", locale = locale).into_owned();

    let size_row: (String, String) = if meta.compressed_size == 0 {
        let label =
            rust_i18n::t!("tui.inspect.preview.label_stored", locale = locale).into_owned();
        (label, human_size(meta.original_size))
    } else {
        let label =
            rust_i18n::t!("tui.inspect.preview.label_compressed", locale = locale).into_owned();
        let ratio = ratio_label(meta.compressed_size, meta.original_size);
        (
            label,
            format!("{} ({})", human_size(meta.compressed_size), ratio),
        )
    };

    let mut comp_rows: Vec<(String, String, bool)> = vec![
        (label_method, meta.compression_method.clone(), false),
        (label_original, human_size(meta.original_size), false),
        (size_row.0, size_row.1, false),
        (
            label_checksum,
            format!("{}…", &meta.checksum_hex[..16]),
            true,
        ),
    ];

    if let Some(enc) = encoding_opt {
        let label_encoding =
            rust_i18n::t!("tui.inspect.preview.label_encoding", locale = locale).into_owned();
        comp_rows.push((label_encoding, enc.to_string(), false));
    }

    let max_comp_key = comp_rows
        .iter()
        .map(|(k, _, _)| k.chars().count())
        .max()
        .unwrap_or(0);

    let mut lines: Vec<Line> = Vec::new();

    for (key, value, is_dim) in &comp_rows {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {:<width$}  ", key, width = max_comp_key),
                key_style,
            ),
            Span::styled(value.clone(), if *is_dim { dim_style } else { val_style }),
        ]));
    }

    // ── Extra metadata tags ───────────────────────────────────────────────
    if !meta.extra_tags.is_empty() {
        let max_tag_key = meta
            .extra_tags
            .iter()
            .map(|(k, _)| k.chars().count())
            .max()
            .unwrap_or(0);

        lines.push(Line::from(""));
        for (name, value) in &meta.extra_tags {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {:<width$}  ", name, width = max_tag_key),
                    key_style,
                ),
                Span::styled(value.clone(), val_style),
            ]));
        }
    }

    let viewport_width = area.width.saturating_sub(2);
    let viewport_height = area.height.saturating_sub(2);
    let line_count = count_rendered_lines(&lines, viewport_width);
    state.preview_line_count = line_count;
    state.preview_viewport_height = viewport_height;
    let max_scroll = line_count.saturating_sub(viewport_height);
    state.preview_scroll = state.preview_scroll.min(max_scroll);

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((state.preview_scroll, 0)),
        area,
    );
}

/// Render the **content** floating window (file text / binary / encrypted message).
fn render_content_panel(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    state: &mut AppState,
) {
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

    let locale = state.locale.as_str();

    let block = Block::default()
        .title(" Content ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let Some((_, ref entry_preview)) = state.preview_cache else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " No entry selected",
                Style::default().fg(Color::DarkGray),
            ))
            .block(block),
            area,
        );
        return;
    };

    let dim_style = Style::default().fg(Color::DarkGray);

    let mut lines: Vec<Line> = Vec::new();

    match &entry_preview.content {
        PreviewContent::EncryptedNoPassphrase => {
            let msg = rust_i18n::t!("tui.inspect.preview.encrypted_no_pass", locale = locale);
            let hint = rust_i18n::t!("tui.inspect.preview.encrypted_hint", locale = locale);
            lines.push(Line::from(Span::styled(
                format!("  {}", msg),
                Style::default().fg(Color::Red),
            )));
            lines.push(Line::from(Span::styled(format!("  {}", hint), dim_style)));
        }
        PreviewContent::EncryptedWrongPassphrase => {
            let msg = rust_i18n::t!("tui.inspect.preview.wrong_passphrase", locale = locale);
            lines.push(Line::from(Span::styled(
                format!("  {}", msg),
                Style::default().fg(Color::Red),
            )));
        }
        PreviewContent::Binary => {
            let msg = rust_i18n::t!("tui.inspect.preview.binary", locale = locale);
            lines.push(Line::from(Span::styled(format!("  {}", msg), dim_style)));
        }
        PreviewContent::Text {
            encoding: _,
            text,
            truncated,
        } => {
            lines.push(Line::from(""));
            for line in text.lines() {
                lines.push(Line::from(Span::raw(line.to_string())));
            }
            if *truncated {
                lines.push(Line::from(""));
                let trunc_msg = rust_i18n::t!("tui.inspect.preview.truncated", locale = locale);
                lines.push(Line::from(Span::styled(
                    format!("  {}", trunc_msg),
                    dim_style,
                )));
            }
        }
        PreviewContent::HighlightedText {
            encoding: _,
            lines: highlighted_lines,
            truncated,
        } => {
            lines.push(Line::from(""));
            for hl_line in highlighted_lines {
                lines.push(hl_line.clone());
            }
            if *truncated {
                lines.push(Line::from(""));
                let trunc_msg = rust_i18n::t!("tui.inspect.preview.truncated", locale = locale);
                lines.push(Line::from(Span::styled(
                    format!("  {}", trunc_msg),
                    dim_style,
                )));
            }
        }
    }

    let viewport_width = area.width.saturating_sub(2);
    let viewport_height = area.height.saturating_sub(2);
    let line_count = count_rendered_lines(&lines, viewport_width);
    state.preview_line_count = line_count;
    state.preview_viewport_height = viewport_height;
    let max_scroll = line_count.saturating_sub(viewport_height);
    state.preview_scroll = state.preview_scroll.min(max_scroll);

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((state.preview_scroll, 0)),
        area,
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return a [`Rect`] centered inside `r` that spans `percent_x`% of the width
/// and `percent_y`% of the height, used for floating popup windows.
fn centered_popup_rect(percent_x: u16, percent_y: u16, r: ratatui::layout::Rect) -> ratatui::layout::Rect {
    use ratatui::layout::{Constraint, Layout};
    let margin_v = (100u16.saturating_sub(percent_y)) / 2;
    let margin_h = (100u16.saturating_sub(percent_x)) / 2;
    let vert = Layout::vertical([
        Constraint::Percentage(margin_v),
        Constraint::Percentage(percent_y),
        Constraint::Percentage(margin_v),
    ])
    .split(r);
    Layout::horizontal([
        Constraint::Percentage(margin_h),
        Constraint::Percentage(percent_x),
        Constraint::Percentage(margin_h),
    ])
    .split(vert[1])[1]
}

/// Count the total number of terminal rows that `lines` occupies when rendered
/// inside a panel of `viewport_width` columns (with `Wrap { trim: false }`).
///
/// Each logical [`Line`] takes `ceil(visible_width / viewport_width)` rows,
/// with a minimum of 1 row for empty lines.
fn count_rendered_lines(lines: &[ratatui::text::Line], viewport_width: u16) -> u16 {
    if viewport_width == 0 {
        return lines.len() as u16;
    }
    let w = viewport_width as usize;
    lines
        .iter()
        .map(|line| {
            let width: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
            if width == 0 {
                1
            } else {
                ((width + w - 1) / w) as u16
            }
        })
        .fold(0u16, |acc, n| acc.saturating_add(n))
}

fn human_size(bytes: u32) -> String {
    const KB: u32 = 1024;
    const MB: u32 = 1024 * 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn ratio_label(compressed: u32, original: u32) -> String {
    if original == 0 {
        return String::from("—");
    }
    let ratio = compressed as f64 / original as f64;
    if compressed < original {
        format!("{:.1}% saved", (1.0 - ratio) * 100.0)
    } else if compressed == original {
        String::from("no change")
    } else {
        format!("{:.1}% larger", (ratio - 1.0) * 100.0)
    }
}
