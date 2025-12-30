# 🏗️ PROJECT TILES: THE DATA COMMAND CENTER
**Version:** 3.1 (The Visual Evolution)
**Stack:** Rust, Ratatui, Terma Engine, Tokio.

## 1. 🚀 Development Status

### Phase 1: The Engine Rebirth (COMPLETED)
- [x] **Standalone Window:** App opens its own OS window.
- [x] **Direct Rendering:** Pixels blitted to framebuffer (No TTY lag).
- [x] **Decoupled Events:** Custom `terma::Event` system replaces `crossterm`.
- [x] **Embedded Assets:** Font bundled into binary.

### Phase 2: Visual Intelligence (CURRENT PRIORITY)
*   **Asset Engine:**
    *   [ ] **Real Image Previews:** Blit raw pixels for JPG/PNG thumbnails.
    *   [ ] **Icon Overhaul:** Replace symbolic icons with high-res "Sprite" icons.
    *   [ ] **Smooth Scaling:** Logic to fit high-res images into grid cells without distortion.
*   **UI Polish:**
    *   [ ] **Custom Chrome:** Rounder corners and custom dividers using Terma's Shape engine.
    *   [ ] **Animations:** Support for subtle frame-based transitions (e.g., sliding tabs).

### Phase 3: The System Cockpit
*   **Goal:** Modular "System Lego" blocks.
*   **Features:**
    *   [ ] **Docker Block:** Live container logs and control.
    *   [ ] **System Block:** GPU-drawn telemetry graphs.

## 2. 📝 Current Todo
1.  **Refactor Asset Pipeline:** Create a system to load and cache PNGs into Terma's `image_assets`.
2.  **Implementation:** Show an image thumbnail when a user selects an image file.
3.  **Optimization:** Ensure 60FPS while blitting multiple high-res thumbnails.

