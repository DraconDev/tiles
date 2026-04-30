# Project State

## Current Focus
Added search debounce handling with an instant field and constant delay in FileState

## Completed
- [x] Introduced `search_debounce_until` field to track debounce timing
- [x] Added `SEARCH_DEBOUNCE_MS` constant set to 300 ms
- [x] Updated `FileState::new` to initialize `search_debounce_until` as `None`
