# Project State

## Current Focus
Remove unused drag-and-drop target variants (`Pane` and `RemotesHeader`) and their associated UI rendering logic

## Completed
- [x] Remove `DropTarget::Pane(usize)` enum variant from state module
- [x] Remove `DropTarget::RemotesHeader` enum variant from state module
- [x] Remove pane border highlight styling for drag-over state in file view rendering
- [x] Remove footer dropdown text labels for Pane and RemotesHeader targets
- [x] Remove empty catch-all match arm in event handler
