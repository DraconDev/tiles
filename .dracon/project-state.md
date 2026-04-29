# Project State

## Current Focus
Implemented editor context‑menu handling for copy, paste, undo, redo, save, and run actions.

## Completed
- [x] Added `dracon_terminal_engine::contracts` imports for key‑event contracts.
- [x] Implemented editor‑target context‑menu branch with full action handling.
- [x] Integrated clipboard copy/paste via `copy_text_to_clipboard` and primary selection.
- [x] Added undo/redo via simulated Ctrl+Z/Ctrl+Y key events.
- [x] Added save action that sends `AppEvent::SaveFile` through the event channel.
- [x] Added run action that resolves and spawns a terminal with the appropriate command.
- [x] Updated `ContextMenuTarget` handling to route `Editor` actions to the active editor.
