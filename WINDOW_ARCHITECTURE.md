# 🏗️ Terma Window Architecture
**Goal:** Render a TUI-style grid of cells into a hardware-accelerated OS window.

## 1. The Stack
We are avoiding heavy game engines (Bevy/Godot) and web-views (Tauri/Electron). We want raw speed and minimal bloat.

*   **Windowing:** `winit` (The Rust standard for cross-platform windows).
*   **Rendering:** `softbuffer` (Cross-platform software framebuffer).
    *   *Why?* We are drawing a grid of fixed-width characters. We don't need complex 3D shaders. A simple pixel buffer is plenty fast for 1080p/4k text.
    *   *Alternative:* `pixels` (similar, but strictly WGPU based). `softbuffer` is simpler for now.
*   **Font Rendering:** `swash` or `ab_glyph`.
    *   *Strategy:* **Glyph Cache**. We render each character (e.g., 'A' in bold red) once to a texture atlas, then blit it to the screen.

## 2. The Loop (`TermaBackend`)

The `TermaBackend` will implement `ratatui::Backend`.

### Step A: The Grid State
*   The `Compositor` maintains the "Truth" (The grid of `Cell`s).
*   Resolution: `Cols x Rows`.
*   Calculated by: `WindowPixelWidth / CharPixelWidth`.

### Step B: The Draw Call (`flush()`)
When `terminal.draw()` calls `flush()`:
1.  **Lock Framebuffer:** Get the mutable `&[u32]` buffer from `softbuffer`.
2.  **Iterate Cells:** Loop through the Compositor's grid.
3.  **Render Glyphs:**
    *   For each cell `(x, y)`:
    *   Calculate pixel coordinates: `px = x * char_w`, `py = y * char_h`.
    *   Fill background color rect.
    *   Look up Glyph in Cache (Char + Style + Color).
    *   Blit Glyph pixels over background.
4.  **Render Images:**
    *   Iterate `image_assets`.
    *   Blit RGBA pixels directly over the buffer at the correct coordinates.
5.  **Present:** Submit the buffer to the OS.

## 3. Input Handling
We must map `winit` events to `terma::Event` (which mirrors `crossterm::event::Event` for now to keep the rest of the app working).

*   **Mouse:**
    *   `CursorMoved` -> Calculate `(col, row)` based on `(pixel_x, pixel_y)`.
    *   `MouseInput` -> Map Left/Right/Middle/Back/Forward.
    *   `MouseWheel` -> Map ScrollUp/Down.
*   **Keyboard:**
    *   `KeyboardInput` -> Map standard keys.
    *   **Modifiers:** Track Shift/Ctrl/Alt state manually.

## 4. Font Strategy
We will **Embed** a high-quality Nerd Font (e.g., JetBrains Mono Nerd Font) directly into the binary (`include_bytes!`).
This guarantees that all icons work on all machines, zero configuration required.

## 5. Directory Structure
```
terma/
├── src/
│   ├── backend/
│   │   ├── mod.rs
│   │   ├── tty.rs      # The old ANSI backend
│   │   └── window.rs   # The new Winit backend
│   ├── renderer/
│   │   ├── font.rs     # Glyph caching
│   │   └── soft.rs     # Softbuffer logic
```
