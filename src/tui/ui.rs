use ratatui::{
    layout::{Constraint, Direction, Layout, Rect, Alignment},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap, Clear},
    Frame,
};

use super::app::{TuiState, TuiMode, MessageType};
use super::symbols::TerminalSymbols;

thread_local! {
    static SYMBOLS: TerminalSymbols = TerminalSymbols::detect();
}

fn get_symbols() -> TerminalSymbols {
    SYMBOLS.with(|s| s.clone())
}
pub fn draw(f: &mut Frame, app: &TuiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(f.size());

    draw_header(f, app, chunks[0]);

    match app.mode {
        TuiMode::Browse => draw_browse_view(f, app, chunks[1]),
        TuiMode::Preview => draw_preview_view(f, app, chunks[1]),
        TuiMode::ExtractDialog => draw_extract_dialog(f, app, f.size()),
        TuiMode::Help => draw_help_view(f, app, chunks[1]),
    }

    draw_mode_specific_footer(f, app, chunks[2]);

    if let Some((msg, _, msg_type)) = &app.message {
        if !app.should_clear_message() {
            draw_message(f, msg, *msg_type, f.size());
        }
    }
}

fn draw_header(f: &mut Frame, app: &TuiState, area: Rect) {
    let title = format!(
        " Archive: {} | Files: {} | Size: {} ",
        app.archive_path,
        app.root_tree.file_count(),
        format_size(app.root_tree.total_uncompressed_size())
    );

    let block = Block::default()
        .title(title)
        .borders(Borders::BOTTOM);

    f.render_widget(block, area);
}

fn draw_browse_view(f: &mut Frame, app: &TuiState, area: Rect) {
    let items = app.get_visible_items();

    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let is_selected = idx == app.selected_index;
            let symbols = get_symbols();
            let prefix = if item.node.is_dir {
                if item.node.is_expanded {
                    symbols.expanded_dir.clone()
                } else {
                    symbols.collapsed_dir.clone()
                }
            } else {
                symbols.file.clone()
            };

            let indent = "  ".repeat(item.depth);
            let name = &item.node.name;
            let size = if !item.node.is_dir {
                format!(" ({})", format_size(item.node.entry.as_ref().unwrap().uncompressed_size))
            } else {
                String::new()
            };

            let selection_marker = if item.node.selected {
                symbols.selected
            } else {
                symbols.unselected
            };

            let content = format!("{}{}{}{}{}", indent, selection_marker, prefix, name, size);

            let style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else if item.node.selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(list_items)
        .block(Block::default().borders(Borders::ALL).title(" Archive Contents "));

    f.render_widget(list, area);
}

fn draw_preview_view(f: &mut Frame, app: &TuiState, area: Rect) {
    if let Some(content) = &app.preview_content {
        let lines = super::preview::PreviewReader::get_lines(content, area.width as usize - 4);

        let display_lines: Vec<Line> = lines
            .iter()
            .skip(app.preview_scroll)
            .take((area.height - 4) as usize)
            .enumerate()
            .map(|(idx, line)| {
                let line_num = app.preview_scroll + idx + 1;
                Line::from(vec![
                    Span::styled(
                        format!("{:>5} ", line_num),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(line),
                ])
            })
            .collect();

        let text_block = Block::default()
            .borders(Borders::ALL)
            .title(" Preview ")
            .style(Style::default());

        let paragraph = Paragraph::new(display_lines)
            .block(text_block)
            .style(Style::default().fg(Color::White));

        f.render_widget(paragraph, area);
    }
}

fn draw_extract_dialog(f: &mut Frame, app: &TuiState, area: Rect) {
    let dialog_width = std::cmp::min(90, area.width - 4);
    let dialog_height = std::cmp::min(20, area.height - 4);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(dialog_width),
            Constraint::Min(0),
        ])
        .split(area);

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(dialog_height),
            Constraint::Min(0),
        ])
        .split(horizontal[1]);

    let dialog_area = vertical[1];

    f.render_widget(Clear, dialog_area);

    let dialog = Block::default()
        .borders(Borders::ALL)
        .title(" Extract ");

    f.render_widget(dialog, dialog_area);

    let inner = Rect {
        x: dialog_area.x + 1,
        y: dialog_area.y + 1,
        width: dialog_area.width.saturating_sub(2),
        height: dialog_area.height.saturating_sub(2),
    };

    let lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(format!(
            "Extracting {} file(s)",
            app.extract_selected.len()
        )),
        Line::from(""),
    ];

    let selected_display: Vec<Line> = app
        .extract_selected
        .iter()
        .take(3)
        .map(|p| Line::from(format!("  • {}", p)))
        .collect();

    let mut all_lines = lines;
    all_lines.extend(selected_display);

    if app.extract_selected.len() > 3 {
        all_lines.push(Line::from(format!(
            "  • ... and {} more",
            app.extract_selected.len() - 3
        )));
    }

    all_lines.push(Line::from(""));
    all_lines.push(Line::from("Extract to:"));
    all_lines.push(Line::from(""));

    let input_style = Style::default().fg(Color::Black).bg(Color::White);
    all_lines.push(Line::from(vec![Span::styled(
        format!("{:<width$}", app.input_buffer, width = inner.width as usize - 2),
        input_style,
    )]));

    all_lines.push(Line::from(""));
    all_lines.push(Line::from(
        "[Enter] Extract | [Esc] Cancel",
    ));

    let paragraph = Paragraph::new(all_lines)
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Center);

    f.render_widget(paragraph, inner);
}

fn draw_help_view(f: &mut Frame, _app: &TuiState, area: Rect) {
    let help_text = vec![
        Line::from(""),
        Line::from("KEYBOARD SHORTCUTS"),
        Line::from(""),
        Line::from("Navigation:"),
        Line::from("  j/↓ - Down         k/↑ - Up              l/→ - Expand        h/← - Collapse"),
        Line::from("  g - Home           Shift+G - End"),
        Line::from(""),
        Line::from("Selection & Actions:"),
        Line::from("  Space - Toggle selection              Ctrl+A - Select all"),
        Line::from("  Ctrl+D - Deselect all                 p - Preview file"),
        Line::from("  e - Extract selected                  ? - Help"),
        Line::from("  q - Quit"),
        Line::from(""),
        Line::from("Preview Mode:"),
        Line::from("  j/↓ - Scroll down                     k/↑ - Scroll up"),
        Line::from("  q - Close preview                     Esc - Close preview"),
        Line::from(""),
        Line::from("Press any key to close"),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title(" Help "))
        .style(Style::default().fg(Color::White));

    f.render_widget(paragraph, area);
}

fn draw_mode_specific_footer(f: &mut Frame, app: &TuiState, area: Rect) {
    let text = match app.mode {
        TuiMode::Browse => vec![
            Line::from(vec![
                Span::raw("["),
                Span::styled("j/k", Style::default().fg(Color::Yellow)),
                Span::raw("] Move | ["),
                Span::styled("Space", Style::default().fg(Color::Yellow)),
                Span::raw("] Select | ["),
                Span::styled("Ctrl+A", Style::default().fg(Color::Yellow)),
                Span::raw("] All | ["),
                Span::styled("Ctrl+D", Style::default().fg(Color::Yellow)),
                Span::raw("] None | ["),
                Span::styled("p", Style::default().fg(Color::Yellow)),
                Span::raw("] Preview | ["),
                Span::styled("e", Style::default().fg(Color::Yellow)),
                Span::raw("] Extract | ["),
                Span::styled("?", Style::default().fg(Color::Yellow)),
                Span::raw("] Help"),
            ]),
        ],
        TuiMode::Preview => vec![
            Line::from(vec![
                Span::raw("["),
                Span::styled("j/k", Style::default().fg(Color::Yellow)),
                Span::raw("] Scroll | ["),
                Span::styled("g", Style::default().fg(Color::Yellow)),
                Span::raw("] Top | ["),
                Span::styled("G", Style::default().fg(Color::Yellow)),
                Span::raw("] Bottom | ["),
                Span::styled("q/Esc", Style::default().fg(Color::Yellow)),
                Span::raw("] Back to Browse"),
            ]),
        ],
        TuiMode::ExtractDialog => vec![
            Line::from(vec![
                Span::raw("Edit path and press ["),
                Span::styled("Enter", Style::default().fg(Color::Yellow)),
                Span::raw("] to extract or ["),
                Span::styled("Esc", Style::default().fg(Color::Yellow)),
                Span::raw("] to cancel"),
            ]),
        ],
        TuiMode::Help => vec![
            Line::from(vec![
                Span::raw("Press ["),
                Span::styled("q", Style::default().fg(Color::Yellow)),
                Span::raw("] or any key to close"),
            ]),
        ],
    };

    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::TOP))
        .style(Style::default().fg(Color::DarkGray));

    f.render_widget(paragraph, area);
}

fn draw_message(f: &mut Frame, msg: &str, msg_type: MessageType, area: Rect) {
    let (symbol, color) = match msg_type {
        MessageType::Error => ("✕", Color::Red),
        MessageType::Success => ("✓", Color::Green),
        MessageType::Info => ("ℹ", Color::Cyan),
    };

    let text = vec![Line::from(vec![Span::styled(
        format!(" {} {} ", symbol, msg),
        Style::default().fg(color).bg(Color::Black),
    )])];

    let para = Paragraph::new(text);

    let msg_area = Rect {
        x: area.left(),
        y: area.bottom().saturating_sub(2),
        width: std::cmp::min(area.width, msg.len() as u16 + 6),
        height: 1,
    };

    f.render_widget(para, msg_area);
}

pub fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{}{}", bytes, UNITS[unit_idx])
    } else {
        format!("{:.1}{}", size, UNITS[unit_idx])
    }
}
