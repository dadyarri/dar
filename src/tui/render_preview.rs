use crate::tui::{meta_search::TAG_ALIASES_FULL, preview::PreviewContent, state::AppState};
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

/// Compute a human-readable compression ratio label.
///
/// Returns `"—"` when `original` is zero, a "X.X% saved" string when
/// `compressed < original`, `"no change"` when equal, or "X.X% larger" otherwise.
/// This is a pure function — it performs no I/O.
pub(crate) fn ratio_label(compressed: u64, original: u64, locale: &str) -> String {
    if original == 0 {
        return String::from("—");
    }
    let ratio = compressed as f64 / original as f64;
    if compressed < original {
        let pct = format!("{:.1}", (1.0 - ratio) * 100.0);
        rust_i18n::t!(
            "tui.inspect.ratio.saved",
            locale = locale,
            pct = pct.as_str()
        )
        .into_owned()
    } else if compressed == original {
        rust_i18n::t!("tui.inspect.ratio.no_change", locale = locale).into_owned()
    } else {
        let pct = format!("{:.1}", (ratio - 1.0) * 100.0);
        rust_i18n::t!(
            "tui.inspect.ratio.larger",
            locale = locale,
            pct = pct.as_str()
        )
        .into_owned()
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
        .title(rust_i18n::t!("tui.inspect.panels.tag_search", locale = locale).into_owned())
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

/// A single row in the metadata panel: `(label, value, is_dim)`.
///
/// `is_dim` is `true` for secondary values (e.g. the checksum) that should be
/// rendered in a muted colour.
pub type MetadataRow = (String, String, bool);

/// Build the ordered list of metadata rows for the given entry preview.
///
/// Returns plain `String` tuples — no ratatui types.  The result is consumed
/// by [`render_metadata_panel`] for rendering and by tests for assertions.
pub(crate) fn build_metadata_rows_data(
    meta: &crate::tui::preview::EntryMetadata,
    content: &PreviewContent,
    locale: &str,
) -> Vec<MetadataRow> {
    let encoding_opt = match content {
        PreviewContent::StoredChecksumMismatch => None,
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
        let ratio = ratio_label(meta.compressed_size, meta.original_size, locale);
        (
            label,
            format!("{} ({})", human_size(meta.compressed_size), ratio),
        )
    };

    let mut rows: Vec<MetadataRow> = vec![
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
        rows.push((label_encoding, enc.to_string(), false));
    }

    rows
}

/// Render the **metadata** floating window (compression stats + extra tags).
pub(crate) fn render_metadata_panel(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    state: &mut AppState,
) {
    let locale = state.locale.as_str();

    let block = Block::default()
        .title(rust_i18n::t!("tui.inspect.panels.metadata", locale = locale).into_owned())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let Some((_, ref entry_preview)) = state.preview.cache else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                rust_i18n::t!("tui.inspect.panels.no_entry_selected", locale = locale).into_owned(),
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
    let comp_rows = build_metadata_rows_data(meta, &entry_preview.content, locale);

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
    state.preview.line_count = line_count;
    state.preview.viewport_height = viewport_height;
    let max_scroll = line_count.saturating_sub(viewport_height);
    state.preview.scroll = state.preview.scroll.min(max_scroll);

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((state.preview.scroll, 0)),
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
        .title(rust_i18n::t!("tui.inspect.panels.content", locale = locale).into_owned())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let Some((_, ref entry_preview)) = state.preview.cache else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                rust_i18n::t!("tui.inspect.panels.no_entry_selected", locale = locale).into_owned(),
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
        PreviewContent::StoredChecksumMismatch => {
            let msg = rust_i18n::t!("tui.inspect.preview.stored_checksum_mismatch", locale = locale);
            let hint = rust_i18n::t!("tui.inspect.preview.stored_checksum_hint", locale = locale);
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("  {}", msg),
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("  {}", hint),
                Style::default().fg(Color::Yellow),
            )));
        }
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
    state.preview.line_count = line_count;
    state.preview.viewport_height = viewport_height;
    let max_scroll = line_count.saturating_sub(viewport_height);
    state.preview.scroll = state.preview.scroll.min(max_scroll);

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((state.preview.scroll, 0)),
        area,
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // human_size
    // ------------------------------------------------------------------

    #[test]
    fn human_size_bytes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1023), "1023 B");
    }

    #[test]
    fn human_size_kilobytes() {
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1536), "1.5 KB");
    }

    #[test]
    fn human_size_megabytes() {
        assert_eq!(human_size(1024 * 1024), "1.0 MB");
        assert_eq!(human_size(2 * 1024 * 1024), "2.0 MB");
    }

    // ------------------------------------------------------------------
    // ratio_label
    // ------------------------------------------------------------------

    #[test]
    fn ratio_label_zero_original() {
        assert_eq!(ratio_label(0, 0, "en"), "—");
    }

    #[test]
    fn ratio_label_no_change() {
        let r = ratio_label(100, 100, "en");
        // The en locale string for no_change should be non-empty.
        assert!(!r.is_empty());
    }

    #[test]
    fn ratio_label_saved() {
        let r = ratio_label(50, 100, "en");
        // 50 % saved — the result should mention a percentage.
        assert!(r.contains("50.0"), "expected 50.0 in: {r}");
    }

    #[test]
    fn ratio_label_larger() {
        let r = ratio_label(150, 100, "en");
        assert!(r.contains("50.0"), "expected 50.0 in: {r}");
    }

    // ------------------------------------------------------------------
    // build_metadata_rows_data
    // ------------------------------------------------------------------

    fn make_meta(original: u64, compressed: u64) -> crate::tui::preview::EntryMetadata {
        crate::tui::preview::EntryMetadata {
            compression_method: "zstd".to_string(),
            original_size: original,
            compressed_size: compressed,
            checksum_hex: "a".repeat(64),
            extra_tags: vec![],
        }
    }

    #[test]
    fn metadata_rows_uncompressed_has_stored_label() {
        let meta = make_meta(1024, 0);
        let rows = build_metadata_rows_data(&meta, &PreviewContent::Binary, "en");
        // Should have at least method, original, stored, checksum rows.
        assert!(rows.len() >= 4);
        // No encoding row for binary content.
        assert_eq!(rows.len(), 4);
    }

    #[test]
    fn metadata_rows_compressed_has_ratio() {
        let meta = make_meta(1024, 512);
        let rows = build_metadata_rows_data(&meta, &PreviewContent::Binary, "en");
        // The size row value should contain "50.0" (50 % saved).
        let size_row = &rows[2];
        assert!(
            size_row.1.contains("50.0"),
            "expected ratio in: {}",
            size_row.1
        );
    }

    #[test]
    fn metadata_rows_text_content_adds_encoding() {
        let meta = make_meta(100, 0);
        let content = PreviewContent::Text {
            encoding: "UTF-8",
            text: String::new(),
            truncated: false,
        };
        let rows = build_metadata_rows_data(&meta, &content, "en");
        // 4 base rows + 1 encoding row.
        assert_eq!(rows.len(), 5);
        let enc_row = rows.last().unwrap();
        assert_eq!(enc_row.1, "UTF-8");
    }

    #[test]
    fn metadata_rows_checksum_is_dimmed() {
        let meta = make_meta(0, 0);
        let rows = build_metadata_rows_data(&meta, &PreviewContent::Binary, "en");
        // Checksum row (index 3) should have is_dim = true.
        assert!(rows[3].2, "expected checksum row to be dimmed");
    }

    #[test]
    fn metadata_rows_checksum_mismatch_has_no_encoding() {
        let meta = make_meta(100, 0);
        let rows = build_metadata_rows_data(&meta, &PreviewContent::StoredChecksumMismatch, "en");
        assert_eq!(rows.len(), 4);
    }
}
