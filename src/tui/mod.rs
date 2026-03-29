pub mod state;

use crate::models::archive::CompressionMethod;
use crate::tui::state::AppState;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use eyre::Result;
use ratatui::{Terminal, backend::CrosstermBackend};
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

                _ => {}
            }
        }
    }
    Ok(())
}

fn move_up(state: &mut AppState) {
    if state.entries.is_empty() {
        return;
    }
    let new = state
        .list_state
        .selected()
        .map(|i| i.saturating_sub(1))
        .unwrap_or(0);
    state.list_state.select(Some(new));
}

fn move_down(state: &mut AppState) {
    let n = state.entries.len();
    if n == 0 {
        return;
    }
    let new = state
        .list_state
        .selected()
        .map(|i| (i + 1).min(n - 1))
        .unwrap_or(0);
    state.list_state.select(Some(new));
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn draw(frame: &mut ratatui::Frame, state: &mut AppState) {
    use ratatui::layout::{Constraint, Layout};
    use ratatui::style::{Modifier, Style};
    use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

    let locale = state.locale.as_str();

    // Top area = list, bottom row = status bar.
    let chunks = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)])
        .split(frame.area());
    let (main_area, status_area) = (chunks[0], chunks[1]);

    // Build list rows: "path  compressed_size (algorithm)"
    let items: Vec<ListItem> = state
        .entries
        .iter()
        .map(|w| {
            let size = human_size(w.entry.compressed_size);
            let algo = algorithm_name(w.entry.compression_method);
            ListItem::new(format!("{}  {} ({})", w.path, size, algo))
        })
        .collect();

    let filename = state
        .archive_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?");

    let list = List::new(items)
        .block(
            Block::default()
                .title(format!(" {} ", filename))
                .borders(Borders::ALL),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD));

    frame.render_stateful_widget(list, main_area, &mut state.list_state);

    // Status bar
    let pos = state.list_state.selected().map(|i| i + 1).unwrap_or(0);
    let total = state.entries.len();
    let status =
        rust_i18n::t!("tui.inspect.status_bar", locale = locale, pos = pos, total = total)
            .to_string();
    frame.render_widget(Paragraph::new(status), status_area);
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
