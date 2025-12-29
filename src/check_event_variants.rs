
use terma::input::parser::Event;

pub fn check() {
    let e: Event = unsafe { std::mem::zeroed() };
    match e {
        Event::Mouse { .. } => {},
        Event::Key(_) => {},
        Event::Resize { .. } => {},
        Event::Paste(_) => {},
        _ => {}, // Check for other variants
    }
}
