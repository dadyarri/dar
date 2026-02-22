use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug)]
pub enum TuiEvent {
    MoveUp,
    MoveDown,
    MoveToStart,
    MoveToEnd,
    PageUp,
    PageDown,
    ToggleExpanded,
    ExpandDir,
    CollapseDir,
    ToggleSelect,
    SelectAll,
    DeselectAll,
    Preview,
    Extract,
    Help,
    Quit,
    TextInput(char),
    Backspace,
    Enter,
    Cancel,
}

pub fn process_key_event(key: KeyEvent, in_text_mode: bool) -> Option<TuiEvent> {
    if in_text_mode {
        match key.code {
            KeyCode::Enter => Some(TuiEvent::Enter),
            KeyCode::Esc => Some(TuiEvent::Cancel),
            KeyCode::Backspace => Some(TuiEvent::Backspace),
            KeyCode::Char(c) => Some(TuiEvent::TextInput(c)),
            _ => None,
        }
    } else {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => Some(TuiEvent::MoveUp),
            KeyCode::Down | KeyCode::Char('j') => Some(TuiEvent::MoveDown),
            KeyCode::Home | KeyCode::Char('g') => Some(TuiEvent::MoveToStart),
            KeyCode::End | KeyCode::Char('G') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                Some(TuiEvent::MoveToEnd)
            }
            KeyCode::PageUp => Some(TuiEvent::PageUp),
            KeyCode::PageDown => Some(TuiEvent::PageDown),
            KeyCode::Right | KeyCode::Char('l') => Some(TuiEvent::ExpandDir),
            KeyCode::Left | KeyCode::Char('h') => Some(TuiEvent::CollapseDir),
            KeyCode::Enter => Some(TuiEvent::ToggleExpanded),
            KeyCode::Char(' ') => Some(TuiEvent::ToggleSelect),
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(TuiEvent::SelectAll)
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(TuiEvent::DeselectAll)
            }
            KeyCode::Char('p') => Some(TuiEvent::Preview),
            KeyCode::Char('e') => Some(TuiEvent::Extract),
            KeyCode::Char('?') => Some(TuiEvent::Help),
            KeyCode::Char('q') | KeyCode::Esc => Some(TuiEvent::Quit),
            _ => None,
        }
    }
}
