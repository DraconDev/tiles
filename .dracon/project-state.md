# Project State

## Current Focus
Added "Open With" context menu action handler for files

## Completed
- [x] Implemented handler for `ContextMenuAction::OpenWith` that:
  - Extracts file path from current file state
  - Switches app mode to `OpenWith` with the file path
  - Clears input buffer
```
