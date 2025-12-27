🏗️ MASTER PLAN: PROJECT TILES
Version: 1.0 (Architecture Draft)
Legal: Dracon License v1.0 (Proprietary / Source Available)
Stack: Rust, Ratatui, Tokio, Bollard, Sysinfo.
1. 🌍 The High-Level Vision
Tiles is a "Terminal Workspace Environment." It is not just a monitoring tool; it is an interactive operating system for the terminal. It solves the context-switching problem by unifying Files, Containers, and System Resources into a single, tiling pane interface that supports cross-module context (e.g., selecting a directory filters the Docker view).
Business Goal: To capture the 5+ employee company market with a fixed-tier license model by offering a tool that replaces lazydocker, ranger, and btop combined.
2. 🏛️ Technical Architecture (Rust)
A. The Core Event Loop (main.rs)
The application must run on a Dual-Threaded Async Architecture:
Main Thread (UI): Synchronous. Handles drawing to the terminal via Ratatui and capturing user keyboard input via Crossterm.
Background Runtime (Tokio): Asynchronous. Handles heavy lifting (Docker API calls, File I/O, System polling) to ensure the UI never freezes.
Communication: Use tokio::sync::mpsc channels to pass messages from Background -> UI (e.g., DockerContainerUpdated(Vec<ContainerSummary>)).
B. State Management (app.rs)
The Global State must be centralized. Do not scatter state across widgets.
code
Rust
￼
download
￼
content_copy
￼
expand_less
pub struct App {
    pub running: bool,
    pub active_tile: TileType, // Enum: Files, Docker, System, Logs
    pub mode: AppMode,         // Enum: Normal, Input, Zoomed
    
    // The Data Stores
    pub file_state: FileState,
    pub docker_state: DockerState,
    pub system_state: SystemState,
    
    // Commercial / Config
    pub config: Config, // Loaded from tiles.toml
    pub license: LicenseStatus, // Enum: Free, Commercial(Key)
}

pub enum LicenseStatus {
    FreeMode,           // Show "Support Tiles" footer
    Commercial(String), // Hide footer, show Company Name
}
3. 🧩 The Modules (The "Tiles")
Tile 1: The File Manager (modules/files.rs)
Library: std::fs, walkdir.
Visuals: Tree view or List view.
Features:
Vim navigation (j/k).
Git Integration: Show [+], [-], [M] next to files using libgit2 or CLI parsing.
Context Trigger: When a user hovers a folder containing a Dockerfile or docker-compose.yml, emit a ContextEvent::ProjectSelected(path) signal.
Tile 2: The Docker Manager (modules/docker.rs)
Library: bollard (Async).
Visuals: Table view (Containers) + Sparklines (Stats).
Features:
Listing: ID, Image, Status, Ports, CPU%.
Actions: s (start), x (stop), r (restart), l (logs), e (exec shell).
Reactive Filtering: If ContextEvent is received, filter the list to only show containers related to the current file path (project).
Tile 3: The System Monitor (modules/system.rs)
Library: sysinfo.
Visuals: Gauges (Ratatui) and Sparklines for history.
Features:
Global CPU/RAM usage.
Process List: List top 10 processes.
Port Watcher: List active listening ports (TCP).
Tile 4: The Command Center (The "Glue")
Visuals: A popup modal (like VS Code Command Palette).
Trigger: Ctrl+P or :.
Function: Fuzzy search across Files, Container Names, and App Commands (e.g., "Kill Container", "Git Commit").
4. 🎨 UX & Interaction Design
The Layout System
Default View: 3-Pane Split.
Left (50%): File Explorer.
Top Right (25%): System Resources.
Bottom Right (25%): Docker Containers.
The "Zoom" Mechanic:
Pressing Enter on a focused tile expands it to 100% width/height.
Pressing Esc returns to the split view.
Keybindings (Vim Standard)
Tab: Cycle focus between tiles.
h/j/k/l: Navigation within a list.
?: Toggle Help Modal.
Ctrl+c: Quit.
5. 💼 Commercial Logic Implementation
Strategy: "Soft Lock" / Honor System.
The License Check (utils/license.rs)
On Startup: Check for ~/.config/tiles/license.key.
Verification:
If file exists: Verify cryptographic signature (using ed25519 public key embedded in binary).
If valid: Set App.license to Commercial(CompanyName).
If missing/invalid: Set App.license to FreeMode.
The UI Consequence
Footer Rendering:
If FreeMode: Render a text span at the bottom right: Style::default().fg(DarkGray).content("Tiles Free Edition (<5 employees). Support us at dracon.uk").
If Commercial: Render Style::default().fg(Gold).content("Licensed to Acme Corp").
Features: All features are accessible in Free Mode (we rely on corporate compliance for revenue, not feature gating).
6. 🚀 Development Roadmap
Phase 1: The Skeleton (MVP)
Set up Rust project with ratatui + crossterm.
Build the App struct and the Layout logic.
Implement basic keyboard handling (Quit, Switch focus).
Phase 2: The Data
Implement System tile (easiest data source).
Implement Files tile (directory walking).
Implement Docker tile (connecting to socket).
Phase 3: The Interactivity
Add the "Zoom" function.
Add Docker controls (Start/Stop).
Implement the license.rs checker.
Phase 4: The "Glue" (Context)
Make the File selection filter the Docker view.
Add the Command Palette (Ctrl+P).
7. File Structure Proposal
code
Code
￼
download
￼
content_copy
￼
expand_less
src/
├── main.rs           # Entry point, event loop
├── app.rs            # State holding
├── event.rs          # Keyboard/Tick event handling
├── config.rs         # TOML parsing
├── license.rs        # Key validation logic
├── ui/
│   ├── mod.rs        # Main UI rendering
│   ├── layout.rs     # Tiling logic
│   └── theme.rs      # Colors (Dracon branding)
└── modules/
    ├── files.rs      # File Manager logic
    ├── docker.rs     # Bollard integration
    └── system.rs     # Sysinfo logic
🤖 Prompt for the AI:
"Take this Master Plan. I want to start with Phase 1. Generate the cargo.toml dependencies and the main.rs and app.rs boilerplate that sets up the Ratatui loop, the 3-pane layout, and the App struct with placeholders for the three modules."