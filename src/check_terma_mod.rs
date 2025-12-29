
use terma::input::parser::{Event, MouseButton, KeyCode, KeyModifiers};
use terma::integration::ratatui::TermaBackend;

pub fn check() {
    let _ = KeyModifiers::CONTROL;
    let _ = terma::read();
}
