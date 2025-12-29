
use terma::input::parser::KeyModifiers;

pub fn check() {
    let m = KeyModifiers::empty();
    if m.contains(KeyModifiers::CONTROL) {}
    if m.contains(KeyModifiers::ALT) {}
    if m.contains(KeyModifiers::SHIFT) {}
}
