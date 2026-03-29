pub mod state;
pub mod tree;

use crate::models::archive::CompressionMethod;
use crate::tui::{state::AppState, tree as tui_tree};
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

                (KeyCode::Up, _) | (KeyCode::Char('k'), _) => move_up(state),
                (KeyCode::Down, _) | (KeyCode::Char('j'), _) => move_down(state),

                (KeyCode::Enter, _) | (KeyCode::Char(' '), _) => toggle_at_cursor(state),

                _ => {}
            }
        }
    }
    Ok(())
}

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

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn draw(frame: &mut ratatui::Frame, state: &mut AppState) {
    use ratatui::layout::{Constraint, Layout};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

    let locale = state.locale.as_str();

    // Top area = table, bottom row = status bar.
    let chunks = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).split(frame.area());
    let (main_area, status_area) = (chunks[0], chunks[1]);

    // Translated column headers.
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

    // Build one row per visible FlatNode.
    let rows: Vec<Row> = state
        .visible
        .iter()
        .map(|flat| {
            let icon = if flat.is_dir {
                if flat.expanded { "▼ " } else { "▶ " }
            } else {
                "  " // 2 spaces align with dir icon width
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

    // Column widths: path fills remaining space; size and compression are fixed.
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

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(format!(" {} ", filename))
                .borders(Borders::ALL),
        )
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD));

    frame.render_stateful_widget(table, main_area, &mut state.table_state);

    // ── Status bar ────────────────────────────────────────────────────────────
    let total = state.entries.len();

    let total_key = crate::utils::plural_key(total, "tui.inspect.status_total", locale);
    let total_text = rust_i18n::t!(&total_key, locale = locale, total = total);

    // Styles used in the status bar.
    let key_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Gray);
    let count_style = Style::default().fg(Color::LightCyan);
    let bar_bg = Style::default().bg(Color::Black);

    // (key label, translated action description)
    let nav_hint = rust_i18n::t!("tui.inspect.hint_navigate", locale = locale);
    let toggle_hint = rust_i18n::t!("tui.inspect.hint_toggle", locale = locale);
    let quit_hint = rust_i18n::t!("tui.inspect.hint_quit", locale = locale);
    let hints: &[(&str, &str)] = &[
        ("↑↓/jk", nav_hint.as_ref()),
        ("Enter/Space", toggle_hint.as_ref()),
        ("q", quit_hint.as_ref()),
    ];

    let mut hint_spans: Vec<Span> = vec![Span::raw(" ")];
    for (i, (key, desc)) in hints.iter().enumerate() {
        if i > 0 {
            hint_spans.push(Span::raw("   "));
        }
        hint_spans.push(Span::styled(*key, key_style));
        hint_spans.push(Span::styled(format!(" {desc}"), desc_style));
    }

    // Right side: total entry count — size the column to the text length so it
    // sits flush against the right edge.
    let right_text = format!(" {} ", total_text);
    let right_width = right_text.chars().count() as u16;
    let status_chunks = Layout::horizontal([Constraint::Fill(1), Constraint::Length(right_width)])
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

fn algorithm_name(method: CompressionMethod) -> &'static str {
    match method {
        CompressionMethod::None => "None",
        CompressionMethod::Brotli => "Brotli",
        CompressionMethod::Zstandard => "Zstandard",
        CompressionMethod::Lzma => "LZMA",
        CompressionMethod::LeptonJpeg => "Lepton",
    }
}
