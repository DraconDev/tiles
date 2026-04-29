# Project State

## Current Focus
Handle editor context‑menu actions directly by processing each action individually instead of using a dedicated `ContextMenuTarget::Editor` branch.

## Completed
- [x] Removed the empty `ContextMenuTarget::Editor => vec![]` branch
- [x] Refactored `handle_context_menu_action` to match on `ContextMenuAction` for editor actions
- [x] Implemented `EditorSelectAll` to select all text in the active editor
- [x] Implemented `EditorCopy` to copy selected text to clipboard
- [x] Implemented `EditorPaste` to paste primary selection into the active editor
- [x] Implemented `EditorUndo` and `EditorRedo` via Ctrl+Z / Ctrl+Y key events
- [x] Implemented `Save` action to send a save event with file path and content
- [x] Implemented `Run` action to spawn a terminal with the file’s run command
