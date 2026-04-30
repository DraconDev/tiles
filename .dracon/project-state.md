# Project State

## Current Focus
Add trash file support, unify delete mode handling, and update UI and event flow accordingly.

## Completed
- [x] feat(context_menu): add Rename and Delete actions with a separator; update context menu handling to send `TrashFile` for file targets and `Delete` for others.
- [x] feat(events): upgrade Delete mode to accept a string (`trash` or `permanent`) and adjust key handlers (`handle_trash_key`, `handle_permanent_delete_key`) to use the new enum variant.
- [x] feat(modals): update modal handling to match the new Delete mode signature; adjust input modal logic to trigger `TrashFile` event when mode is `"trash"`.
- [x] feat(ui): modify delete modal rendering to display different titles, messages, and border colors based on whether the action is a move to trash or permanent deletion.
- [x] fix(main): remove unused `view_mode_before` optimization that conflicted with the new modal logic.
- [x] refactor(event_helpers): replace legacy Delete file action with trash handling for file targets, preserving existing Delete action for folders or non-file targets.
- [x] docs: implicit updates to reflect new trash event handling and modal changes.
