
use terma::{Event, MouseButton, KeyCode};

pub fn check() {
    let event: Event = unsafe { std::mem::zeroed() };
    match event {
        Event::Mouse { button, row, column, .. } => {
            match button {
                MouseButton::Left => {},
                MouseButton::Right => {},
                MouseButton::Middle => {},
                MouseButton::Back => {},
                MouseButton::Forward => {},
                _ => {},
            }
        }
        Event::Key(key) => {
            let code = key.code;
            match code {
                KeyCode::Char(_) => {},
                KeyCode::Enter => {},
                _ => {},
            }
        }
        _ => {}
    }
}
