# Project State

## Current Focus
Add auto_save persistence and smart date formatting for file timestamps.

## Completed
- [x] Add `auto_save` field to `PersistentState` with default true
- [x] Persist `auto_save` when serializing state
- [x] Initialize `app.auto_save` from loaded state in `setup_app`
- [x] Refactor `format_modified_time` to support smart date logic
- [x] Use `app.smart_date` in `draw_file_view` for modified and created timestamps
