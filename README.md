# Tiles

A high-performance terminal file manager built in Rust. Modular pane system, integrated text editor, git awareness, remote SSH file browsing, and system monitoring — all in one TUI.

## Features

- **File Manager** — Dual-pane navigation, drag & drop, batch operations, hidden file toggle, column sorting
- **Text Editor** — Syntax highlighting via `syntect`, unlimited undo/redo, multi-selection, live search
- **Git Integration** — Commit history viewer, staged/unstaged diffs, branch info, ahead/behind tracking
- **Remote SSH** — Browse remote filesystems via SSH, SFTP-style file operations
- **System Monitor** — CPU, memory, disk, network stats, process list
- **Sidebar** — Favorites, project directories, expandable folder tree
- **Path Input** — Click the breadcrumb bar to edit the path directly, copy on click
- **Keyboard-first** — Vim-style navigation, command palette (`:`), context menus

## Architecture

```
tiles
├── dracon-terminal-engine  (git)  — Terminal runtime, compositor, input parser, ratatui bridge
├── dracon-files            (path) — Filesystem operations, metadata, search
├── dracon-git              (path) — Git log, diff, status, branch parsing
└── dracon-system           (path) — System stats, SSH remote operations
```

## Prerequisites

- Rust 1.75+ (`rustup install stable`)
- Git
- For remote SSH: an SSH key configured

## Build from Source

```bash
git clone https://github.com/DraconDev/tiles
cd tiles
cargo build --release
./target/release/tiles
```

## Install Locally

```bash
./install.sh
```

This builds a release binary and copies it to `~/.local/bin/tiles`.

## Development

```bash
# Run in debug mode
cargo run

# Run with debug logging
TILES_DEBUG_LOG=1 cargo run

# Run tests
cargo test

# Lint
cargo clippy
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `h/j/k/l` or arrows | Navigate |
| `Enter` | Open file / enter directory |
| `Backspace` | Go to parent directory |
| `:` | Command palette |
| `Ctrl+E` | Editor view |
| `Ctrl+G` | Git history view |
| `Ctrl+D` | System monitor view |
| `Ctrl+L` | Edit current path |
| `Tab` | Switch panes |
| `q` | Quit |

## Optional Dependencies

For drag & drop support (dragging files from Tiles to other apps):
- [dragon](https://github.com/mwh/dragon)
- [ripdrag](https://github.com/nik012003/ripdrag)

Tiles auto-detects these tools and adds a "Drag" option to the context menu.

## Download Pre-compiled Binaries

Download the latest binaries for Linux, macOS, and Windows from [GitHub Releases](https://github.com/DraconDev/tiles/releases).

## License

Dracon License v1.1 — see [LICENSE](LICENSE).
