use crate::tui::{
    icons,
    render_preview::human_size,
    state::{AppState, Focus},
};
use ratatui::{
    layout::Constraint,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Row, Table},
};

/// Render the main file-tree table into `area`.
pub(crate) fn draw_list(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    state: &mut AppState,
) {
    let locale = state.locale.as_str().to_owned();

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
                icons::folder_icon(flat.expanded, state.powerline)
            } else {
                let ext = std::path::Path::new(&flat.full_path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                icons::file_icon(ext, state.powerline)
            };
            let indent = "  ".repeat(flat.depth);

            let file_cell: Line = if flat.match_indices.is_empty() {
                Line::raw(format!("{}{}{}", indent, icon, flat.display_name))
            } else {
                let highlight = Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD);
                let mut spans: Vec<Span> = vec![Span::raw(format!("{}{}", indent, icon))];
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

    let list_border_style = if state.preview.focus == Focus::Preview {
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

    frame.render_stateful_widget(table, area, &mut state.table_state);
}
