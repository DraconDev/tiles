# Project State

## Current Focus
Implement search debounce to prevent immediate refresh on each keystroke

## Completed
- [x] Added `SEARCH_DEBOUNCE_MS` constant (300 ms) and `is_valid_search_char` helper function
- [x] Replaced raw character validation with `is_valid_search_char` check
- [x] Introduced `search_debounce_until` field and logic to delay `RefreshFiles` events
- [x] Updated multiple event‑handling branches to set debounce timer and conditionally send refresh
