# 👹 THE DRACON SOVEREIGN STACK: MASTER PLAN
**Version:** 1.0 (The Agent Director Strategy)
**Philosophy:** "The Engine is the Product."
**Legal:** Dracon License v1.0 (Sovereign/Open Core).

## 1. The Core Vision
The era of the "Code Editor" is ending. The era of the **Agent Director** has begun.
We are not building a better VS Code. We are building the **Subsurface Intelligence Layer** that empowers the Sovereign Developer.

**The Stack:**
1.  **Demon:** The Logic Engine (Headless, Multi-threaded Rust Daemon).
2.  **Tiles:** The Data Command Center (TUI File Manager).
3.  **Source:** The Web Neutralizer (TUI Browser/Filter).
4.  **Terma:** The Hardware Abstraction (Compositor/Input Engine).

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
*   **Tech:** Rust, Ratatui, Terma Compositor.
*   **Key Feature:** **Kitty Graphics Thumbnails**. Viewing assets/plots directly in the terminal.
*   **Integration:** Acts as the default "View Layer" for the Demon.

### Component C: SOURCE (The Senses)
*   **Role:** Protocol-Level Web Filter.
*   **Workflow:** "Neutralizing" the web into actionable schemas.
*   **Tech:** Headless Browser (Playwright/Chrome) -> Rust Semantic Parser -> TUI Grid.
*   **Key Feature:** **Ad/Outrage Blocking**. Re-renders web pages as clean data, stripping emotional manipulation and tracking.

---

## 3. 🔧 The Engine: TERMA
**"Crossterm is for Text. Terma is for Interfaces."**
*   **Compositor:** Z-Index Planes, Transparency, Floating Modals.
*   **Input:** Native SGR 1006 (5-button mouse), Kitty Keyboard Protocol.
*   **Visuals:** Procedural Assets (Shapes), Kitty Graphics Protocol (Images).
*   **Render Strategy:** Explicit Row Positioning (Robust against terminal wrap bugs).

---

## 4. 💼 Business Model: The Dracon License
*   **Open Core:** CLI/TUI binaries are free for individuals and small teams (<5 employees).
*   **Proprietary Intelligence:** The "Demon Brain" (Advanced RAG, Symbol Graph, Swarm Logic) is protected.
*   **Monetization:**
    *   **Demon Pro:** Subscription for pre-indexed symbol maps and high-speed cloud inference.
    *   **Enterprise:** "Sovereign License" for running Demon on air-gapped corporate servers.

---

## 5. 🚀 Grand Unified Roadmap

### Phase 1: The Foundation (Current)
*   [x] **Terma:** Stable Engine (Compositor, Input, Shapes).
*   [x] **Tiles:** Functional File Manager (Layout, Basic Ops).
*   [ ] **Demon:** Basic CLI structure (`clap`).

### Phase 2: The "Hacker-man" Tools (Next)
*   **Tiles:** Implement Kitty Graphics for image previews.
*   **Terma:** Add "Sprite" support for UI chrome (buttons/dividers).
*   **Demon:** Implement "Context Scalpel" (Tree-sitter pruning).

### Phase 3: The Swarm
*   **Demon:** Implement Shadow Worktrees for parallel agent execution.
*   **Tiles:** Add "Swarm Dashboard" plane (Z-index 100) to visualize background threads.

### Phase 4: The Sovereign Web
*   **Source:** Build the headless scraper and schema renderer.
*   **Integration:** Demon can "read" docs via Source to fix code in Tiles.

### Phase 5: The "Trojan Horse"
*   **Extension:** Build a lightweight VS Code extension that pipes the Demon TUI into the integrated terminal.
