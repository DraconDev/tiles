use terma::input::event::Event as TermaEvent;
use terma::input::event::{KeyEvent, MouseEvent, Event};

pub fn convert_event(evt: TermaEvent) -> Option<Event> {
    match evt {
        TermaEvent::Key(k) => Some(Event::Key(k)),
        TermaEvent::Mouse(m) => Some(Event::Mouse(m)),
        TermaEvent::Resize(w, h) => Some(Event::Resize(w, h)),
        _ => None,
    }
}
