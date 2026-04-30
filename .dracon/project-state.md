# Project State

## Current Focus
Adds a new sidebar view mode for displaying a file‑system tree and cycles sidebar scopes with Ctrl + b.

## Completed
- [x] feat(sidebar): introduce `SidebarScope::Tree` and UI rendering for a navigable file‑tree view.
- [x] feat(sidebar): enable cycling through sidebar scopes (All → Favorites → Remotes → Tree) via Ctrl + b while the sidebar is visible.
- [x] fix(sidebar-toggle): preserve sidebar visibility toggle when Ctrl + b is not pressed.
