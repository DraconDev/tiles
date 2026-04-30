# Project State

## Current Focus
Introduce search debounce refresh handling and fix sidebar index updates

## Completed
- [x] Added `needs_refresh` flag to track when a file search requires a refresh
- [x] Modified refresh event sending to occur only when `needs_refresh` is true
- [x] Reset `app.sidebar_index` to 0 appropriately when handling sidebar navigation and after refresh events
