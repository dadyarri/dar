use crate::tui::{
    meta_search::TAG_ALIASES_FULL,
    preview::PreviewContent,
    state::AppState,
};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Return a [`ratatui::layout::Rect`] centered inside `r` that spans
/// `percent_x`% of the width and `percent_y`% of the height.
pub(crate) fn centered_popup_rect(
    percent_x: u16,
    percent_y: u16,
    r: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
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

pub(crate) fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn ratio_label(compressed: u64, original: u64) -> String {
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

/// Render the **tag-search help** panel shown as a right split when meta search is active.
pub(crate) fn render_meta_search_help_panel(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    error: &Option<String>,
    locale: &str,
) {
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

    if let Some(err) = error {
        let wrap_width = area.width.saturating_sub(4) as usize;
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
        }
        if !row.is_empty() {
            lines.push(Line::from(Span::styled(format!(" {row}"), error_style)));
        }
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(" Examples", heading_style)));
    for example in &[
        " artist:beatles",
        " album:abbey road",
        " make:canon model:5D",
    ] {
        lines.push(Line::from(Span::styled(*example, example_style)));
    }
    lines.push(Line::from(""));

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
pub(crate) fn render_metadata_panel(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    state: &mut AppState,
) {
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

    let encoding_opt = match &entry_preview.content {
        PreviewContent::Text { encoding, .. } => Some(*encoding),
        PreviewContent::HighlightedText { encoding, .. } => Some(*encoding),
        _ => None,
    };

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
pub(crate) fn render_content_panel(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    state: &mut AppState,
) {
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
