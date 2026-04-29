# Project State

## Current Focus
feat(save-as): Add "Save As" functionality with Ctrl+Shift+S shortcut for saving files under a different path

## Completed
- [x] Add `SaveAs(PathBuf)` variant to `AppMode` enum in state module
- [x] Implement Ctrl+Shift+S keyboard shortcut in editor to trigger Save As mode
- [x] Add `handle_save_as_keys` function to process Save As modal input (Esc to cancel, Enter to save)
- [x] Add `draw_save_as_modal` UI component with yellow-bordered rounded dialog for path input
- [x] Handle path resolution logic for both absolute and relative paths in Save As
