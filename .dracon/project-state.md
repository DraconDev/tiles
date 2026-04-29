# Project State

## Current Focus
Handle context‑menu actions by converting Key events to InputEvent and routing through the runtime event system.

## Completed
- [x] Convert undo action key event handling from `Event::Key` to `InputEvent::Key` and use `to_runtime_event(&event)`.
- [x] Convert redo action key event handling similarly.
