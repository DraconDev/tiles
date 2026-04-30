# Project State
##Current Focus
Add support for trashing local files via `AppEvent::TrashFile`, handling remote files by falling back to permanent delete and providing status messages.

## Completed
- [x] Added `AppEvent::TrashFile` handling that checks for a remote session, deletes local files via `trash::delete`, and sends appropriate status messages.
- [x] Implemented status notifications for successful trash, failed trash, and remote file restrictions, and refreshed files after the operation.
