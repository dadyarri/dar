pub mod state;

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
    pub fn run(state: AppState) -> Result<()> {
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

        let result = run_loop(&mut terminal, &state);

        // Always restore the terminal before returning.
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        result
    }
}

fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    state: &AppState,
) -> Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, state))?;

        if let Event::Key(key) = event::read()? {
            // Only act on key-press events; ignore repeat/release on terminals that report them.
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match (key.code, key.modifiers) {
                (KeyCode::Char('q'), _)
                | (KeyCode::Char('Q'), _)
                | (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,
                _ => {}
            }
        }
    }
    Ok(())
}

fn draw(frame: &mut ratatui::Frame, state: &AppState) {
    use ratatui::layout::{Alignment, Constraint, Layout};
    use ratatui::widgets::{Block, Borders, Paragraph};

    let area = frame.area();
    let locale = state.locale.as_str();

    // Center the block vertically (fixed height) and horizontally (60 % width).
    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(5),
        Constraint::Fill(1),
    ])
    .split(area);

    let horizontal = Layout::horizontal([
        Constraint::Percentage(20),
        Constraint::Percentage(60),
        Constraint::Percentage(20),
    ])
    .split(vertical[1]);

    let unknown = rust_i18n::t!("tui.inspect.unknown_filename", locale = locale);
    let filename = state
        .archive_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&unknown);

    let n = state.entries.len();
    let body = rust_i18n::t!("tui.inspect.skeleton_hint", locale = locale, n = n).to_string();

    let block = Block::default()
        .title(format!(" {} ", filename))
        .borders(Borders::ALL);

    let paragraph = Paragraph::new(body).block(block).alignment(Alignment::Center);

    frame.render_widget(paragraph, horizontal[1]);
}



