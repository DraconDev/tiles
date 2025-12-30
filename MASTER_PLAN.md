# 👹 THE DRACON SOVEREIGN STACK: MASTER PLAN
**Version:** 2.0 (The Sovereign Window)
**Philosophy:** "The Engine is the Product."
**Legal:** Dracon License v1.0 (Sovereign/Open Core).

## 1. The Core Vision
The era of the "Code Editor" is ending. The era of the **Agent Director** has begun.
We are not building a better VS Code. We are building the **Subsurface Intelligence Layer** that empowers the Sovereign Developer.

**The Pivot (Dec 2025):**
We are abandoning the "Terminal Emulator" constraint. `Tiles` is no longer a CLI tool that runs *inside* `gnome-terminal`. It is a **Sovereign Application** that renders its own high-performance window using `terma`. This guarantees 100% image support, perfect color reproduction, and standard input handling.

**The Stack:**
1.  **Demon:** The Logic Engine (Headless, Multi-threaded Rust Daemon).
2.  **Tiles:** The Data Command Center (Standalone Windowed File Manager).
3.  **Source:** The Web Neutralizer (TUI Browser/Filter).
4.  **Terma:** The Engine (Windowing, Rendering, Input).

---

## 2. 🏛️ Architecture: The "Sovereign Trinity"

### Component A: DEMON (The Brain)
*   **Role:** Headless Agent Orchestrator.
*   **Workflow:** Stateless & Triggered (`demon --fix "auth"`).
*   **Deployment:** Runs on a persistent VPS ("The Beefy Server") or local machine.
*   **Tech:** Rust, Tokio, Tree-sitter (Context Pruning), Vector DB (Qdrant-lite).
*   **Key Feature:** **Shadow Worktrees**. Agents work in parallel git worktrees without blocking the user.

### Component B: TILES (The Memory)
*   **Role:** High-Density Data Commander.
*   **Workflow:** Visualizing the filesystem and swarm status.
*   **Tech:** Rust, Ratatui, **Terma Window (Winit/Softbuffer)**.
*   **Key Feature:** **Native Rendering**. No more "images not working in xterm". We draw the pixels.
*   **Remote Mode:** Connects to remote hosts via SSH but renders the UI **locally**. (Local 60FPS UI, Remote Data).

### Component C: SOURCE (The Senses)
*   **Role:** Protocol-Level Web Filter.
*   **Workflow:** "Neutralizing" the web into actionable schemas.
*   **Tech:** Headless Browser (Playwright/Chrome) -> Rust Semantic Parser -> TUI Grid.
*   **Key Feature:** **Ad/Outrage Blocking**. Re-renders web pages as clean data.

---

## 3. 🔧 The Engine: TERMA
**"Crossterm is for Text. Terma is for Interfaces."**
*   **Backend:**
    *   **Window Mode (Primary):** Spawns a dedicated OS window. Renders a grid of cells + images directly to a framebuffer.
    *   **TTY Mode (Legacy/Headless):** Standard ANSI output for scripts/SSH-piping.
*   **Compositor:** Z-Index Planes, Transparency, Floating Modals.
*   **Input:** Native OS Events (Mouse, Keyboard) mapped to TUI logic. No more regex parsing of ANSI escape codes.
*   **Visuals:**
    *   **Font Rendering:** Embedded NerdFont (JetBrains Mono).
    *   **Images:** Direct pixel blitting (No Kitty Protocol needed in Window Mode).

---

## 4. 💼 Business Model: The Dracon License
*   **Open Core:** CLI/TUI binaries are free for individuals and small teams (<5 employees).
*   **Proprietary Intelligence:** The "Demon Brain" (Advanced RAG, Symbol Graph, Swarm Logic) is protected.
*   **Monetization:**
    *   **Demon Pro:** Subscription for pre-indexed symbol maps and high-speed cloud inference.
    *   **Enterprise:** "Sovereign License" for running Demon on air-gapped corporate servers.

---

## 5. 🚀 Grand Unified Roadmap

### Phase 1: The Engine Rebirth (Current)
*   [x] **Terma Core:** Compositor & Planes.
*   [ ] **Terma Window:** Implement `winit` + `softbuffer` + `swash` (font rendering).
*   [ ] **Migrate Tiles:** Switch `Tiles` main loop to use `TermaWindow` instead of `TermaTTY`.

### Phase 2: The "Hacker-man" Tools
*   **Tiles:** Implement "Image Tiles" using direct pixel rendering (bypass ASCII limitations).
*   **Terma:** Add "Sprite" support for UI chrome (buttons/dividers).

### Phase 3: The Swarm
*   **Demon:** Implement Shadow Worktrees for parallel agent execution.
*   **Tiles:** Add "Swarm Dashboard" plane (Z-index 100) to visualize background threads.

### Phase 4: The Sovereign Web
*   **Source:** Build the headless scraper and schema renderer.
*   **Integration:** Demon can "read" docs via Source to fix code in Tiles.

### Phase 5: The "Trojan Horse"
*   **Extension:** Build a lightweight VS Code extension that pipes the Demon TUI into the integrated terminal (Using TTY fallback).
