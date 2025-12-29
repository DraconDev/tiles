use terma::input::parser::Event;

pub fn check() {
    let e: Event = unsafe { std::mem::zeroed() };
    if let Event::Mouse { kind, .. } = e {
        // check if 'kind' exists
    }
}