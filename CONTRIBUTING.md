# Contributing to Tiles

## Prerequisites

- Rust stable (1.75+)
- Git
- A terminal with TrueColor support

## Setup

```bash
git clone https://github.com/DraconDev/tiles
cd tiles
cargo build
```

## Project Structure

```
src/
├── main.rs              # Entry point, event loop, tokio runtime, file watchers
├── app.rs               # App state, debug logging, widget definitions
├── event.rs             # Event type conversion helpers
├── event_helpers.rs     # Navigation, clipboard, path resolution, history
├── config.rs            # Settings persistence (TOML)
├── icons.rs             # File type icon mapping
├── state/
│   └── mod.rs           # Data structures: FileState, AppMode, CurrentView, etc.
├── modules/
│   ├── files.rs         # Local filesystem: read_dir, metadata, search, git data
│   ├── remote.rs        # SSH remote: directory listing, git data, file ops
│   └── system.rs        # System stats: CPU, memory, disk, processes
├── events/
│   ├── mod.rs           # Event routing: keyboard → handler dispatch
│   ├── input.rs         # Input helpers (delete_word_backwards)
│   ├── file_manager.rs  # File pane: navigation, selection, drag/drop, mouse
│   ├── editor.rs        # Text editor: key handling, save, undo/redo
│   ├── git.rs           # Git view: commit browsing, diff viewing
│   ├── monitor.rs       # System monitor view key handling
│   └── modals.rs        # Modals: settings, properties, path input, context menu
└── ui/
    ├── mod.rs           # Main draw function, all page renderers
    ├── modals.rs        # Modal rendering (settings, properties, confirmations)
    ├── theme.rs         # Color themes and styling
    └── panes/
        ├── mod.rs       # Pane layout utilities
        ├── files.rs     # File list table rendering
        ├── breadcrumbs.rs # Breadcrumb bar with editable path
        └── sidebar.rs   # Sidebar (favorites, projects, folder tree)
```

## Dependencies

| Crate | Source | Purpose |
|-------|--------|---------|
| `dracon-terminal-engine` | Git (dracon-libs) | Terminal runtime, compositor, input parser, ratatui bridge, widgets |
| `dracon-files` | Git (dracon-libs) | Filesystem operations, metadata, search |
| `dracon-git` | Git (dracon-libs) | Git log, diff, status parsing |
| `dracon-system` | Git (dracon-libs) | System stats, SSH remote operations |

## Running

```bash
# Debug mode
cargo run

# Release mode
cargo run --release

# With debug logging (writes to debug.log)
TILES_DEBUG_LOG=1 cargo run
```

## Testing

```bash
# Run all tests
cargo test

# Run clippy linter
cargo clippy

# Both should pass with 0 warnings before submitting
cargo test && cargo clippy
```

## Key Patterns

### Event Flow
1. Input thread reads stdin → parses via `dracon-terminal-engine::input::parser`
2. Raw events sent through `mpsc` channel to main loop
3. Main loop dispatches to handler functions in `events/`
4. Handlers modify `App` state and may send `AppEvent` back through channel

### Locking
- `App` is wrapped in `Arc<Mutex<App>>`
- Keep lock hold time minimal — clone data out, release lock, process
- The main event loop holds the lock briefly per event
- Spawned tasks (`tokio::spawn`) clone the Arc and acquire lock only to apply results

### File Watching
- Uses `notify-debouncer-mini` with 200ms debounce
- `Recursive` mode — watches directory trees (expanded folders and their contents)
- `sync_watches` has fast bail-out when paths haven't changed

## Code Style

- No comments in code unless specifically asked
- Follow existing patterns in neighboring files
- Use `crate::` imports for cross-module references
- Prefer `PathBuf` over `String` for path handling
- Use `ratatui::layout::Rect` for bounds tracking
