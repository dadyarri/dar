pub mod app;
pub mod tree;
pub mod event;
pub mod preview;
pub mod extract;
pub mod ui;
pub mod symbols;

use eyre::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use crossterm::event::{Event, poll, read};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::fs::File;
use std::io::{self, Seek};

use app::{TuiState, TuiMode};
use event::{process_key_event, TuiEvent};
use tree::ArchiveTreeBuilder;

pub fn run(archive_path: &str) -> Result<()> {
    let mut file = File::open(archive_path)?;

    let (header, header_result) = crate::archive::read_header(&mut file);
    header_result?;
    let header = header.ok_or_else(|| eyre::eyre!("Failed to read header"))?;

    let file_size = std::fs::metadata(archive_path)?.len();
    let (end_record, end_result) = crate::archive::read_end_record(&mut file, file_size);
    end_result?;
    let end_record = end_record.ok_or_else(|| eyre::eyre!("Failed to read end record"))?;

    file.seek(std::io::SeekFrom::Start(header.index_section_start))?;

    let mut entry_count_buf = [0u8; 4];
    file.read_exact(&mut entry_count_buf)?;
    let entry_count = u32::from_be_bytes(entry_count_buf);

    let mut entries = Vec::new();
    for _ in 0..entry_count {
        match crate::archive::parse_index_entry(&mut file) {
            Ok(entry) => entries.push(entry),
            Err(e) => eprintln!("Warning: Failed to parse entry: {}", e),
        }
    }

    let root_tree = ArchiveTreeBuilder::from_entries(entries);
    let mut state = TuiState::new(
        archive_path.to_string(),
        file,
        header,
        end_record,
        root_tree,
    );

    setup_terminal()?;
    let res = run_app(&mut state);
    restore_terminal()?;

    res
}

fn setup_terminal() -> Result<()> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;
    Ok(())
}

fn restore_terminal() -> Result<()> {
    let mut stdout = io::stdout();
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;
    Ok(())
}

fn run_app(state: &mut TuiState) -> Result<()> {
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    terminal.clear()?;

    let mut in_text_mode = false;
    let mut preview_reader = preview::PreviewReader::new(
        state.file_handle.try_clone()?,
        state.header.data_section_start,
    );

    loop {
        terminal.draw(|f| ui::draw(f, state))?;

        if state.should_clear_message() {
            state.message = None;
        }

        if poll(std::time::Duration::from_millis(250))? {
            if let Event::Key(key) = read()? {
                if let Some(evt) = process_key_event(key, in_text_mode) {
                    match evt {
                        TuiEvent::Help => {
                            state.mode = if matches!(state.mode, TuiMode::Help) {
                                TuiMode::Browse
                            } else {
                                TuiMode::Help
                            };
                        }

                        TuiEvent::TextInput(c) if in_text_mode => {
                            state.input_buffer.push(c);
                        }
                        TuiEvent::Backspace if in_text_mode => {
                            state.input_buffer.pop();
                        }
                        TuiEvent::Enter if in_text_mode => {
                            in_text_mode = false;
                            state.extract_output_dir = state.input_buffer.clone();

                            if let Err(e) = perform_extract(state, &mut preview_reader) {
                                state.show_error(format!("Extract failed: {}", e));
                            }

                            state.extract_selected.clear();
                            state.input_buffer.clear();
                            state.mode = TuiMode::Browse;
                        }
                        TuiEvent::Cancel if in_text_mode => {
                            in_text_mode = false;
                            state.input_buffer = state.extract_output_dir.clone();
                            state.extract_selected.clear();
                            state.mode = TuiMode::Browse;
                        }

                        _ if in_text_mode => {}
                        
                        TuiEvent::MoveUp if matches!(state.mode, TuiMode::Browse) => {
                            if state.selected_index > 0 {
                                state.selected_index -= 1;
                            }
                        }
                        TuiEvent::MoveDown if matches!(state.mode, TuiMode::Browse) => {
                            let items = state.get_visible_items();
                            if state.selected_index < items.len().saturating_sub(1) {
                                state.selected_index += 1;
                            }
                        }
                        TuiEvent::MoveToStart if matches!(state.mode, TuiMode::Browse) => {
                            state.selected_index = 0;
                        }
                        TuiEvent::MoveToEnd if matches!(state.mode, TuiMode::Browse) => {
                            let items = state.get_visible_items();
                            state.selected_index = items.len().saturating_sub(1);
                        }
                        TuiEvent::PageUp if matches!(state.mode, TuiMode::Browse) => {
                            state.selected_index = state.selected_index.saturating_sub(10);
                        }
                        TuiEvent::PageDown if matches!(state.mode, TuiMode::Browse) => {
                            let items = state.get_visible_items();
                            state.selected_index = std::cmp::min(
                                state.selected_index + 10,
                                items.len().saturating_sub(1),
                            );
                        }
                        TuiEvent::ToggleExpanded if matches!(state.mode, TuiMode::Browse) => {
                            let items = state.get_visible_items();
                            if state.selected_index < items.len() {
                                let target_path = items[state.selected_index].node.path.clone();
                                toggle_expansion_by_path(&mut state.root_tree, &target_path);
                            }
                        }
                        TuiEvent::ExpandDir if matches!(state.mode, TuiMode::Browse) => {
                            let items = state.get_visible_items();
                            if state.selected_index < items.len() {
                                let node = items[state.selected_index].node;
                                if node.is_dir && !node.is_expanded {
                                    let target_path = node.path.clone();
                                    expand_dir_by_path(&mut state.root_tree, &target_path);
                                }
                            }
                        }
                        TuiEvent::CollapseDir if matches!(state.mode, TuiMode::Browse) => {
                            let items = state.get_visible_items();
                            if state.selected_index < items.len() {
                                let node = items[state.selected_index].node;
                                if node.is_dir && node.is_expanded {
                                    let target_path = node.path.clone();
                                    collapse_dir_by_path(&mut state.root_tree, &target_path);
                                }
                            }
                        }
                        TuiEvent::ToggleSelect if matches!(state.mode, TuiMode::Browse) => {
                            let items = state.get_visible_items();
                            if state.selected_index < items.len() {
                                let target_path = items[state.selected_index].node.path.clone();
                                toggle_selection_by_path(&mut state.root_tree, &target_path);
                            }
                        }
                        TuiEvent::SelectAll if matches!(state.mode, TuiMode::Browse) => {
                            select_all(&mut state.root_tree, true);
                        }
                        TuiEvent::DeselectAll if matches!(state.mode, TuiMode::Browse) => {
                            select_all(&mut state.root_tree, false);
                        }
                        TuiEvent::Preview if matches!(state.mode, TuiMode::Browse) => {
                            let items = state.get_visible_items();
                            if state.selected_index < items.len() {
                                let node = items[state.selected_index].node;
                                if !node.is_dir {
                                    if let Some(entry) = &node.entry {
                                        match preview_reader.load_file(entry) {
                                            Ok(content) => {
                                                state.set_preview(content);
                                                state.mode = TuiMode::Preview;
                                            }
                                            Err(e) => {
                                                state.show_error(format!("Preview failed: {}", e));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        TuiEvent::Extract if matches!(state.mode, TuiMode::Browse) => {
                            state.extract_selected = state.root_tree.collect_selected();
                            if !state.extract_selected.is_empty() {
                                in_text_mode = true;
                                state.input_buffer = state.extract_output_dir.clone();
                                state.mode = TuiMode::ExtractDialog;
                            } else {
                                state.show_error("No files selected".to_string());
                            }
                        }
                        TuiEvent::MoveUp if matches!(state.mode, TuiMode::Preview) => {
                            if state.preview_scroll > 0 {
                                state.preview_scroll -= 1;
                            }
                        }
                        TuiEvent::MoveDown if matches!(state.mode, TuiMode::Preview) => {
                            state.preview_scroll += 1;
                        }
                        TuiEvent::MoveToStart if matches!(state.mode, TuiMode::Preview) => {
                            state.preview_scroll = 0;
                        }
                        TuiEvent::MoveToEnd if matches!(state.mode, TuiMode::Preview) => {
                            if let Some(content) = &state.preview_content {
                                let lines = preview::PreviewReader::get_lines(content, 100);
                                state.preview_scroll = lines.len().saturating_sub(10);
                            }
                        }
                        TuiEvent::PageUp if matches!(state.mode, TuiMode::Preview) => {
                            state.preview_scroll = state.preview_scroll.saturating_sub(10);
                        }
                        TuiEvent::PageDown if matches!(state.mode, TuiMode::Preview) => {
                            state.preview_scroll += 10;
                        }
                        TuiEvent::Quit if matches!(state.mode, TuiMode::Preview) => {
                            state.clear_preview();
                            state.mode = TuiMode::Browse;
                        }
                        TuiEvent::Cancel if matches!(state.mode, TuiMode::Preview) => {
                            state.clear_preview();
                            state.mode = TuiMode::Browse;
                        }
                        TuiEvent::Quit => break,

                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}

fn perform_extract(
    state: &mut TuiState,
    _preview_reader: &mut preview::PreviewReader,
) -> Result<()> {
    let mut extractor = extract::Extractor::new(
        state.file_handle.try_clone()?,
        state.header.data_section_start,
    );
    let mut entries_to_extract = Vec::new();

    // Extract only the selected files
    for selected_path in &state.extract_selected {
        find_entry_by_path(&state.root_tree, selected_path, &mut entries_to_extract);
    }

    let count = entries_to_extract.len();
    extractor.extract_files(entries_to_extract, &state.extract_output_dir)?;
    state.show_success(format!(
        "Successfully extracted {} files to:\n{}",
        count,
        state.extract_output_dir
    ));

    Ok(())
}

fn find_entry_by_path(
    node: &tree::TreeNode,
    target_path: &str,
    found: &mut Vec<crate::models::archive::ArchiveIndexEntry>,
) {
    if !node.is_dir && node.path == target_path {
        if let Some(entry) = &node.entry {
            found.push(entry.clone());
        }
    }

    for child in &node.children {
        find_entry_by_path(child, target_path, found);
    }
}

fn toggle_expansion_by_path(node: &mut tree::TreeNode, target_path: &str) {
    if node.path == target_path {
        node.toggle_expanded();
        return;
    }

    for child in &mut node.children {
        toggle_expansion_by_path(child, target_path);
    }
}

fn toggle_selection_by_path(node: &mut tree::TreeNode, target_path: &str) {
    if node.path == target_path {
        node.toggle_selected();
        return;
    }

    for child in &mut node.children {
        toggle_selection_by_path(child, target_path);
    }
}

fn select_all(node: &mut tree::TreeNode, selected: bool) {
    node.selected = selected;
    for child in &mut node.children {
        select_all(child, selected);
    }
}

fn expand_dir_by_path(node: &mut tree::TreeNode, target_path: &str) {
    if node.path == target_path && node.is_dir {
        node.is_expanded = true;
        return;
    }

    for child in &mut node.children {
        expand_dir_by_path(child, target_path);
    }
}

fn collapse_dir_by_path(node: &mut tree::TreeNode, target_path: &str) {
    if node.path == target_path && node.is_dir {
        node.is_expanded = false;
        return;
    }

    for child in &mut node.children {
        collapse_dir_by_path(child, target_path);
    }
}

use std::io::Read;
