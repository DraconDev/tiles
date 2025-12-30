# 🏗️ PROJECT TILES: THE DATA COMMAND CENTER
**Version:** 2.2 (The Director's Console)
**Legal:** Dracon License v1.0 (Source Available).
**Stack:** Rust, Ratatui, Terma, Tokio, Bollard, Sysinfo.

## 1. 🌍 The Vision
Tiles is not just a file manager; it is the **Data Command Center** for the Sovereign Developer.
It provides a high-density, mouse-driven, visual interface for **Files**, **Containers**, and **System Resources** using the "Director" aesthetic.

## 2. 🏛️ Technical Architecture
*   **Engine:** `terma` (Compositor, SGR 1006 Input, Kitty Graphics, Procedural Shapes).
*   **Layout:** Grid-based "Industrial" design. No "box-inside-a-box" padding.
*   **Input:** Hybrid. Keyboard for speed (`h/j/k/l`), Mouse for spatial orchestration (5-button support).

## 3. 🚀 Development Roadmap

### Phase 1: The Core Foundation (Completed)
- [x] **Engine:** Migrated from Crossterm to Terma.
- [x] **Input:** Reliable mouse clicks (0-based) and history nav (Back/Forward buttons).
- [x] **Rendering:** Fixed "Extra Row" bug and Wide-char alignment.
- [x] **Files:** Basic listing, sorting, and metadata (with correct symlink sizing).
- [x] **Visuals:** Flattened UI borders (Rounded) and removed redundant margins.

### Phase 2: The "Perfect" File Manager (Current Priority)
*   **Visual Polish:**
    *   [ ] **Icons:** Add NerdFont icons for sidebar (Home, Downloads, etc.) and better file type icons.
    *   [ ] **Contextual Menu:** Different menus for "Folder", "File", and "Empty Space".
    *   [ ] **Column Headers:** Click to sort (Asc/Desc toggle).
    *   [ ] **Selection:** Ensure drag-to-select works or feels natural.
*   **Navigation:**
    *   [ ] **Tabs:** Clickable tabs (MMB to close).
    *   [ ] **Arrow Keys:** Fix arrow key navigation if broken.
*   **CLI Integration:**
    *   [ ] **Smart Commands:** Formatted CLI output for AI consumption (`tiles --json list .`).

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

### Phase 5: The "Wow" Factor (Kitty Graphics)
*   [ ] **Thumbnails:** Show image/video previews on hover using Terma's image protocol.
*   [ ] **Asset Management:** "Drag-and-drop" assets between folders (visually).

## 4. 📝 Todo List (User Requests)
- [ ] **Sidebar Icons:** Specific icons for Home, Downloads, Documents, etc.
- [ ] **Better File Icons:** Distinguish types clearly.
- [ ] **Context Menu Logic:** Differentiate clicking on a file vs. empty space.
- [ ] **Header Sorting:** Click column headers to sort.
- [ ] **Tab Interaction:** Click to select, Middle-Click to close.
- [ ] **Visual Drag:** Drag files around? (Requires cursor tracking).
- [ ] **Key Navigation:** Ensure Arrow Keys work perfectly.
- [ ] **Remote Header:** Needs a better icon/section style.
- [ ] **Layout:** In "Split Mode", maximize space usage.

but can't we just install our own terminal like wes and kitty? cause its alreayd not built it, so instead of just having it and opening it in their terminal we alwyas open it in ours, and for vps we can even use ssh, so we dont have to have it installed there this way our tiles dreams are alive? so typing tiles opens our terminal not tiles in their terminal if this makes sense