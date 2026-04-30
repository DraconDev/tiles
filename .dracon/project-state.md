# Project State

## Current Focus
Editor enhancements — context menu, unified clipboard, modified indicators, run command, and editor shortcuts.

## Completed
- [x] Updated dependencies and regenerated Cargo.lock
- [x] Added ContextMenuTarget::Editor and related actions (Save, Cut, Copy, Paste, Undo, Redo, Select All)
- [x] Implemented unified clipboard handling with system clipboard sync
- [x] Added footer bar showing line, column, language, modified indicator, and save/run shortcuts
- [x] Added amber modified indicator on tabs for active and inactive editors
- [x] Synced preview path and editor state after Save‑As
- [x] Added Ctrl+N auto‑open new file in editor view
- [x] Wired Run command with hotkey Ctrl+Enter and shebang/extension detection
- [x] Implemented top‑tier editor shortcuts (duplicate line, move line up/down, etc.)
- [x] Increased tab limit from 3 to 8
- [x] Refactored clipboard operations to be borrow‑scope safe and extracted helper functions
- [x] Fixed all build warnings; 0 errors, 0 warnings; all 37 tests passing
