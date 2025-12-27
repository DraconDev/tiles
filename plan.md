🏗️ MASTER PLAN: PROJECT TILES
Version: 1.3 (Nautilus-Perfect: Pure Arrow & Mouse Architecture)
Legal: Dracon License v1.0 (Proprietary / Source Available)
Stack: Rust, Ratatui, Tokio, Bollard, Sysinfo, Chrono.

1. 🌍 The High-Level Vision
Tiles is a "Terminal Workspace Environment" providing a 1-1 terminal recreation of Ubuntu's "Files" (Nautilus). It integrates Docker and System monitoring as native views. It eschews Vim-style complexity for a user-friendly, mouse-aware interface driven by standard Linux shortcuts and arrow-key navigation.
Business Goal: To offer a professional, unified dev-environment that eliminates context switching between terminal and GUI tools.

2. 🏛️ Technical Architecture (Rust)
A. The Core Event Loop (main.rs)
- Dual-Threaded Async: Ratatui UI + Tokio background runtime.
- Input Priority: Modifier-based shortcuts (Ctrl+X) are processed before single-character inputs to support "Type-to-Search" without conflict.
- No Vim Keys: Standalone navigation is strictly handled by Arrow Keys to keep the character space open for instant searching.
- Mouse Integration: Direct hit-testing for Tabs, Sidebar, and Lists.

B. State Management (app.rs)
- Tabbed Files: `file_tabs: Vec<FileState>` allows multiple concurrent directory views.
- Unified Modes: `AppMode` handles modal states like Rename, New Folder, and Column Setup.

3. 🧩 The Modules (The "Views")
View 1: The File Manager (Files) [^F]
- Layout: Top-aligned tabs, Left sidebar (Places), Main table content.
- Table Columns: Customizable visibility (Name, Size, Modified, Created, Permissions, Ext).
- Features: 
    - Type-to-Search: Filters list instantly as you type.
    - Icons: `📁` / `📄` markers.
    - Stars: `Ctrl + B` to bookmark/star items.
    - Git: Porcelain status indicators ([M], [A], [??]).
    - Clipboard: `Ctrl + C/X/V` (Copy/Cut/Paste) with recursive support.

View 2: The System Monitor (Processes) [^P]
- Visuals: Live Gauges for CPU/RAM/Disk.
- Interaction: Scrollable/Selectable Top 10 Processes list.
- Action: `Delete` key prompts to kill selected process.

View 3: The Docker Manager (Docker) [^D]
- Visuals: Detailed container table (ID, Image, State, Status).
- Controls: `s` to Start, `x` to Stop.
- Action: `Delete` key prompts to remove container.

4. 🎨 UX & Interaction Design
The Layout System
- Header: Tab Bar showing main views and open file tabs.
- Main: Sidebar (20%) | Content (80%).
- Footer: Console Shortcut (`^.`) and standard action hints + Live Storage metrics.

Shortcut Suite (Pure Nautilus)
- `Ctrl + T`: New File Tab.
- `Ctrl + W`: Close current Tab/App.
- `Ctrl + Tab`: Next Tab.
- `Ctrl + L`: Location/Path entry.
- `Ctrl + H`: Toggle Hidden Files.
- `Ctrl + Shift + N`: New Folder.
- `Alt + Enter`: Properties Modal.
- `Alt + C`: Column Setup Modal.
- `Alt + Up`: Navigate to Parent.
- `Delete`: Delete/Kill/Remove action.
- `Esc`: Clear search / Exit modal.

5. 💼 Commercial Logic Implementation
- License Key: `~/.config/tiles/license.key` verification.
- UI: Unobtrusive footer branding based on status.

6. 🚀 Development Roadmap
Phase 1: Nautilus Foundations (Completed)
- [x] Ratatui setup with Top-Tab layout.
- [x] Full Nautilus hotkey suite.
- [x] Type-to-Search & Icons.
- [x] Multi-tab file management.

Phase 2: View Unification & Mouse (Completed)
- [x] Interactive Mouse support (Click tabs/sidebar/lists + Scroll).
- [x] Unified "Delete" and "Properties" modals.
- [x] Customizable Column setup.
- [x] Git & Star indicators.

Phase 3: Remote & Advanced Ops (Next)
- [ ] SSH Integration: Mount remote systems as virtual folders in the Files view.
- [ ] Drag & Drop: Exploring terminal mouse-drag events for file moving.
- [ ] Split View: Toggle vertical split for dual-pane file management.
- [ ] Docker Logs: Dedicated log stream view.

7. File Structure
src/
├── main.rs           # Event loop, Input priority, Mouse hit-testing
├── app.rs            # State (Tabs, Modes, Columns)
├── ui/
│   └── mod.rs        # Tabular rendering & Modal logic
└── modules/
    ├── files.rs      # FS ops, Git, Recursive copy
    ├── docker.rs     # Bollard connectivity
    └── system.rs     # Sysinfo diagnostics


i wonder if we can style a bit and make the lef