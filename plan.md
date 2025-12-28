🏗️ MASTER PLAN: PROJECT TILES
Version: 2.0 (The Universal OS Command Center - Max Ambition)
Legal: Dracon License v1.0 (Proprietary / Source Available)
Stack: Rust, Ratatui, Tokio, Bollard, Sysinfo, Chrono.

1. 🌍 The High-Level Vision
Tiles is the "VLC of Terminal Tools"—a single, agentless binary that unifies File Management, System Operations, and Container Orchestration into a high-density, mouse-driven cockpit. It replaces the fragmented CLI experience with a professional, interactive environment that works locally and over SSH with zero installation on the target.

Business Goal: Standardize team workflows and provide secure, agentless remote management for enterprises (5+ employees).

2. 🏛️ Technical Architecture (Rust)
A. The Core Event Loop (main.rs)
- Dual-Threaded Async: Synchronous Ratatui UI thread + Asynchronous Tokio background runtime.
- Input Philosophy: "Just Type" search (Space is a search character). Modifier-based shortcuts (Ctrl/Alt) for global actions.
- Spatial Navigation: Left/Right arrows jump between "Logical Zones" (Sidebar ↔ Main Pane ↔ Split Pane).
- Mouse-First: SGR 1006 protocol support for Double-click activation, Right-click context menus, and Drag-and-Drop.

B. State Management (app.rs)
- Relational Engine: Links between Files, Processes, and Containers (e.g., jump from Process -> Docker Container -> Source File).
- Persona Toggles: "Focus Mode" (Files only) vs "Ops Mode" (Full Dashboard) toggleable via config or UI.

3. 🧩 The Three Pillars (The Trinity)
Pillar I: The Virtual File Workspace (Files)
- 1:1 Nautilus Replacement: High-density tables, icons, and safe deletions (Global Trash Bin).
- Agentless SFTP: Remote servers appear as sidebar bookmarks; drag-and-drop file transfers via SSH.
- Smart Create: `n` shortcut creates full paths and pre-fills templates (e.g., `#!/bin/bash` for `.sh`).

Pillar II: The System Cockpit (Processes)
- Actionable Observer: Stable process tree. Click/Right-click to Kill, Inspect Ports, or see Open Files.
- 15-Minute Buffer: Time-traveling resource graphs to scrub back and see historical spikes.
- Network Mapping: App-to-IP mapping with human-readable geolocation instead of raw scrolling text.

Pillar III: The Container Orchestrator (Docker)
- Visual Topology: Dependency maps based on Compose projects and Traefik/Nginx routing labels.
- Magic Tunnels: One-click port forwarding from remote containers to `localhost`.
- Log Streamer: JSON-aware, searchable log tailing with auto-formatting.

4. 🛡️ Safety & Operations
- Production "Red Zone": Pulsing red UI for production connections; destructive actions require typed confirmation.
- Safe Edit: Press `e` on remote/container files to edit locally in VS Code/Vim with auto-sync back to target.
- Archive VFS: Browse `.zip` and `.tar.gz` as virtual folders without manual extraction.

5. 💼 Commercial Logic (Dracon License v1.0)
- Model: Free for Individuals; Paid for Teams (5+ employees).
- Trigger: Companies pay for standardized team configs and agentless security compliance.
- Enforcement: "Hero Badge" UI indicator; cryptographic key verification via Ed25519.

6. 🚀 Development Roadmap (Updated)
Phase 1: Nautilus Foundations (Completed)
- [x] Ratatui loop with Tab/Sidebar layout.
- [x] Standard file management (Sort, Icons, Clipboard).
- [x] Pure Arrow & Mouse navigation.

Phase 2: The Agentless Leap (Next)
- [ ] SSH Connection Manager: Sidebar bookmarks for remote hosts.
- [ ] Agentless SFTP: Local/Remote split view with drag-drop.
- [ ] Docker SSH Tunneling: Managing remote containers via standard I/O streams.

Phase 3: The Relational Engine
- [ ] Atomic Combos: Linking Process -> Container -> File.
- [ ] The 15-Minute History Buffer for system metrics.
- [ ] Network Tab: App-to-IP mapping.

Phase 4: Safety & Polish
- [ ] Production "Red Zone" Mode.
- [ ] Global Trash Bin with Undo.
- [ ] "Just Type" Search integration across all modules.

7. File Structure
src/
├── main.rs           # Dual-threaded loop, Spatial Input, SGR Mouse
├── app.rs            # Relational State, Personas, License Status
├── ui/
│   ├── mod.rs        # Layout, Modals, Breadcrumbs
│   └── theme.rs      # High-density, high-contrast engineering styles
└── modules/
    ├── files.rs      # Local/Remote VFS, SFTP
    ├── docker.rs     # Bollard + SSH tunneling
    └── system.rs     # Sysinfo + SSH text parsing


the menu should differentiate what we clicking on so for ex files empty space we might see new folder and new file, while clicking on a folder has rename and delete options