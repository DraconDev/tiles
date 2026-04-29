# Tiles

![Tiles](assets/tiles-screenshot.png)

A high-performance terminal file manager built in Rust. Modular pane system, integrated text editor, git awareness, remote SSH file browsing, and system monitoring — all in one TUI.

## Features

- **File Manager** — Dual-pane navigation, drag & drop, batch operations, hidden file toggle, column sorting
- **Text Editor** — Syntax highlighting via `syntect`, unlimited undo/redo, multi-selection, live search, right-click context menu
- **Editor Shortcuts** — Alt+↑/↓ move lines, Ctrl+D duplicate, Ctrl+K/U kill lines, Ctrl+A select all, Ctrl+Home/End jump to edges
- **Run Files** — Ctrl+Enter runs scripts (shebang), Cargo projects, and extension-mapped executables
- **Git Integration** — Commit history viewer, staged/unstaged diffs, branch info, ahead/behind tracking
- **Remote SSH** — Browse remote filesystems via SSH, SFTP-style file operations
- **System Monitor** — CPU, memory, disk, network stats, process list
- **Sidebar** — Favorites, project directories, expandable folder tree
- **Path Input** — Click the breadcrumb bar to edit the path directly, copy on click
- **Keyboard-first** — Vim-style navigation, command palette (`:`), context menus

## Keyboard Shortcuts

### Navigation
| Key | Action |
|-----|--------|
| `h/j/k/l` or arrows | Navigate |
| `Enter` | Open file / enter directory |
| `Backspace` | Go to parent directory |
| `Tab` | Switch panes |
| `:` | Command palette |

### Editor
| Key | Action |
|-----|--------|
| `Ctrl+E` | Editor view |
| `Ctrl+Enter` | Run current file |
| `Alt+↑` / `Alt+↓` | Move current line up/down |
| `Ctrl+D` | Duplicate current line |
| `Ctrl+K` | Kill to end of line |
| `Ctrl+U` | Kill to start of line |
| `Ctrl+A` | Select all |
| `Ctrl+Home` | Jump to document start |
| `Ctrl+End` | Jump to document end |
| `Right-click` | Open context menu |

### Other Views
| Key | Action |
|-----|--------|
| `Ctrl+G` | Git history view |
| `Ctrl+D` | System monitor view |
| `Ctrl+L` | Edit current path |
| `q` | Quit |

## Install

```bash
git clone https://github.com/DraconDev/tiles
cd tiles
./install.sh
```

Or build manually:

```bash
cargo build --release
./target/release/tiles
```

## Optional Dependencies

For drag & drop support (dragging files from Tiles to other apps):
- [dragon](https://github.com/mwh/dragon)
- [ripdrag](https://github.com/nik012003/ripdrag)

Tiles auto-detects these tools and adds a "Drag" option to the context menu.

## Download Pre-compiled Binaries

Download the latest binaries for Linux, macOS, and Windows from [GitHub Releases](https://github.com/DraconDev/tiles/releases).

## License

Dracon License v1.1 — see [LICENSE](LICENSE).
