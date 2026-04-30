# Project State

## Current Focus
Render a bulk rename modal in the UI when BulkRename mode is active, displaying selected file count, find/replace fields, a preview of the first five renamed files, and action hints.

## Completed
- [x] Implemented `draw_bulk_rename_modal` to render a centered modal with borders, input previews, file count, and a live regex-based rename preview.
- [x] Added hint footer inside the modal (`Enter = Apply  Esc = Cancel`).
- [x] Integrated pattern and replacement handling from `AppMode::BulkRename` and displayed a preview of up to five affected files.
