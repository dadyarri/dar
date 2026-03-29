pub mod preview;
pub mod state;
pub mod tree;

use crate::models::archive::CompressionMethod;
use crate::tui::{
    preview::{build_preview, PreviewContent},
    state::{AppState, Focus},
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
            match (key.code, key.modifiers) {
                (KeyCode::Char('q'), _)
                | (KeyCode::Char('Q'), _)
                | (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,

                // Preview navigation (scroll) when preview panel is focused.
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

                // List navigation when list is focused (or preview is closed).
                (KeyCode::Up, _) | (KeyCode::Char('k'), _) => move_up(state),
                (KeyCode::Down, _) | (KeyCode::Char('j'), _) => move_down(state),

                (KeyCode::Enter, _) | (KeyCode::Char(' '), _) => toggle_at_cursor(state),

                // Tab: open / close / switch the preview panel.
                (KeyCode::Tab, _) => toggle_preview(state),

                // Esc closes the preview when it is open.
                (KeyCode::Esc, _) if state.preview_open => close_preview(state),

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
    if state.preview_open {
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
    if state.preview_open {
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

/// Open the preview for the selected file entry (or close it if already open).
/// Pressing Tab on a directory does nothing.
fn toggle_preview(state: &mut AppState) {
    if state.preview_open {
        close_preview(state);
        return;
    }

    // Check that the selected node is a file.
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
    build_and_cache_preview(state, entry_idx);
    state.preview_open = true;
    state.focus = Focus::Preview;
}

/// Close the preview panel and return focus to the list.
fn close_preview(state: &mut AppState) {
    state.preview_open = false;
    state.focus = Focus::List;
    // Keep the cache so it can be reused if the user reopens the same entry.
}

/// Rebuild the preview cache for the currently selected entry.
/// Automatically closes the preview if a directory is selected.
fn refresh_preview(state: &mut AppState) {
    let Some(idx) = state.table_state.selected() else {
        return;
    };
    let Some(flat) = state.visible.get(idx).cloned() else {
        return;
    };
    if flat.is_dir || flat.entry_idx.is_none() {
        // Directories have no preview; switch focus back to list.
        state.preview_cache = None;
        state.focus = Focus::List;
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
}

fn scroll_preview_up(state: &mut AppState, lines: u16) {
    state.preview_scroll = state.preview_scroll.saturating_sub(lines);
}

fn scroll_preview_down(state: &mut AppState, lines: u16) {
    state.preview_scroll = state.preview_scroll.saturating_add(lines);
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn draw(frame: &mut ratatui::Frame, state: &mut AppState) {
    use ratatui::layout::{Constraint, Layout, Rect};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

    let locale = state.locale.as_str();

    // Top area = main content, bottom row = status bar.
    let vert = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).split(frame.area());
    let (main_area, status_area) = (vert[0], vert[1]);

    // When preview is open and the terminal is wide enough, split horizontally.
    let show_preview = state.preview_open && main_area.width >= 80;
    let (list_area, preview_area): (Rect, Option<Rect>) = if show_preview {
        let cols = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main_area);
        (cols[0], Some(cols[1]))
    } else {
        (main_area, None)
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
            let file_cell = format!("{}{}{}", indent, icon, flat.display_name);

            let (size_str, algo_str) = if let Some(idx) = flat.entry_idx {
                let e = &state.entries[idx];
                (
                    human_size(e.entry.compressed_size),
                    algorithm_name(e.entry.compression_method),
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

    // ── Preview panel ──────────────────────────────────────────────────────
    if let Some(area) = preview_area {
        render_preview_panel(frame, area, state);
    }

    // ── Status bar ─────────────────────────────────────────────────────────
    let total = state.entries.len();
    let total_key = crate::utils::plural_key(total, "tui.inspect.status_total", locale);
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
    let preview_hint = rust_i18n::t!("tui.inspect.hint_preview", locale = locale);
    let close_hint = rust_i18n::t!("tui.inspect.hint_close_preview", locale = locale);
    let scroll_hint = rust_i18n::t!("tui.inspect.hint_scroll", locale = locale);

    let hints_vec: Vec<(&str, &str)> = if state.focus == Focus::Preview {
        vec![
            ("↑↓/PgUp/PgDn", scroll_hint.as_ref()),
            ("Tab/Esc", close_hint.as_ref()),
            ("q", quit_hint.as_ref()),
        ]
    } else {
        vec![
            ("↑↓/jk", nav_hint.as_ref()),
            ("Enter/Space", toggle_hint.as_ref()),
            ("Tab", preview_hint.as_ref()),
            ("q", quit_hint.as_ref()),
        ]
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

// ---------------------------------------------------------------------------
// Preview panel rendering
// ---------------------------------------------------------------------------

fn render_preview_panel(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    state: &AppState,
) {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

    let locale = state.locale.as_str();
    let focused = state.focus == Focus::Preview;

    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" Preview ")
        .borders(Borders::ALL)
        .border_style(border_style);

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

    let mut lines: Vec<Line> = Vec::new();

    let section_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let key_style = Style::default().fg(Color::Cyan);
    let val_style = Style::default().fg(Color::White);
    let dim_style = Style::default().fg(Color::DarkGray);

    // ── Compression metadata ──────────────────────────────────────────────
    let section_compression =
        rust_i18n::t!("tui.inspect.preview.section_compression", locale = locale);
    lines.push(Line::from(Span::styled(
        format!(" {}", section_compression),
        section_style,
    )));

    let meta = &entry_preview.metadata;

    let label_method = rust_i18n::t!("tui.inspect.preview.label_method", locale = locale);
    lines.push(Line::from(vec![
        Span::styled(format!("  {:14}", label_method), key_style),
        Span::styled(meta.compression_method.clone(), val_style),
    ]));

    let label_original = rust_i18n::t!("tui.inspect.preview.label_original", locale = locale);
    lines.push(Line::from(vec![
        Span::styled(format!("  {:14}", label_original), key_style),
        Span::styled(human_size(meta.original_size), val_style),
    ]));

    if meta.compressed_size == 0 {
        let label_stored = rust_i18n::t!("tui.inspect.preview.label_stored", locale = locale);
        lines.push(Line::from(vec![
            Span::styled(format!("  {:14}", label_stored), key_style),
            Span::styled(human_size(meta.original_size), val_style),
        ]));
    } else {
        let label_compressed =
            rust_i18n::t!("tui.inspect.preview.label_compressed", locale = locale);
        let ratio = ratio_label(meta.compressed_size, meta.original_size);
        lines.push(Line::from(vec![
            Span::styled(format!("  {:14}", label_compressed), key_style),
            Span::styled(
                format!("{} ({})", human_size(meta.compressed_size), ratio),
                val_style,
            ),
        ]));
    }

    let label_checksum = rust_i18n::t!("tui.inspect.preview.label_checksum", locale = locale);
    lines.push(Line::from(vec![
        Span::styled(format!("  {:14}", label_checksum), key_style),
        // Show only the first 16 hex chars (8 bytes) to keep it readable.
        Span::styled(
            format!("{}…", &meta.checksum_hex[..16]),
            dim_style,
        ),
    ]));

    // ── Extra metadata tags ───────────────────────────────────────────────
    if !meta.extra_tags.is_empty() {
        lines.push(Line::from(""));
        let section_metadata =
            rust_i18n::t!("tui.inspect.preview.section_metadata", locale = locale);
        lines.push(Line::from(Span::styled(
            format!(" {}", section_metadata),
            section_style,
        )));
        for (name, value) in &meta.extra_tags {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:14}", name), key_style),
                Span::styled(value.clone(), val_style),
            ]));
        }
    }

    // ── Content ───────────────────────────────────────────────────────────
    lines.push(Line::from(""));
    let section_content =
        rust_i18n::t!("tui.inspect.preview.section_content", locale = locale);
    lines.push(Line::from(Span::styled(
        format!(" {}", section_content),
        section_style,
    )));

    match &entry_preview.content {
        PreviewContent::EncryptedNoPassphrase => {
            let msg =
                rust_i18n::t!("tui.inspect.preview.encrypted_no_pass", locale = locale);
            let hint =
                rust_i18n::t!("tui.inspect.preview.encrypted_hint", locale = locale);
            lines.push(Line::from(Span::styled(
                format!("  {}", msg),
                Style::default().fg(Color::Red),
            )));
            lines.push(Line::from(Span::styled(
                format!("  {}", hint),
                dim_style,
            )));
        }
        PreviewContent::EncryptedWrongPassphrase => {
            let msg =
                rust_i18n::t!("tui.inspect.preview.wrong_passphrase", locale = locale);
            lines.push(Line::from(Span::styled(
                format!("  {}", msg),
                Style::default().fg(Color::Red),
            )));
        }
        PreviewContent::Binary => {
            let msg = rust_i18n::t!("tui.inspect.preview.binary", locale = locale);
            lines.push(Line::from(Span::styled(
                format!("  {}", msg),
                dim_style,
            )));
        }
        PreviewContent::Text {
            encoding,
            text,
            truncated,
        } => {
            let label_encoding =
                rust_i18n::t!("tui.inspect.preview.label_encoding", locale = locale);
            lines.push(Line::from(vec![
                Span::styled(format!("  {:14}", label_encoding), key_style),
                Span::styled(encoding.to_string(), val_style),
            ]));
            lines.push(Line::from(""));
            for line in text.lines() {
                lines.push(Line::from(Span::raw(line.to_string())));
            }
            if *truncated {
                lines.push(Line::from(""));
                let trunc_msg =
                    rust_i18n::t!("tui.inspect.preview.truncated", locale = locale);
                lines.push(Line::from(Span::styled(
                    format!("  {}", trunc_msg),
                    dim_style,
                )));
            }
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((state.preview_scroll, 0));

    frame.render_widget(paragraph, area);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn algorithm_name(method: CompressionMethod) -> &'static str {
    match method {
        CompressionMethod::None => "None",
        CompressionMethod::Brotli => "Brotli",
        CompressionMethod::Zstandard => "Zstandard",
        CompressionMethod::Lzma => "LZMA",
        CompressionMethod::LeptonJpeg => "Lepton",
    }
}
