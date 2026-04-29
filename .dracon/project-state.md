# Project State

## Current Focus
Implement right‑click context menu for the editor with cut, copy, paste, undo, redo, select all, save, and run actions.

## Completed
- [x] Added right‑click handling in `src/events/editor.rs` that opens a context menu when the right mouse button is pressed inside the editor area, populating it with cut, copy, paste, undo, redo, select all, save, and run actions.
- [x] Extended `ContextMenuAction` enum in `src/state/mod.rs` with `Undo` and `Redo` variants to support generic undo/redo actions.
- [x] Updated `Cargo.lock` to reflect resolved dependency versions after recent refactor.

## Plan
- [ ] Integrate context menu actions into the application's action dispatch so clicking menu items triggers the corresponding editor operations.
- [ ] Bind the context menu to the editor's selection state and ensure proper positioning relative to the mouse cursor.
- [ ] Add unit tests for the right‑click context menu behavior and edge cases such as clicks outside the editor bounds.
- [ ] Polish UI feedback (e.g., highlight selected menu item) and ensure accessibility.
