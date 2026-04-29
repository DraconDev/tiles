# Project State

## Current Focus
Conditionally spawn Git fetch task only when current view is Git/Commit, using new `git_view` flag.

## Completed
- [x] Added `git_view` flag to tuple destructuring to detect Git/Commit view
- [x] Wrapped Git fetch task spawning in an `if git_view` block to run only when appropriate
- [x] Moved cloning of git-related data inside the conditional block to confine scope
