# Project State

## Current Focus
Add visual indicator for open files in the project sidebar and refactor line rendering using spans with styled accent.

## Completed
- [x] Import `HashSet` from `std::collections`
- [x] Collect `open_files` as a `HashSet<PathBuf>` from all tab previews
- [x] Create `open_indicator` span styled with accent primary when a file is open
- [x] Use `indent_str` for consistent indentation instead of repeated `"  "` string
- [x] Rebuild sidebar items with `Line` containing marker, icon, optional open indicator, and name, applying style cambios
