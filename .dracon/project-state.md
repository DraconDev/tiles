# Project State

## Current Focus
Remove the `if should_refresh` guard around the search debounce assignment and always schedule debounce on file events.

## Completed
- [x] Removed the conditional `if should_refresh { … }` block and moved `fs.search_debounce_until = Some(now + Duration::from_millis(SEARCH_DEBOUNCE_MS));` outside the condition
- [x] Simplified the debounce logic so it always sets the debounce timer when handling file events
- [x] Updated Cargo.lock (binary change) to reflect dependency updates
