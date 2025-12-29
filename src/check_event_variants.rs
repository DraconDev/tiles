
use terma::input::parser::KeyModifiers;

pub fn check() {
    let m: KeyModifiers = unsafe { std::mem::zeroed() };
    if let KeyModifiers { a, b } = m {}
}
