# Project State

## Current Focus
Implement editor‑cut action in context‑menu and refine editor‑state path handling for Save‑As

## Completed
- [x] Added EditorCut handling that copies selected text to clipboard and deletes selection via `get_active_editor_mut`
- [x] Refactored `get_active_editor_mut` to move preview‑editor lookup after primary pane logic
- [x] Refactored `get_active_editor_path` to move preview‑path lookup after primary pane logic
- [x] Updated Save‑As handler to update `preview.path` when saving over the original file path
