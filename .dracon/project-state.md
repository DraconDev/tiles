# Project State

## Current Focus
Simplify sidebar header highlighting by removing the unused hover‑target check for the Remotes header.

## Completed
- [x] refactor(sidebar): eliminate redundant `matches!(app.hovered_drop_target, Some(DropTarget::RemotesHeader))` condition, now styling depends solely on the active sidebar index.
