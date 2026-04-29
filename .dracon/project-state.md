# Project State

## Current Focus
Integrate preview handling into each file's state and update reload logic accordingly.

## Completed
- [x] fix(preview): move `preview` from `Pane` to `FileState`, changing its type to `Option<PreviewState>` and removing the now‑unused field from `Pane`.
- [x] fix(preview): update reload check in `run_tty` to obtain preview information via `pane.current_state()` instead of the removed `pane.preview`.
