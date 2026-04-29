# Project State

## Current Focus
Switched event imports to the new `input::mapping` module and added `to_runtime_event` import.

## Completed
- [x] Replaced import of `InputEvent`, `KeyCode`, `KeyEvent`, `KeyEventKind`, `KeyModifiers` from `dracon_terminal_engine::contracts` with the new path `dracon_terminal_engine::input::mapping`, renaming `Event` to `InputEvent`
- [x] Added import of `to_runtime_event` from the same module
