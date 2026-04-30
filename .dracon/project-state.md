# Project State

## Current Focus
Add task‑progress notifications for local file copy operations, including start, incremental, and completion events.

## Completed
- [x] feat(copy): implement progress‑aware copy for local files, reporting start (`AppEvent::TaskProgress` with 0.0), incremental percentages, and finish (`AppEvent::TaskFinished`) events.
- [x] feat(copy): count total files in a source directory (`count_files`) to calculate progress.
- [x] feat(copy): create `copy_recursive_with_progress` helper that tracks copied count, invokes a callback for progress updates, and copies files/directories recursively.
- [x] fix(copy): replace single copy call with progress‑aware logic, preserving existing remote copy path.
- [x] chore(cargo): update `Cargo.lock` to match new dependencies used for UUID generation and any other added crates.
