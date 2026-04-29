# Project State

## Current Focus
Implemented ContextMenuTarget::Editor handling and refactored Run/RunTerminal actions for file targets.

## Completed
- [x] Refactored ContextMenuAction::Run and RunTerminal to use a match on ContextMenuTarget, extracting the file path via `app.current_file_state().files.get(*idx)`.
- [x] Added early return when the target path is a directory.
- [x] Moved the `remote_session` lookup before invoking `get_run_command`.
- [x] Preserved the status‑message logic for missing run commands and directory cases.
- [x] Added a new branch for `ContextMenuTarget::Editor` that obtains the active editor path using `get_active_editor_path(app)` and runs the same command flow.
- [x] Unified handling of `SpawnTerminal` events and status‑message emission for both file and editor paths.
- [x] Maintained existing behavior for unknown file extensions and error reporting.
