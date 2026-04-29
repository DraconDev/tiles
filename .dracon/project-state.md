# Project State

## Current FocusMoved welcome‑pane rendering outside nested logic and refactored directory‑name extraction for cleaner UI handling.

## Completed
- [x] Renamed `welcome_path` to `welcome_name` and moved its rendering block to the end of the function.
- [x] Simplified directory name extraction using `fs.current_path.file_name()` directly instead of cloning via `pane.current_state().map`.
- [x] Updated path display to use `fs.current_path.to_string_lossy()` in the UI line.
- [x] Preserved welcome paragraph rendering when `welcome_name` is present, ensuring consistent UI feedback.
- [x] Adjusted surrounding code block structure to maintain correct control flow and early returns.
