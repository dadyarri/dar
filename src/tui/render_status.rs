use crate::models::archive::CompressionMethod;
use crate::tui::state::{AppState, PreviewMode};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// Render the status bar (or active search input box) into `area`.
pub(crate) fn draw_status_bar(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    state: &mut AppState,
) {
    let locale = state.locale.as_str().to_owned();

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
            Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).split(area);
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
            Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).split(area);
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
                .split(area);

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
