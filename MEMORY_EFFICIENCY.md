# 🧠 THE MEMORY MASTERCLASS: WHY 20MB?

**Project:** Tiles / Terma
**Subject:** Architectural Memory Efficiency
**The Core Secret:** "We don't have widgets. We only have pixels."

## 1. The Framebuffer Math (The Cost of Sight)
The absolute minimum memory required to show a window is determined by pixels.
For a standard 1080p window:
`1920 (width) x 1080 (height) x 4 (bytes per pixel: RGBA)` = **8,294,400 bytes (~8 MB)**.

-   **Tiles:** We allocate one or two of these buffers. Total: **16 MB**.
-   **VS Code/Electron:** They allocate hundreds of buffers for GPU textures, CSS layers, and browser tab isolation.

## 2. Why Standard GUIs (Files, VS Code) are 500MB+

### The "Browser Tax" (Electron/Zed)
Standard modern editors (VS Code, Slack, Discord) run inside a full **Chromium Browser**.
-   You aren't just running an editor; you are running a multi-process web engine.
-   **Cost:** 200MB just to open a blank window.

### The "Widget Tax" (GTK/Qt/Zed)
Standard GUIs use **Retained Mode Rendering**.
-   For every button, label, and icon on the screen, the OS keeps a "Widget Object" in memory.
-   Each object has properties: focus state, hover state, accessibility nodes, layout constraints (Flexbox/Grid), and event listeners.
-   **Cost:** Thousands of objects = massive memory overhead and "garbage collection" pauses.

## 3. Why Tiles is 20MB (The Sovereign Strategy)

### A. The Glyph Cache (Memory Recycling)
Instead of keeping a "Widget" for every letter on the screen, we use a **Glyph Cache**.
-   We render the letter 'A' **once** into a small pixel buffer.
-   Whenever we need to draw 'A', we just "copy-paste" (blit) those pixels to the framebuffer.
-   **Cost:** A few hundred KB for the entire font.

### B. The Deterministic Grid
In a GUI, the computer has to run complex math (Layout Engine) to figure out where a button goes (`margin-left: 10px; padding: 5%;`).
In **Tiles**, we know exactly where everything goes because it's a **Grid**.
-   `Col 5, Row 10` is always the same pixel coordinate.
-   We don't need a layout engine. We just need basic multiplication.

### C. No "DOM" or "Widget Tree"
Our "Logic" is just a `Vec<Cell>`. A `Cell` is just a few bytes (Char, FG Color, BG Color).
-   A screen of 80x40 characters is only **3,200 Cells**.
-   Even with high-density data, our "State" is only a few KB.

## 4. Summary: The "Director" Aesthetic
-   **Standard GUI:** A heavy, 3D-modeled world with complex physics (mushy, slow, heavy).
-   **Standard TUI:** A telegram sent over a wire (fast, but blind).
-   **Sovereign Window (Tiles):** A **Military-Grade Dashboard**. It uses the raw speed of the TUI logic but paints it directly onto the GPU canvas.

## Comparison Chart: The "Resource Pyramid"

| Layer | Sovereign Window (Tiles) | Standard GUI (Zed/Files) |
| :--- | :--- | :--- |
| **Runtime** | Bare-metal Rust (~1MB) | V8 Engine / JS Runtime (150MB+) |
| **UI State** | Simple Grid (~100KB) | DOM / Widget Tree (50MB+) |
| **Graphics** | Raw Framebuffer (8MB) | Composited Layers / Textures (100MB+) |
| **Total** | **~10-20MB** | **200MB - 1GB+** |

---
*Efficiency is Sovereignty. When your tools are light, you are fast.*
