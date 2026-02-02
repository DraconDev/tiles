# Tiles Refactor Blueprint 󱐋

## Current Bottlenecks
- `src/main.rs` (~5,500 lines): Handles event loop, input routing, and core state coordination.
- `src/ui/mod.rs` (~4,000 lines): Contains all rendering logic for every mode (Editor, Monitor, Files, Settings, etc.).

## Target Architecture

### 1. Event Handling (`src/events/`)
Decompose the massive `handle_event` function in `main.rs`.
- `mod.rs`: Main entry point for event routing.
- `input.rs`: Raw key/mouse to logical command mapping.
- `editor.rs`: Dedicated handlers for `AppMode::Editor`.
- `file_manager.rs`: Handlers for `CurrentView::Files`.
- `monitor.rs`: Handlers for `CurrentView::Processes`.

### 2. UI Modules (`src/ui/`)
Decompose `src/ui/mod.rs` by functional area.
- `mod.rs`: High-level layout and entry point (`draw` function).
- `panes/`: Logic for rendering individual file panes and breadcrumbs.
- `modals/`: Centralized rendering for all dialogs (Rename, New File, Delete, etc.).
- `pages/`: Full-screen views like `System Monitor`, `Git History`, and `Settings`.
- `editor/`: Unified logic for the full-screen and preview editors.

### 3. State Management (`src/app.rs` refinement)
- Extract nested structs (like `FileState`, `SystemState`) into a new `src/state/` module to keep `app.rs` focused on the `App` coordinator.

## Phase 1: UI Decomposition (The "Easy" Wins)
1. Move all modal drawing functions to `src/ui/modals.rs`.
2. Move `draw_monitor_page` and its sub-functions to `src/ui/pages/monitor.rs`.
3. Move `draw_settings_modal` to `src/ui/pages/settings.rs`.

## Phase 2: Event Decomposition (The "Crucial" Fix)
1. Extract `AppMode` specific event handling into `src/events/`.
2. Reduce `main.rs` to just the `tokio` loop and high-level setup.
