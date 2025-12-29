🏗️ MASTER PLAN: PROJECT TILES
Version: 2.1 (The Universal OS Command Center)
Legal: Dracon License v1.0 (Source Available; Free for individuals/<5 employees; Paid for 5+ employees).
Stack: Rust, Ratatui, Tokio, Bollard, Sysinfo, Chrono.

1. 🌍 The High-Level Vision
Tiles is a "Terminal Workspace Environment." It solves the context-switching problem by unifying **Files**, **Containers**, and **System Resources** into a single, tiling pane interface.
**Core Philosophy:** "The Glue." Selecting a file in the File Tile provides context in the Docker or System tiles (e.g., highlighting the container associated with a specific project folder).

Business Goal: Capture the 5+ employee company market with a "Fixed Tier" license model (predictable costs, zero admin).

2. 🏛️ Technical Architecture (Rust)
A. The Core Event Loop (`main.rs`)
- Dual-Threaded Async: UI (Synchronous Ratatui) + Background (Asynchronous Tokio).
- Input Philosophy: Mouse-first (SGR 1006) + Vim-style keys (`h/j/k/l`).
- Spatial Navigation: Focus cycles between tiles; `Enter` zooms a tile to full screen.

B. State Management (`app.rs`)
- Centralized `App` struct holding state for all tiles (`FileState`, `DockerState`, `SystemState`).
- **Context Engine:** A mechanism to broadcast events (e.g., `ProjectSelected(Path)`) to other modules.

3. 🧩 The Three Pillars (The Trinity)
Pillar I: The Virtual File Workspace (Files)
- **Visuals:** Tree/List view with high-density tables.
- **Git Integration:** Color-coded status (`[+]`, `[-]`, `[M]`) and branch display.
- **Smart Create:** `n` shortcut for templated creation.
- **Context Trigger:** Hovering a folder with `Dockerfile` emits a context signal.

Pillar II: The System Cockpit (Processes)
- **Visuals:** Gauges for CPU/RAM, Sparklines for history.
- **Process List:** Interactive tree. Right-click to Kill.
- **Port Watcher:** List active listening ports and link them to Docker containers.

Pillar III: The Container Orchestrator (Docker)
- **Library:** `bollard` (Async).
- **Features:** Full lifecycle (Start/Stop/Logs/Exec).
- **Reactive Filtering:** Auto-filter container list based on selected File path.
- **Log Streamer:** Aggregated log view for containers and files.

Pillar IV: The Command Center (The "Glue")
- **Trigger:** `Ctrl+P` or `:`.
- **Function:** Fuzzy search across Files, Container Names, and App Commands (e.g., "Kill Container", "Git Commit").

4. 🛡️ Safety & Operations
- Production "Red Zone": Visual warnings for production contexts.
- Safe Edit: `e` to edit remote files locally.
- Archive VFS: Browse archives transparently.

5. 💼 Commercial Logic (Dracon License v1.0)
- **Model:** Fixed Tier Pricing (No per-seat tracking).
    - Personal (<5 employees): Free.
    - Small Team (5-20): ~$290/year flat.
    - Corporate (20+): Tiered flat fees.
- **Enforcement:** "Soft Lock" / Honor System.
    - Free Mode: Footer shows "Free Edition (<5 employees). Support us at dracon.uk".
    - Commercial Mode: Footer shows "Licensed to [Company Name]" (via `~/.config/tiles/license.key`).

6. 🚀 Development Roadmap (Updated)
Phase 1: Foundations (Completed)
- [x] Ratatui loop with Tab/Sidebar layout.
- [x] Standard file management (Sort, Icons, Clipboard).
- [x] Mouse & Scroll logic (Fixed).
- [x] Context Menus.

Phase 2: The Agentless Leap & Data (Current)
- [ ] SSH Connection Manager: Sidebar bookmarks for remote hosts.
- [ ] Docker Module: Connect `bollard` to real Docker socket.
- [ ] System Module: Connect `sysinfo` to real metrics.

Phase 3: The Interactivity
- [ ] Zoom Mechanic (`Enter` to expand tile).
- [ ] Docker Controls (Start/Stop via keybindings).
- [ ] License Check (`utils/license.rs`).

Phase 4: The "Glue" (Context)
- [ ] Context Engine: File selection filters Docker list.
- [ ] Command Palette (`Ctrl+P`).
- [ ] Git Integration (File tile).

7. File Structure
src/
├── main.rs           # Event loop, Input handling
├── app.rs            # State, Context Engine, License
├── ui/
│   ├── mod.rs        # Layout, Draw logic
│   └── theme.rs      # Styling
├── modules/
│   ├── files.rs      # Local/Remote VFS, Git
│   ├── docker.rs     # Bollard integration
│   └── system.rs     # Sysinfo integration
└── utils/
    └── license.rs    # Key verification


the menu should differentiate what we clicking on so for ex files empty space we might see new folder and new file, while for ex clicking on a folder has rename and delete options

tiles should also have well formatted cli commands, partly for quick commands for human but mainly for the ai to understand, so they don't need to call 5 different commands but 1, and we cna be sneaky cause they might not need files on the remote

when we click on col headers we should order the files by that columnin asc or desc order, this is a toggle

incorrect file sizes are displayed

be able to click on the tabs we open with ctrl t and mmb

add icons to sidebar places such as home, downloads, documents, etc, persis change we make here

preferably we want to star by clicking not by pressing ctrl b and it also need a better icon

remote header on the sidebar needs a better icon

apparently we have drag dragging files around would be nice

can we use better icons for files?

on default files view we can take up all the space of the container, we can even customize the columns, on split mode, we 