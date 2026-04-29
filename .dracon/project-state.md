# Project State

## Current Focus
Refactor UI to use `FileState` instead of `Preview`, improving state management.

## Completed
- [x] Refactor `src/ui/mod.rs` to utilize `pane.current_state_mut()` and handle preview.
- [x] Update `breadcrumbs.rs` UI logic to reference file content status.
- [x] Update `sidebar.rs` to use `FileState` in place of `Preview` for file path operations.
