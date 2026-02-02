# Tiles Refactor Blueprint 󱐋

## Core Mandate: Full Terma Integration
- **Zero Cutting**: We are NOT moving away from `terma`. We are organizing Tiles to use `terma` widgets (TextEditor, TextInput, HotkeyHint) more efficiently.
- **Engine First**: All event handling must map directly to `terma::input::event::Event` variants.
- **Widget Consistency**: UI components in `src/ui/panes/` must remain compatible with `terma`'s rendering patterns.

## Current Status: Phase 1 & 2 (Stable) ✅
- **UI Modularized**: God-file `ui/mod.rs` decomposed. Original Footer, Header, and Mouse-interactive areas restored and linked to Terma-based logic.
- **Events Modularized**: Central `handle_event` in `main.rs` is now a thin wrapper. Logic moved to specialized sub-handlers in `src/events/`.

## Phase 3: State & Logic Hardening (CURRENT) 🚧
- [ ] **Consolidate State**: Extract `FileState`, `SelectionState`, and `RemoteSession` from `app.rs` into a new `src/state/` module.
- [ ] **Terma Widget Alignment**: Ensure `App` acts only as a coordinator, delegating specific state to `terma`-ready widgets.
- [ ] **Warning Cleanup**: Systematic removal of dead code and unused imports from the transition.
- [ ] **Verification**: Pass `cargo test` and `cargo check` after each state move.

## Phase 4: Performance & Polish 💎
- [ ] **Incremental Rendering**: Optimize `draw` calls to only update changed panes (leveraging `panes_needing_refresh`).
- [ ] **Selection Hardening**: Verify multi-pane drag-and-drop works flawlessly across all remote/local combinations.
- [ ] **Documentation**: Document the new module boundaries for future feature additions.
