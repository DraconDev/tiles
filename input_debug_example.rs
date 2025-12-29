use std::io::{self, Read, Write};
use terma::input::parser::Parser;
use terma::Terma;

fn main() -> io::Result<()> {
    println!("Preparing to enter Raw Mode...");
    println!("Type 'q' to quit.");
    std::thread::sleep(std::time::Duration::from_secs(1));

    let stdout = io::stdout();
    let mut term = Terma::new(stdout)?;

    // Enable SGR Mouse (1006) + Any Event (1003)
    // We strictly write ANSI commands manually to prove low-level control
    // \x1b[?1000h: Press/Release
    // \x1b[?1003h: All motion (Warning: Spammy)
    // \x1b[?1006h: SGR Extended Mode (Required for Side Buttons!)
    // \x1b[>1u: Kitty Keyboard Protocol (Disambiguate keys)
    // \x1b[?1004h: Focus Reporting (In/Out)
    // \x1b[?2004h: Bracketed Paste Mode
    write!(term, "\x1b[?1000h\x1b[?1006h\x1b[>1u\x1b[?1004h\x1b[?2004h")?;
    write!(term, "\x1b[2J\x1b[H")?;
    write!(term, "Input Debugger. Press keys or click mouse.\r\n")?;
    term.flush()?;

    let mut parser = Parser::new();
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    let mut buf = [0u8; 128];

    loop {
        let n = handle.read(&mut buf)?;
        if n == 0 {
            break;
        }

        for &byte in &buf[..n] {
            if let Some(event) = parser.advance(byte) {
                // Quit on 'q'
                if let terma::input::parser::Event::Key(terma::input::parser::KeyEvent {
                    code: terma::input::parser::KeyCode::Char('q'),
                    ..
                }) = event
                {
                    return Ok(());
                }

                // Print event details
                // formatted with \r\n for raw mode cleanliness
                write!(term, "{:?}\r\n", event)?;
            }
        }
        term.flush()?;
    }

    Ok(())
}
