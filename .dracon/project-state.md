# Project State

## Current Focus
feat(editor): add Ctrl+Tab and Ctrl+Shift+Tab shortcuts for cycling through editor pane tabs

## Completed
- [x] Add tab cycling with Ctrl+Tab (next tab, wraps to first when at end)
- [x] Add reverse tab cycling with Ctrl+Shift+Tab (previous tab, saturating subtraction)
- [x] Guard tab switching behind check for multiple tabs (only active when tab_count > 1)
