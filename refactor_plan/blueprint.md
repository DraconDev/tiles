# Tiles Refactor Blueprint 󱐋

## Status: Phase 1 & 2 Complete ✅
- **UI Decomposition**: `src/ui/mod.rs` reduced from 4k lines to ~200. Logic moved to `modals.rs`, `pages/`, and `panes/`.
- **Event Decomposition**: `handle_event` moved from `main.rs` to `src/events/` with specialized sub-handlers.
- **System Logic**: Telemetry state updates moved to `src/modules/system.rs`.

## Phase 3: Verification & Stabilization (CURRENT)
- [ ] Implement unit tests for `src/events/` routing.
- [ ] Verify `FileState` and `SelectionState` logic in `src/app.rs`.
- [ ] Clean up unused imports and dead code warnings.
- [ ] Verify mouse coordinate mapping in refactored UI panes.

## Phase 4: State Management Refinement
- [ ] Extract `FileState`, `SystemState`, and `RemoteSession` into `src/state/`.
- [ ] Decouple `App` coordinator from specific widget states.

## Testing Strategy
1. **Event Routing**: Ensure `handle_event` correctly delegates based on `AppMode` and `CurrentView`.
2. **Editor Logic**: Test clipboard operations and text manipulation in `src/events/editor.rs`.
3. **File Manager**: Test directory navigation and selection logic.