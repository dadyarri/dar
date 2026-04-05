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

// ---------------------------------------------------------------------------
// Pure data-preparation helpers (testable without a terminal)
// ---------------------------------------------------------------------------

/// Build the display label for a single visible row (icon + indent + name).
///
/// Returns a plain `String`; ratatui-specific span construction happens in
/// [`draw_list`].
pub(crate) fn row_file_label(flat: &crate::tui::tree::FlatNode, powerline: bool) -> String {
    let icon = if flat.is_dir {
        icons::folder_icon(flat.expanded, powerline)
    } else {
        let ext = std::path::Path::new(&flat.full_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        icons::file_icon(ext, powerline)
    };
    let indent = "  ".repeat(flat.depth);
    format!("{}{}{}", indent, icon, flat.display_name)
}

/// Return `(size_string, compression_algorithm_str)` for the given optional entry.
///
/// Returns `("", "")` for directory rows that have no associated entry.
pub(crate) fn row_size_and_algo(
    entry: Option<&crate::models::archive::ArchiveIndexEntryWrapper>,
) -> (String, &'static str) {
    match entry {
        Some(e) => (
            human_size(e.entry.compressed_size),
            e.entry.compression_method.as_str(),
        ),
        None => (String::new(), ""),
    }
}

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
            let label = row_file_label(flat, state.powerline);

            let file_cell: Line = if flat.match_indices.is_empty() {
                Line::raw(label)
            } else {
                // Re-build the prefix (indent + icon) so highlighted spans start
                // at the right character position inside `display_name`.
                let prefix_len = label.len() - flat.display_name.len();
                let prefix = &label[..prefix_len];

                let highlight = Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD);
                let mut spans: Vec<Span> = vec![Span::raw(prefix.to_string())];
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

            let entry = flat.entry_idx.map(|idx| &state.entries[idx]);
            let (size_str, algo_str) = row_size_and_algo(entry);

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::archive::{ArchiveIndexEntry, ArchiveIndexEntryWrapper, CompressionMethod};
    use crate::tui::tree::FlatNode;
    use bytemuck::Zeroable;

    fn file_flat(name: &str, depth: usize) -> FlatNode {
        FlatNode {
            depth,
            is_dir: false,
            expanded: false,
            display_name: name.to_string(),
            full_path: format!("src/{name}"),
            entry_idx: None,
            match_indices: vec![],
        }
    }

    fn dir_flat(name: &str, expanded: bool) -> FlatNode {
        FlatNode {
            depth: 0,
            is_dir: true,
            expanded,
            display_name: name.to_string(),
            full_path: name.to_string(),
            entry_idx: None,
            match_indices: vec![],
        }
    }

    fn make_entry(compressed_size: u64, method: CompressionMethod) -> ArchiveIndexEntryWrapper {
        let mut raw = ArchiveIndexEntry::zeroed();
        raw.compressed_size = compressed_size;
        raw.compression_method = method;
        ArchiveIndexEntryWrapper::new(raw, "path".to_string(), String::new())
    }

    #[test]
    fn file_label_contains_display_name() {
        let flat = file_flat("main.rs", 0);
        let label = row_file_label(&flat, false);
        assert!(label.contains("main.rs"), "label: {label}");
    }

    #[test]
    fn file_label_indentation_grows_with_depth() {
        let shallow = row_file_label(&file_flat("a.rs", 0), false);
        let deep = row_file_label(&file_flat("a.rs", 2), false);
        assert!(deep.len() > shallow.len());
    }

    #[test]
    fn dir_label_uses_folder_icon() {
        let flat = dir_flat("src", true);
        let label = row_file_label(&flat, false);
        // Fallback expanded folder icon is "▼ ".
        assert!(label.contains('▼'), "label: {label}");
    }

    #[test]
    fn size_and_algo_none_entry_returns_empty() {
        let (size, algo) = row_size_and_algo(None);
        assert!(size.is_empty());
        assert!(algo.is_empty());
    }

    #[test]
    fn size_and_algo_some_entry_returns_values() {
        let entry = make_entry(1024, CompressionMethod::Brotli);
        let (size, algo) = row_size_and_algo(Some(&entry));
        assert!(!size.is_empty());
        assert_eq!(algo, CompressionMethod::Brotli.as_str());
    }

    #[test]
    fn size_formats_bytes_correctly() {
        let entry = make_entry(512, CompressionMethod::None);
        let (size, _) = row_size_and_algo(Some(&entry));
        assert!(size.contains('B'), "expected bytes unit in: {size}");
    }
}
