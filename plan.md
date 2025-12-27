🏗️ MASTER PLAN: PROJECT TILES
Version: 1.2 (Full Commitment: Terminal-Nautilus Hybrid)
Legal: Dracon License v1.0 (Proprietary / Source Available)
Stack: Rust, Ratatui, Tokio, Bollard, Sysinfo.

1. 🌍 The High-Level Vision
Tiles is a "Terminal Workspace Environment" that is a 1-1 functional terminal version of Ubuntu's "Files" (Nautilus), extended with integrated Dev-Ops tools (Docker, System Monitoring). The UI is streamlined for speed, using native-feeling Linux shortcuts and a clean tabbed layout at the bottom.
Business Goal: To replace standalone terminal tools with a unified environment that feels like a desktop file manager but has the power of a CLI.

2. 🏛️ Technical Architecture (Rust)
A. The Core Event Loop (main.rs)
Dual-Threaded Async:
- Main Thread (UI): Ratatui rendering + Crossterm input.
- Background Runtime (Tokio): Async polling for Docker, System info, and File I/O.
- Hotkey Priority: Modifier-based shortcuts (Ctrl+X) are evaluated before single-character inputs to allow for "Type-to-Search" without navigation conflicts.

B. State Management (app.rs)
```rust
pub struct App {
    pub running: bool,
    pub current_view: CurrentView, 
    pub mode: AppMode,             
    pub file_state: FileState,     // Includes search_filter, git_status, clipboard
    pub docker_state: DockerState,
    pub system_state: SystemState, 
    pub sidebar_focus: bool,
}
```

3. 🧩 The Modules (The "Views")
View 1: The File Manager (Files)
Hotkeys: `Ctrl + F` to view.
Features:
- Type-to-Search: Typing any character instantly filters the current directory.
- Nautilus Actions: `Ctrl+L` (Location), `Ctrl+H` (Hidden), `F2` (Rename), `Del` (Delete), `Alt+Enter` (Properties), `Ctrl+Shift+N` (New Folder), `Alt+Up` (Parent).
- Git Integration: Real-time status indicators ([M], [A], [??]) next to files.
- Clipboard: `Ctrl+C` (Copy), `Ctrl+X` (Cut), `Ctrl+V` (Paste) with recursive directory support.

View 2: The System Monitor (Processes)
Hotkeys: `Ctrl + P` to view.
Features:
- Live Gauges: CPU, RAM, and Disk metrics.
- Interactive Process List: Navigate and select processes.
- Unified Actions: `Delete` to prompt for process kill, `Alt+Enter` for detailed PID info.

View 3: The Docker Manager (Docker)
Hotkeys: `Ctrl + D` to view.
Features:
- Container List: Real-time status from Docker socket.
- Interactive Controls: `s` (Start), `x` (Stop).
- Unified Actions: `Delete` to remove container, `Alt+Enter` for container metadata.

View 4: The Command Center (Console)
Hotkeys: `Ctrl + .` to open.
Features:
- Fuzzy Palette: Quick search across all app actions and container names.

4. 🎨 UX & Interaction Design
The Layout System
- vertical: Sidebar (Places) | Main Stage (Content).
- bottom: Tab Bar (`[^F]iles [^P]rocesses [^D]ocker`) + Footer (Hotkey hints).

Navigation Philosophy
- `Esc`: Always returns to Normal mode or clears Search filters.
- `Backspace`: Smart logic—deletes search query characters if searching, otherwise navigates to the parent directory.
- `Left Arrow`: Shifts focus to Sidebar (Places).

5. 💼 Commercial Logic Implementation
- "Soft Lock" License: Cryptographic key verification at `~/.config/tiles/license.key`.
- UI Branding: Free users see "Tiles Free Edition" footer; Commercial users see "Licensed to [Company]".

6. 🚀 Development Roadmap
Phase 1: Nautilus Foundations (Completed)
- [x] Basic Ratatui setup.
- [x] File Manager with full Nautilus hotkey suite.
- [x] Type-to-Search functionality.
- [x] Git Status integration.

Phase 2: View Unification (Completed)
- [x] Bottom Tab navigation.
- [x] View switching via `Ctrl + F/P/D`.
- [x] Unified "Delete" and "Properties" modals across all views.
- [x] Functional "Places" Sidebar.

Phase 3: Refinement (In Progress)
- [ ] Contextual Auto-Filtering: Hovering over a project folder in Files auto-filters the Docker view.
- [ ] Advanced Clipboard: Full integration of Copy/Paste logic in the main loop.
- [ ] Process Management: Implement actual `kill` logic for the System view.
- [ ] Docker Logs: Dedicated view for container output.

7. File Structure
src/
├── main.rs           # Core loop & Input (Highest priority)
├── app.rs            # State definitions
├── ui/
│   └── mod.rs        # Rendering (Nautilus-hybrid style)
└── modules/
    ├── files.rs      # FS ops + Git + Search filtering
    ├── docker.rs     # Bollard integration
    └── system.rs     # Sysinfo polling

we want icons too, bake it into the ui if that is best

also show more information about the files in the file manager, and allow us to customise it 