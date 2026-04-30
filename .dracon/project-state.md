# Project State

## Current Focus
Harden image preview rendering: prevent divide-by-zero and overflow, use saturating arithmetic, and pass a reference to the block widget.

## Completed
- [x] Guard image scaling against zero width/height and clamp scale to safe bounds (0.1 minimum) while preserving aspect ratio.
- [x] Replace raw arithmetic with saturating operations for area and offset calculations to avoid underflow/overflow.
- [x] Derive sampling steps from already-scaled dimensions to ensure ASCII block rendering stays within bounds.
- [x] Pass block widget by reference to renderer to reduce moves and satisfy borrow checks.
