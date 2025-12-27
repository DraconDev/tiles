🏗️ MASTER PLAN: PROJECT TILES
Version: 1.1 (Nautilus-Like Re-Architecture)
Legal: Dracon License v1.0 (Proprietary / Source Available)
Stack: Rust, Ratatui, Tokio, Bollard, Sysinfo.

1. 🌍 The High-Level Vision
Tiles is a "Terminal Workspace Environment" that brings the familiarity of GUI file managers (like GNOME Files/Nautilus) to the terminal, while integrating powerful developer tools. It abandons the traditional complex tiling approach for a clean, tabbed interface where "Files", "Processes", and "Docker" are first-class views, accessible via single keystrokes.
Business Goal: To capture the 5+ employee company market with a fixed-tier license model by offering a tool that replaces lazydocker, ranger, and btop combined, with an interface familiar to Linux desktop users.

2. 🏛️ Technical Architecture (Rust)
A. The Core Event Loop (main.rs)
The application runs on a Dual-Threaded Async Architecture:
Main Thread (UI): Synchronous. Handles drawing via Ratatui and capturing keyboard input via Crossterm.
Background Runtime (Tokio): Asynchronous. Handles heavy lifting (Docker API calls, File I/O, System polling) to ensure the UI never freezes.
Communication: Use tokio::sync::mpsc channels to pass messages from Background -> UI.

B. State Management (app.rs)
Centralized Global State:
```rust
pub struct App {
    pub running: bool,
    pub current_view: CurrentView, // Enum: Files, Docker, System
    pub mode: AppMode,             // Enum: Normal, Input, CommandPalette, Location, Rename, Delete, Properties, NewFolder
    
    // The Data Stores
    pub file_state: FileState,
    pub docker_state: DockerState,
    pub system_state: SystemState,
    
    // UI State
    pub sidebar_focus: bool,
    pub sidebar_index: usize,
    
    // Commercial / Config
    pub config: Config,
    pub license: LicenseStatus, 
}
```

3. 🧩 The Modules (The "Views")
View 1: The File Manager (Files)
Hotkeys: `f` to switch to.
Features:
- Sidebar: Quick access to Home, Downloads, Documents, Pictures.
- Navigation: Vim (j/k) or Arrow keys.
- Nautilus-style Actions: 
    - `Alt+Up`: Parent directory.
    - `Ctrl+L`: Location bar input.
    - `Ctrl+H`: Toggle hidden files.
    - `Ctrl+Shift+N`: New folder.
    - `F2`: Rename.
    - `Delete`: Delete file/folder (with confirmation).
    - `Alt+Enter`: Properties modal.

View 2: The System Monitor (Processes)
Hotkeys: `p` to switch to.
Features:
- Gauges: CPU, Memory, Disk usage.
- Interactive Process List: Scrollable list of top processes.
- Unified Actions:
    - `Delete`: Kill selected process (with confirmation).
    - `Alt+Enter`: Process details.

View 3: The Docker Manager (Docker)
Hotkeys: `d` to switch to.
Features:
- List: Containers with status.
- Unified Actions:
    - `Delete`: Remove container (with confirmation).
    - `Alt+Enter`: Inspect container.
    - `s`: Start.
    - `x`: Stop.

4. 🎨 UX & Interaction Design
The Layout System
Vertical Split: Sidebar (20%) | Main Content (80%).
Bottom Bar: Tabs + Footer.
    - Tabs: `[F]iles [C]onsole [P]rocesses [D]ocker` (Active tab highlighted).
    - Footer: Context-sensitive shortcuts hints.

Global Hotkeys
- `f`: View Files.
- `p`: View Processes.
- `d`: View Docker.
- `c` / `Ctrl+P`: Command Palette (Fuzzy search actions).
- `q`: Quit.

The "Unified Action" Philosophy
The `Delete` key and `Alt+Enter` (Properties) hotkeys adapt to the current view (Files, Processes, or Docker), creating a consistent mental model.

5. 💼 Commercial Logic Implementation
Strategy: "Soft Lock" / Honor System.
The License Check (utils/license.rs)
On Startup: Check for ~/.config/tiles/license.key.
Verification:
If file exists: Verify cryptographic signature.
If valid: Set App.license to Commercial(CompanyName).
If missing/invalid: Set App.license to FreeMode.
The UI Consequence
Footer Rendering:
If FreeMode: Render "Tiles Free Edition (<5 employees)..."
If Commercial: Render "Licensed to Acme Corp".

6. 🚀 Development Roadmap
Phase 1: The Skeleton & Files (Completed)
- [x] Basic Ratatui setup.
- [x] File Manager with Nautilus shortcuts (Ctrl+L, F2, etc.).
- [x] Sidebar navigation.

Phase 2: The Views & Unification (Completed)
- [x] Tabbed Layout (Bottom bar).
- [x] View Switching (f, p, d).
- [x] System Monitor with interactive process list.
- [x] Docker Module with basic start/stop.
- [x] Unified Actions (Delete/Properties work everywhere).

Phase 3: Refinement & Context (Next)
- [ ] Context Trigger: Selecting a project folder in Files filters Docker/System views.
- [ ] Git Integration: Status indicators in File view.
- [ ] Advanced Docker: Inspect modal (currently placeholder), Logs view.
- [ ] Advanced System: Kill process implementation (currently placeholder).
- [ ] File Operations: Copy/Paste support.

7. File Structure
src/
├── main.rs           # Entry point, event loop, input handling
├── app.rs            # State definitions (App, AppMode, States)
├── event.rs          # (Deprecated/Merged into main)
├── config.rs         # Configuration logic
├── license.rs        # License verification
├── ui/
│   ├── mod.rs        # Main drawing logic (Tabs, Sidebar, Modals)
│   ├── layout.rs     # (Deprecated/Merged)
│   └── theme.rs      # Color definitions
└── modules/
    ├── files.rs      # File system operations
    ├── docker.rs     # Docker API operations
    └── system.rs     # System/Process polling
