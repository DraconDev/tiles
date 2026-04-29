# Project State

## Current Focus
Add comprehensive editor enhancements including context menu, unified clipboard, footer UI, run command, shortcuts, and tab‑limit expansion.

## Completed
- [x] Added Navigation shortcuts table (h/j/k/l, Enter, Backspace, Tab)
- [x] Added Editor shortcuts table (e.g., Ctrl+E, Ctrl+Enter, Alt+↑/↓)
- [x] Added Other Views shortcuts table (Ctrl+G, Ctrl+D, Ctrl+L)
- [x] Removed redundant “Tab | Switch panes” entry
- [x] Implemented Editor Context Menu with Cut, Copy, Paste, Undo, Redo, Select All, Save, Run (editable) and limited actions for read‑only files
- [x] Introduced `ContextMenuTarget::Editor` and new actions: Save, EditorCut, EditorCopy, EditorPaste, EditorUndo, EditorRedo, EditorSelectAll
- [x] Implemented Unified Clipboard: internal buffer sync with system clipboard, copy/cut write to both, paste reads internal buffer first
- [x] Added Editor Footer Bar showing live cursor position, language, modified indicator (●), Save and Run hints
- [x] Added Modified Indicator amber ● on tabs when `editor.modified` is true (active and inactive)
- [x] Synced Save‑As path updates to both pane preview and `app.editor_state`, updating title and tabs
- [x] Auto‑open new file on Ctrl+N, switch to editor view, and fire `PreviewRequested`
- [x] Implemented Run command feature: Ctrl+Enter hotkey, detection of shebang, Cargo.toml, and extension‑mapped interpreters; opens in new terminal tab; footer hint added
- [x] Added Editor shortcuts: Alt+↑/↓ move line, Ctrl+D duplicate, Ctrl+K kill to end, Ctrl+U kill to start, Ctrl+A select all, Ctrl+Home/End navigate document
- [x] Increased tab limit from 3 to 8 using `MAX_TABS` constant
- [x] Ensured all clipboard operations use borrow‑scope‑safe pattern and helper functions avoid duplicate logic
- [x] Build passes with 0 errors and 0 warnings; all 37 tests pass.
