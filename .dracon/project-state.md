# Project State

## Current Focus
Add a dedicated footer area to the editor UI that displays cursor info, language, modified status, and action shortcuts.

## Completed
- [x] Introduced `footer_height` and split the main area into `editor_area` and `footer_area`.
- [x] Rendered a custom footer bar with cursor line/column, language name, ● modified indicator, and ^S Save / ^↵ Run shortcuts.
- [x] Adjusted editor widget rendering to use `editor_area` instead of the full inner area.
- [x] Re‑named search‑footer variables (`search_footer_height`, `search_footer_area`) for clarity.
- [x] Updated footer drawing logic for both normal editor and search/replace modes.
