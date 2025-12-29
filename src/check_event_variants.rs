
use terma::input::parser::MouseButton;

pub fn check() {
    let b: MouseButton = unsafe { std::mem::zeroed() };
    match b {
        MouseButton::Left => {},
        MouseButton::Right => {},
        MouseButton::Middle => {},
        MouseButton::Back => {},
        MouseButton::Forward => {},
        // Force error to list variants
    }
}
