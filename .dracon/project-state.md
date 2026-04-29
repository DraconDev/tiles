# Project State

## Current Focus
Update app state management by removing disk mounting events and adding process termination handling, while eliminating import servers UI option

## Completed
- [x] Removed `MountDisk` event from `AppEvent` enum and replaced with `KillProcess(u32)` for process management
- [x] Removed `DropTarget::ImportServers` from `DragTarget` enum in UI, disabling import servers functionality
