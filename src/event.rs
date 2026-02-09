use dracon_tui_input::Event;
use terma::input::event::Event as RuntimeEvent;

pub fn convert_event(evt: RuntimeEvent) -> Option<Event> {
    Some(dracon_tui_input::from_runtime_event(evt))
}
