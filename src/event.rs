use dracon_terminal_engine::input::Event;
use dracon_terminal_engine::input::event::Event as RuntimeEvent;

pub fn convert_event(evt: RuntimeEvent) -> Option<Event> {
    Some(dracon_terminal_engine::input::mapping::from_runtime_event(evt))
}
