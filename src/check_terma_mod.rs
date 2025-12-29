use terma::input::parser::{Event, MouseButton, KeyCode, KeyModifiers};
use terma::integration::ratatui::TermaBackend;

pub fn check() {
    // Check KeyModifiers
    let _ = KeyModifiers::CONTROL;
    
    // Check read function - guessing names
    // let _ = terma::read(); 
    // let _ = terma::read_event();
    // let _ = terma::input::read();
}