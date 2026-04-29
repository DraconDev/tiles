# Project State

## Current Focus
Adds a RefreshFiles event and a PreviewRequested event when creating a file, using a captured focused_pane variable.

## Completed
- [x] Extract focused_pane from app.lock() and store it in a variable
- [x] Send AppEvent::RefreshFiles with the focused_pane index
- [x] Send AppEvent::PreviewRequested with the focused_pane index and file path
