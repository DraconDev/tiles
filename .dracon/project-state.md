# Project State

## Current Focus
Sanitize search text before updating filter and add debounce to delay refresh

## Completed
- [x] Filter text is filtered through `is_valid_search_char` to remove invalid characters
- [x] Search debounce timer is set to delay `RefreshFiles` event by `SEARCH_DEBOUNCE_MS`
