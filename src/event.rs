use dracon_terminal_engine::input::event::Event as RuntimeEvent;
use dracon_terminal_engine::input::mapping::Event;

pub fn convert_event(evt: RuntimeEvent) -> Option<Event> {
    Some(dracon_terminal_engine::input::mapping::from_runtime_event(
        evt,
    ))
}
