# Changelog

All notable changes to this project will be documented in this file.

## [4.10.0] - Editor Enhancements

### Added
- **Run Files** — Press `Ctrl+Enter` to run the current file. Supports:
  - Scripts with shebang (`#!/bin/bash`, `#!/usr/bin/env python3`, etc.)
  - Rust projects (detects `Cargo.toml` by walking up the directory tree)
  - Extension-mapped executables: Python (`python3`), Node.js (`node`), Ruby (`ruby`), Perl (`perl`), PHP (`php`), Lua (`lua`), R (`Rscript`), Go (`go run`)
  - Run opens in a new terminal tab so the editor stays visible
- **Editor Context Menu** — Right-click in the editor area to access:
  - **Editable files**: Cut, Copy, Paste, Undo, Redo, Select All, Save, Run
  - **Read-only files** (Viewer mode, git diffs, binary files): Copy, Select All, Run
- **Unified Clipboard** — Copy/Cut stores in an internal buffer AND syncs to system clipboard. Paste reads from internal buffer first, falls back to system clipboard.
- **Editor Footer Bar** — Shows live cursor position (`Ln X, Col Y`), language, and modified indicator (`●`)
- **Modified Indicator on Tabs** — Amber dot appears on tab labels when a file has unsaved changes
- **Auto-Open New Files** — `Ctrl+N` creates a new file and immediately opens it in the editor
- **Save-As Path Sync** — After Save-As, the editor title and tab labels update to reflect the new filename

### Editor Shortcuts
| Key | Action |
|-----|--------|
| `Alt+↑` / `Alt+↓` | Move current line up/down |
| `Ctrl+D` | Duplicate current line |
| `Ctrl+K` | Kill to end of line |
| `Ctrl+U` | Kill to start of line |
| `Ctrl+A` | Select all |
| `Ctrl+Home` | Jump to document start |
| `Ctrl+End` | Jump to document end |

### Changed
- Tab limit increased from 3 to 8 tabs per pane
- Context menu in read-only editor modes now only shows relevant actions (no Cut/Paste/Save)

### Fixed
- Save-As now properly updates the editor path in all cases
- Editor helper functions now correctly prefer the active pane editor over stale full-screen editor state

## Prior Versions

See the git history for earlier changelog entries.
