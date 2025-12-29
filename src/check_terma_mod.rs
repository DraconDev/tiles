use terma::{Event, MouseButton, KeyCode};

fn main() {
    let event: Event = unsafe { std::mem::zeroed() }; // Just for type checking, don't run this!
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
            // Check key structure
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