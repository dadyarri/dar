use crate::models::archive::CompressionMethod;
use crate::tui::{
    preview::PreviewIntegrity,
    state::{AppState, PreviewMode},
};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

// ---------------------------------------------------------------------------
// Pure data-preparation helpers (testable without a terminal)
// ---------------------------------------------------------------------------

/// Returns `(is_dir, is_file, is_binary)` for the currently selected visible row.
///
/// All three flags are `false` when nothing is selected or the selection index is
/// out of range.
pub(crate) fn selection_flags(state: &AppState) -> (bool, bool, bool) {
    let sel = state.table_state.selected();
    let flat = sel.and_then(|i| state.visible.get(i));

    let is_dir = flat.map(|f| f.is_dir).unwrap_or(false);
    let is_file = flat
        .map(|f| !f.is_dir && f.entry_idx.is_some())
        .unwrap_or(false);
    let is_binary = flat
        .and_then(|f| f.entry_idx)
        .map(|ei| {
            matches!(
                state.entries[ei].entry.compression_method,
                CompressionMethod::None
            )
        })
        .unwrap_or(false);

    (is_dir, is_file, is_binary)
}

/// Returns `(key_label, i18n_key)` pairs describing the normal-mode hint bar.
///
/// The result depends on the current preview mode and which kind of entry is
/// selected.  Both elements of each pair are `&'static str` so the function is
/// completely pure and requires no I/O.
pub(crate) fn normal_hint_keys(
    mode: &PreviewMode,
    is_dir: bool,
    is_file: bool,
    is_binary: bool,
) -> Vec<(&'static str, &'static str)> {
    match mode {
        PreviewMode::Metadata => {
            let mut hints = vec![("↑↓/PgUp/PgDn", "tui.inspect.hint_scroll")];
            if !is_binary {
                hints.push(("c", "tui.inspect.hint_content"));
            }
            hints.push(("Esc", "tui.inspect.hint_close_preview"));
            hints.push(("q", "tui.inspect.hint_quit"));
            hints
        }
        PreviewMode::Content => vec![
            ("↑↓/PgUp/PgDn", "tui.inspect.hint_scroll"),
            ("m", "tui.inspect.hint_metadata"),
            ("Esc", "tui.inspect.hint_close_preview"),
            ("q", "tui.inspect.hint_quit"),
        ],
        PreviewMode::Closed => {
            let mut hints = vec![("↑↓/jk", "tui.inspect.hint_navigate")];
            if is_dir {
                hints.push(("Enter/Space", "tui.inspect.hint_toggle"));
            }
            if is_file {
                hints.push(("m", "tui.inspect.hint_metadata"));
                if !is_binary {
                    hints.push(("c", "tui.inspect.hint_content"));
                }
            }
            if is_file || is_dir {
                hints.push(("x", "tui.inspect.hint_extract"));
            }
            hints.push(("/", "tui.inspect.hint_search"));
            hints.push(("s", "tui.inspect.hint_meta_search"));
            hints.push(("q", "tui.inspect.hint_quit"));
            hints
        }
    }
}

/// Render the status bar (or active search / extract input box) into `area`.
pub(crate) fn draw_status_bar(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    state: &mut AppState,
) {
    // Extract dialog owns the status bar while it is open.
    if state.extract.active {
        crate::tui::render_extract::draw_extract_status_bar(frame, area, state);
        return;
    }
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

    let search_type_hint = rust_i18n::t!("tui.inspect.hint_search_type", locale = locale);
    let search_keep_hint = rust_i18n::t!("tui.inspect.hint_search_keep", locale = locale);
    let search_restore_hint = rust_i18n::t!("tui.inspect.hint_search_restore", locale = locale);
    let meta_search_keep_hint = rust_i18n::t!("tui.inspect.hint_meta_search_keep", locale = locale);
    let meta_search_restore_hint =
        rust_i18n::t!("tui.inspect.hint_meta_search_restore", locale = locale);

    if state.meta_search.active {
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
                state.meta_search.query.clone(),
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
    } else if state.search.active {
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
                state.search.query.clone(),
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
        let (selected_is_dir, selected_is_file, selected_is_binary) = selection_flags(state);

        let hint_keys = normal_hint_keys(
            &state.preview.mode,
            selected_is_dir,
            selected_is_file,
            selected_is_binary,
        );
        // Translate each i18n key to the current locale string.
        let translated: Vec<(String, String)> = hint_keys
            .iter()
            .map(|&(key_label, i18n_key)| {
                (
                    key_label.to_string(),
                    rust_i18n::t!(i18n_key, locale = locale).into_owned(),
                )
            })
            .collect();
        let hints_vec: Vec<(&str, &str)> = translated
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let mut hint_spans: Vec<Span> = vec![Span::raw(" ")];
        for (i, (key, desc)) in hints_vec.iter().enumerate() {
            if i > 0 {
                hint_spans.push(Span::raw("   "));
            }
            hint_spans.push(Span::styled(*key, key_style));
            hint_spans.push(Span::styled(format!(" {desc}"), desc_style));
        }

        let integrity_marker = preview_integrity_marker(state);
        let incremental_marker = selected_incremental_marker(state);
        let right_text = format!(
            " {}{}{} ",
            total_text,
            incremental_marker
                .as_ref()
                .map(|(marker, _)| format!("{marker} "))
                .unwrap_or_default(),
            integrity_marker
                .as_ref()
                .map(|(marker, _)| format!(" {marker}"))
                .unwrap_or_default()
        );
        let right_width = right_text.chars().count() as u16;
        let status_chunks =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(right_width)]).split(area);

        frame.render_widget(
            Paragraph::new(Line::from(hint_spans)).style(bar_bg),
            status_chunks[0],
        );
        frame.render_widget(
            Paragraph::new(Line::from(build_right_status_spans(
                &total_text,
                incremental_marker.as_ref(),
                integrity_marker.as_ref(),
                count_style,
            )))
            .style(bar_bg),
            status_chunks[1],
        );
    }
}

fn preview_integrity_marker(state: &AppState) -> Option<(String, Style)> {
    let (_, preview) = state.preview.cache.as_ref()?;
    match preview.integrity {
        PreviewIntegrity::Verified => Some((
            if state.powerline {
                String::from("🛡")
            } else {
                String::from("✓")
            },
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        PreviewIntegrity::Mismatch => Some((
            String::from("!"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        PreviewIntegrity::NotChecked => None,
    }
}

fn selected_incremental_marker(state: &AppState) -> Option<(String, Style)> {
    let selected = state
        .table_state
        .selected()
        .and_then(|idx| state.visible.get(idx))?;
    if selected.incremental {
        Some((
            String::from("Δ"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        None
    }
}

fn build_right_status_spans<'a>(
    total_text: &'a str,
    incremental_marker: Option<&(String, Style)>,
    integrity_marker: Option<&(String, Style)>,
    count_style: Style,
) -> Vec<Span<'a>> {
    let mut spans = vec![Span::raw(" ")];
    if let Some((marker, style)) = incremental_marker {
        spans.push(Span::styled(marker.clone(), *style));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(total_text.to_string(), count_style));
    if let Some((marker, style)) = integrity_marker {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(marker.clone(), *style));
    }
    spans.push(Span::raw(" "));
    spans
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::{
        preview::{EntryMetadata, EntryPreview, PreviewContent, PreviewIntegrity},
        state::{ExtractDialog, Focus, MetaSearchState, PreviewState, SearchState},
    };
    use ratatui::widgets::TableState;

    fn hint_keys(hints: &[(&'static str, &'static str)]) -> Vec<&'static str> {
        hints.iter().map(|(k, _)| *k).collect()
    }

    #[test]
    fn closed_no_selection_shows_nav_search_quit() {
        let keys = normal_hint_keys(&PreviewMode::Closed, false, false, false);
        let labels = hint_keys(&keys);
        assert!(labels.contains(&"↑↓/jk"));
        assert!(labels.contains(&"/"));
        assert!(labels.contains(&"s"));
        assert!(labels.contains(&"q"));
        // No dir/file-specific hints.
        assert!(!labels.contains(&"Enter/Space"));
        assert!(!labels.contains(&"m"));
        assert!(!labels.contains(&"c"));
        assert!(!labels.contains(&"x"));
    }

    #[test]
    fn closed_dir_selected_shows_toggle_and_extract() {
        let keys = normal_hint_keys(&PreviewMode::Closed, true, false, false);
        let labels = hint_keys(&keys);
        assert!(labels.contains(&"Enter/Space"));
        assert!(labels.contains(&"x"));
        assert!(!labels.contains(&"m"));
        assert!(!labels.contains(&"c"));
    }

    #[test]
    fn closed_text_file_shows_metadata_and_content() {
        let keys = normal_hint_keys(&PreviewMode::Closed, false, true, false);
        let labels = hint_keys(&keys);
        assert!(labels.contains(&"m"));
        assert!(labels.contains(&"c"));
        assert!(labels.contains(&"x"));
        assert!(!labels.contains(&"Enter/Space"));
    }

    #[test]
    fn closed_binary_file_omits_content_hint() {
        let keys = normal_hint_keys(&PreviewMode::Closed, false, true, true);
        let labels = hint_keys(&keys);
        assert!(labels.contains(&"m"));
        assert!(!labels.contains(&"c"));
        assert!(labels.contains(&"x"));
    }

    #[test]
    fn metadata_mode_binary_omits_content_hint() {
        let keys = normal_hint_keys(&PreviewMode::Metadata, false, false, true);
        let labels = hint_keys(&keys);
        assert!(!labels.contains(&"c"));
        assert!(labels.contains(&"Esc"));
        assert!(labels.contains(&"q"));
    }

    #[test]
    fn metadata_mode_text_includes_content_hint() {
        let keys = normal_hint_keys(&PreviewMode::Metadata, false, false, false);
        let labels = hint_keys(&keys);
        assert!(labels.contains(&"c"));
    }

    #[test]
    fn content_mode_includes_metadata_switch() {
        let keys = normal_hint_keys(&PreviewMode::Content, false, false, false);
        let labels = hint_keys(&keys);
        assert!(labels.contains(&"m"));
        assert!(labels.contains(&"Esc"));
        assert!(labels.contains(&"q"));
        assert!(!labels.contains(&"c"));
    }

    #[test]
    fn all_hint_i18n_keys_are_non_empty() {
        for mode in [
            PreviewMode::Closed,
            PreviewMode::Metadata,
            PreviewMode::Content,
        ] {
            for &(is_dir, is_file, is_binary) in &[
                (false, false, false),
                (true, false, false),
                (false, true, false),
                (false, true, true),
            ] {
                let keys = normal_hint_keys(&mode, is_dir, is_file, is_binary);
                for (key_label, i18n_key) in keys {
                    assert!(!key_label.is_empty());
                    assert!(!i18n_key.is_empty());
                }
            }
        }
    }

    #[test]
    fn build_right_status_spans_without_marker_is_plain_count() {
        let spans = build_right_status_spans("3 files", None, None, Style::default());
        let rendered: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(rendered, " 3 files ");
    }

    #[test]
    fn build_right_status_spans_with_marker_appends_suffix() {
        let spans = build_right_status_spans(
            "3 files",
            None,
            Some(&(String::from("✓"), Style::default())),
            Style::default(),
        );
        let rendered: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(rendered, " 3 files ✓ ");
    }

    #[test]
    fn preview_integrity_marker_uses_powerline_variant() {
        let state = make_state_with_integrity(PreviewIntegrity::Verified, true);
        let marker = preview_integrity_marker(&state).unwrap();
        assert_eq!(marker.0, "🛡");
    }

    #[test]
    fn preview_integrity_marker_uses_ascii_fallback_variant() {
        let state = make_state_with_integrity(PreviewIntegrity::Verified, false);
        let marker = preview_integrity_marker(&state).unwrap();
        assert_eq!(marker.0, "✓");
    }

    #[test]
    fn preview_integrity_marker_uses_warning_on_mismatch() {
        let state = make_state_with_integrity(PreviewIntegrity::Mismatch, false);
        let marker = preview_integrity_marker(&state).unwrap();
        assert_eq!(marker.0, "!");
    }

    #[test]
    fn preview_integrity_marker_absent_when_not_checked() {
        let state = make_state_with_integrity(PreviewIntegrity::NotChecked, false);
        assert!(preview_integrity_marker(&state).is_none());
    }

    fn make_state_with_integrity(integrity: PreviewIntegrity, powerline: bool) -> AppState {
        AppState {
            archive_path: std::path::PathBuf::from("archive.dar"),
            archive_timestamp: 0,
            entries: vec![],
            passphrase: None,
            locale: crate::i18n::Locale::new("en"),
            powerline,
            tree_root: crate::tui::tree::build_tree(&[], 0),
            visible: vec![],
            table_state: TableState::default(),
            search: SearchState {
                query: String::new(),
                active: false,
            },
            meta_search: MetaSearchState {
                query: String::new(),
                active: false,
                error: None,
            },
            extract: ExtractDialog {
                active: false,
                path: String::new(),
                resolved: None,
                error: None,
            },
            preview: PreviewState {
                mode: PreviewMode::Closed,
                focus: Focus::List,
                scroll: 0,
                line_count: 0,
                viewport_height: 0,
                cache: Some((
                    0,
                    EntryPreview {
                        metadata: EntryMetadata {
                            compression_method: String::new(),
                            original_size: 0,
                            compressed_size: 0,
                            checksum_hex: "0".repeat(64),
                            extra_tags: vec![],
                        },
                        content: PreviewContent::Binary,
                        integrity,
                    },
                )),
            },
        }
    }
}
