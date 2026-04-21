# Project State

## Current Focus
Refactored debug logging in commit view rendering to use `eprintln!` for direct output and increased content preview length

## Completed
- [x] Replaced `crate::app::log_debug` with direct `eprintln!` calls for commit view debug output
- [x] Increased content preview length from 200 to 500 characters
- [x] Simplified debug message formatting for commit metadata parsing
