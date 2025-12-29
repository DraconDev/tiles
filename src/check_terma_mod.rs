use terma::input::parser::Parser;

pub fn check() {
    let mut parser = Parser::new();
    let _ = parser.next();
    // let _ = parser.read_event();
}