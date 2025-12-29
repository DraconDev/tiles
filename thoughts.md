# Thoughts on the "Demon" Architecture & Strategy

Based on the research and conversation regarding the 2025 AI editor landscape, here is the strategic validation for the **Tiles + Terma + Demon** stack.

## 1. The Core Thesis: "The Engine is the Product"
The market is flooding with "AI Editor Wrappers" (VS Code forks like Cursor/Antigravity). They compete on UI polish.
**Demon's Pivot:** By building a **Headless Rust Engine** first, we bypass the "Editor Wars" and enter the "Orchestration Wars."
*   **Tiles/Terma** is just the *viewport*.
*   **Demon** is the *intelligence*.

## 2. Technical Validation of "Terma" (The Custom Engine)
The bugs we faced today (freezing on Ctrl keys, extra rows, wide-char shifts) confirm why a custom engine is necessary but painful.
*   **Why not Crossterm?** It abstracts too much. We need raw SGR 1006 mouse events and Kitty keyboard protocols to make a TUI feel like a GUI.
*   **The "GPU" approach:** Terma's compositor (Z-index planes) aligns with modern GUI rendering (like Zed's GPUI) but constrained to the terminal grid. This is the *only* way to build a "TUI OS" that handles modals, floating windows, and complex layers performantly.

## 3. The "Atomic Command" Workflow (Unix Philosophy)
Instead of a "Stateful IDE" (where you must be inside the window to work), Demon is **Stateless & Triggered**.
*   **Command:** `demon --fix "auth logic"`
*   **Action:** Spawns background thread -> Indexes code -> Spawns Agent -> Edits Files -> Runs Tests -> Exits.
*   **UI:** Optional. Run `demon --ui` to attach and watch.
This is superior to Antigravity because it is **scriptable** and **headless-native** (works on VPS/SSH).

## 4. The Business Model: Dracon License (Source-Available)
*   **Open Core (The Face):** The CLI and Ratatui UI are open. Community grows the ecosystem.
*   **Proprietary Core (The Brain):** The advanced RAG, Symbol-Graph indexing, and Multi-Agent Orchestration are protected.
*   **The "5 Employee" Rule:** "Free for the 99% (Indies/Startups), Paid for the 1% (Enterprises)." This seeds the market while capturing value from those who have budget.

## 5. Next Steps for "Tiles" (The UI)
*   **Files:** Fix remaining visual bugs (folder sizes, rendering artifacts).
*   **Integration:** The "Tiles" app should basically be the default UI for the "Demon" engine. 
*   **Focus:** Stop trying to build a "Text Editor" inside Tiles. Build a **"Context Viewer"** and **"Agent Dashboard."** Let Neovim/VS Code handle the typing; let Demon handle the *thinking*.
