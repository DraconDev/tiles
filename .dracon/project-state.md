# Project State

## Current Focus
Migrate preview state from Pane-level to per-tab FileState to enable independent previews across tabs.

## Completed
- [x] Remove `preview` field from `Pane` struct and store it inside each `FileState` (tab) instead.
- [x] Update editor event handling to resolve preview/editor via `current_state_mut()` on the active tab.
- [x] Update file-manager escape handling to clear preview across all tabs in every pane.
- [x] Adjust tab-close logic to reset preview on the affected tab and maintain selection integrity.
- [x] Refactor new-file/directory creation to derive base directory from the active tab’s preview or current path.
- [x] Update config serialization to drop the obsolete `preview` field from saved pane state.
- [x] Synchronize mouse and keyboard editor handlers with per-tab preview state.
- [x] Touch `Cargo.lock` to reflect resolved dependency versions.
