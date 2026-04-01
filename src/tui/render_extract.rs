use crate::tui::state::AppState;
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

/// Return a fixed-height centered [`ratatui::layout::Rect`] for the extract dialog.
pub(crate) fn extract_dialog_rect(r: ratatui::layout::Rect) -> ratatui::layout::Rect {
    // Height: title + blank + prompt + input + blank + error + blank = up to 7 inner lines + 2 borders = 9
    let dialog_height = 9u16;
    let dialog_width = r.width.min(70).max(40);

    let horiz = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(dialog_width),
        Constraint::Fill(1),
    ])
    .split(r);

    let vert = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(dialog_height),
        Constraint::Fill(1),
    ])
    .split(horiz[1]);

    vert[1]
}

/// Render the extract-to-path floating dialog.
pub(crate) fn render_extract_dialog(
    frame: &mut ratatui::Frame,
    state: &AppState,
) {
    let locale = state.locale.as_str();

    let title = rust_i18n::t!("tui.inspect.extract.title", locale = locale).into_owned();
    let prompt = rust_i18n::t!("tui.inspect.extract.prompt", locale = locale).into_owned();

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let prompt_style = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);
    let input_style = Style::default().fg(Color::White);
    let cursor_style = Style::default()
        .fg(Color::Black)
        .bg(Color::White)
        .add_modifier(Modifier::BOLD);
    let error_style = Style::default()
        .fg(Color::Red)
        .add_modifier(Modifier::BOLD);

    let popup_area = extract_dialog_rect(frame.area());

    // Clamp the visible portion of the path to fit within the dialog width.
    let inner_width = popup_area.width.saturating_sub(4) as usize; // 2 borders + 2 padding
    let path = &state.extract_path;
    let display_path = if path.chars().count() > inner_width {
        // Show only the tail that fits.
        let skip = path.chars().count() - inner_width;
        path.chars().skip(skip).collect::<String>()
    } else {
        path.clone()
    };

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(Span::styled(format!("  {prompt}"), prompt_style)),
        Line::from(vec![
            Span::styled(format!("  {display_path}"), input_style),
            Span::styled("█", cursor_style),
        ]),
    ];

    // When the user typed a relative path that resolves to a known directory, show the
    // absolute path as a gray hint so they can confirm the destination.
    if let Some(resolved) = &state.extract_path_resolved {
        let hint_style = Style::default().fg(Color::DarkGray);
        let resolved_str = resolved.display().to_string();
        let resolved_char_count = resolved_str.chars().count();
        let display_resolved = if resolved_char_count > inner_width.saturating_sub(6) {
            let skip = resolved_char_count - inner_width.saturating_sub(6);
            format!("  → {}", resolved_str.chars().skip(skip).collect::<String>())
        } else {
            format!("  → {resolved_str}")
        };
        lines.push(Line::from(Span::styled(display_resolved, hint_style)));
    }

    lines.push(Line::from(""));

    if let Some(err) = &state.extract_error {
        // Wrap the error text to the inner width.
        let wrap = inner_width.max(1);
        let mut row = String::new();
        for word in err.split_whitespace() {
            if !row.is_empty() && row.len() + 1 + word.len() > wrap {
                lines.push(Line::from(Span::styled(format!("  {row}"), error_style)));
                row.clear();
            }
            if !row.is_empty() {
                row.push(' ');
            }
            row.push_str(word);
        }
        if !row.is_empty() {
            lines.push(Line::from(Span::styled(format!("  {row}"), error_style)));
        }
    }

    frame.render_widget(Clear, popup_area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .alignment(ratatui::layout::Alignment::Left),
        popup_area,
    );
}

/// Render the status bar when the extract dialog is active.
pub(crate) fn draw_extract_status_bar(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    state: &AppState,
) {
    let locale = state.locale.as_str();

    let bar_bg = Style::default().bg(Color::Black);
    let key_style = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Gray);
    let prompt_style = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);
    let cursor_style = Style::default()
        .fg(Color::Black)
        .bg(Color::White)
        .add_modifier(Modifier::BOLD);

    let confirm_hint = rust_i18n::t!("tui.inspect.extract.hint_confirm", locale = locale);
    let cancel_hint = rust_i18n::t!("tui.inspect.extract.hint_cancel", locale = locale);
    let label = rust_i18n::t!("tui.inspect.extract.bar_label", locale = locale);

    let input_spans = vec![
        Span::styled(format!(" {} ", label), prompt_style),
        Span::styled(state.extract_path.clone(), Style::default().fg(Color::White)),
        Span::styled("█", cursor_style),
    ];

    let mut hint_spans: Vec<Span> = Vec::new();
    for (i, (key, desc)) in [
        ("Enter", confirm_hint.as_ref()),
        ("Esc", cancel_hint.as_ref()),
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
}
