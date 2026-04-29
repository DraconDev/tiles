# Project State

## Current Focus
Centralize selection retrieval and improve clipboard handling for editor context‑menu actions.

## Completed
- [x] Refactor copy action to extract selected text via a temporary variable and gracefully handle a missing editor
- [x] Refactor cut action similarly, add deletion of the selection after copying, and handle missing editor gracefully
- [x] Refactor paste action to use clipboard or system clipboard text, insert it into the active editor, and mark the document as modified
- [x] Remove duplicated clipboard‑copy logic from copy, cut, and paste branches
- [x] Update Cargo.lock with resolved dependency versions
