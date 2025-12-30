# 👹 THE DRACON SOVEREIGN STACK: MASTER PLAN
**Version:** 3.0 (The Application Engine Era)
**Philosophy:** "Own the Pixels, Own the Logic."

## 1. The Core Vision
We are building the **Sovereign Developer Environment**. 
We are moving beyond the "Terminal Emulator" paradigm. We don't build apps that live *inside* terminals; we build **Sovereign Applications** that carry their own high-performance display engine (**Terma**).

**The Stack:**
1.  **Terma:** The Engine (The "Unity" of TUIs). A grid-based, hardware-accelerated rendering and input framework.
2.  **Tiles:** The Data Command Center. A modular, windowed orchestrator for files, containers, and systems.
3.  **Demon:** The Logic Engine. An AI-driven agent orchestrator that integrates directly into the Terma canvas.
4.  **Source:** The Web Filter. A protocol-level scraper that feeds clean data into the Command Center.

---

## 2. 🏛️ Architecture: The Engine Model

### Terma (The Foundation)
- **Nature:** Not an emulator. An **Internal Library** for TUI-style GPU applications.
- **Rendering:** Direct pixel blitting to a framebuffer (Window Mode) with ANSI fallback (TTY Mode).
- **Input:** Raw OS event mapping. 100% reliable 5-button mouse and complex key chords.

### Tiles (The First Sovereign App)
- **Role:** Modular Dashboard.
- **Workflow:** High-density orchestration using the "System Lego" philosophy.
- **Key Feature:** **Smart Client Remotes**. Local 60FPS UI connected to remote data agents via SSH.

## 2.1 🧠 The God Mode Architecture
*Why we build a Window, not just a CLI tool.*

We distinguish between **Tenant Apps** (run inside a terminal, like `vim` or `fish`) and **Landlord Apps** (own the window, like Tiles).
*   **The Problem:** Running 50 separate `xterm` windows for 50 agents results in 50 disconnected processes. No shared state, no coordination.
*   **The Solution:** Tiles runs as a single **Sovereign Window**. It acts as the "God" of 50 virtual terminals (Planes).
    *   **Shared Brain:** All agents live in one process memory space.
    *   **Orchestration:** You can visualize, group, and manage 50 streams of data in a unified grid.
    *   **Polymorphism:** On the desktop, it's a high-performance GUI. Over SSH, it degrades gracefully to a standard TUI, preserving the "Command Center" logic while sacrificing only the pixel-perfect rendering.

---

## 3. 🚀 Grand Unified Roadmap

### Phase 1: The Engine Rebirth (COMPLETED)
- [x] **Terma Core:** Compositor, Planes, and Grid Logic.
- [x] **Terma Window:** Winit + Softbuffer + RustType Font Rendering.
- [x] **The Great Migration:** Tiles moved from TTY-exclusive to Sovereign Window architecture.

### Phase 2: Visual Intelligence (CURRENT)
- **High-Res Assets:** Implement `ImageTiles` for direct pixel-perfect thumbnails.
- **UI Chrome:** Implement "Sprites" for buttons, dividers, and custom borders.
- **Vector Shapes:** Smooth GPU-drawn CPU/RAM history graphs.

### Phase 3: The Orchestration Layer
- **Demon Integration:** First-class AI agent support inside Tiles.
- **Modular Tiling:** Ability to snap new "Building Blocks" (Docker, Kubernetes, Logs) into the grid.

---
_Built for the Director. Fast, Light, and Coordinate-Perfect._
