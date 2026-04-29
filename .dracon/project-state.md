# Project State

## Current Focus
Implement clipboard integration and conditional context‑menu actions based on editor read‑only state.

## Completed
- [x] Store copied/selected text in `app.editor_clipboard` during copy and cut operations
- [x] Retrieve `app.editor_clipboard` (falling back to system clipboard) for paste operation and insert it into the editor
- [x] Generate context‑menu actions dynamically, excluding cut/paste/undo/redo when the editor is read‑only
- [x] Apply the conditional actions logic in both mouse‑click branches of `handle_editor_mouse`
