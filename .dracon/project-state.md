# Project State

## Current Focus
refactor(remove unused): Remove unused `border_style` mutability and theme helper methods (`block_active`, `block_inactive`, `text_highlight`) along with the `Modifier` import

## Completed
- [x] Remove `mut` from `border_style` variable assignment in file view rendering
- [x] Delete unused `block_active()` and `block_inactive()` methods from `DraconTheme`
- [x] Delete unused `text_highlight()` method from `DraconTheme` that used `Modifier::BOLD`
- [x] Remove `Modifier` import from theme.rs since it's no longer needed
- [x] Add `#[allow(dead_code)]` to `DraconTheme` struct to suppress warnings about unused fields
