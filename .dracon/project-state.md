# Project State

## Current Focus
Refactored double-click detection to use a named constant for the timeout duration

## Completed
- [x] Added `DOUBLE_CLICK_MS` constant with value 500ms
- [x] Updated `is_double_click` to use the constant instead of magic number
- [x] Improved maintainability by centralizing the timeout configuration
