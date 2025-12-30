# 🏗️ PROJECT TILES: THE DATA COMMAND CENTER
**Version:** 3.0 (The Sovereign Window)
**Legal:** Dracon License v1.0 (Source Available).
**Stack:** Rust, Ratatui, Terma (Window Mode), Tokio, Bollard, Sysinfo.

## 1. 🌍 The Vision
Tiles is **The Data Command Center**.
It is a standalone, windowed application that combines the density of a TUI with the graphical capabilities of a GUI.
It does **not** run inside your terminal emulator. It **is** the terminal.

## 2. 🏛️ Technical Architecture
*   **Engine:** `terma` (Window Mode).
    *   **Backend:** `winit` (Window/Input) + `softbuffer` (Rendering) + `swash` (Fonts).
    *   **Logic:** Renders a grid of `Cells` just like a TUI, but draws them pixel-perfectly.
*   **Layout:** Grid-based "Industrial" design.
*   **Input:** 100% reliable Mouse (Left/Right/Middle/Back/Forward/Scroll) + Keyboard.

## 3. 🚀 Development Roadmap

### Phase 1: The Engine Rebirth (Current Priority)
- [ ] **Dependencies:** Add `winit`, `softbuffer`, `swash` (or similar) to `terma`.
- [ ] **Window Backend:** Create `WindowBackend` implementing `ratatui::Backend`.
- [ ] **Font Rendering:** Implement a fast cache to draw Monospace cells.
- [ ] **Tiles Migration:** Switch `main.rs` to spawn the window instead of using stdout.

### Phase 2: The "Perfect" File Manager
*   **Visual Polish:**
    *   [ ] **Real Images:** Replace "Kitty Protocol" hacks with direct pixel blitting in `Terma`.
    *   [ ] **Icons:** Render NerdFonts reliably without worrying about user's installed fonts.
    *   [ ] **Contextual Menu:** Different menus for "Folder", "File", and "Empty Space".
*   **Navigation:**
    *   [ ] **Tabs:** Clickable tabs (MMB to close).
    *   [ ] **Remote:** SSH Connect -> Local Window (The "Remote Superpower").

### Phase 3: The System Cockpit (Processes)
*   **Goal:** "htop" but better.
*   **Features:**
    *   [ ] **Process Tree:** Interactive visual tree.
    *   [ ] **Kill Switch:** Right-click context menu to `kill -9`.
    *   [ ] **Visuals:** Use Terma Shapes for smooth CPU/RAM history graphs (Sparklines).

### Phase 4: The Container Orchestrator (Docker)
*   **Goal:** "Lazydocker" but integrated.
*   **Features:**
    *   [ ] **Live State:** Connect `bollard` for real-time updates.
    *   [ ] **Controls:** Start/Stop/Restart context menu.
    *   [ ] **Logs:** Streaming log view in a floating Terma plane.

## 4. 📝 Todo List (Architecture Shift)
- [ ] **Research:** Verify `softbuffer` performance for 120fps TUI rendering.
- [ ] **Font:** Pick the "Official" Dracon font (JetBrains Mono Nerd Font?) and embed it in the binary.
- [ ] **Input:** Map `winit` events to `crossterm::event::Event` enum for compatibility with existing logic? Or create `terma::Event`? (Prefer `terma::Event`).

