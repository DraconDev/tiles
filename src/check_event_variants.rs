
use terma::input::parser::MouseButton;

pub fn check() {
    let b = MouseButton::Left;
    match b {
        MouseButton::Left => {},
        MouseButton::Right => {},
        MouseButton::Middle => {},
        MouseButton::Release => {}, // Guess
        MouseButton::WheelUp => {}, // Guess
        MouseButton::WheelDown => {}, // Guess
        _ => {},
    }
}
