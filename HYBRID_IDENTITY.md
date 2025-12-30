# ☯️ The Hybrid Identity: Tiles & Terma

**Tiles is a "Polymorphic" Application.** It changes its nature based on where it runs.

## 1. The Chameleon Architecture

### 🖥️ Mode A: The "Native GUI" (Standalone)
When you run `tiles` from your desktop launcher:
*   **It is:** A **GUI Application** (using `winit` & `softbuffer`).
*   **How it works:** It opens its own window, completely independent of any terminal emulator.
*   **The Look:** It *looks* like a terminal (grid of characters), but it is actually drawing pixels directly.
*   **Superpowers:** 
    *   Zero-latency rendering (60+ FPS).
    *   Pixel-perfect image previews (no "sixel" hacks).
    *   Full mouse support (hover, drag, right-click).
    *   Custom shaders/blur effects (future).

### 📟 Mode B: The "Terminal Tenant" (Compatibility)
When you run `tiles` inside VS Code, `gnome-terminal`, or over SSH:
*   **It is:** A **TUI Application** (Standard ANSI Output).
*   **How it works:** It detects it is inside a TTY and switches drivers.
*   **The Look:** It adapts to the host terminal's font and color scheme.
*   **Superpowers:**
    *   **Ubiquity:** Runs on any server, VPS, or potato via SSH.
    *   **VS Code Integration:** You just open the VS Code Terminal (`Ctrl+` `) and type `./tiles`. It runs *inside* the pane.

## 2. Q&A: Addressing the Identity Crisis

**Q: "Is it a terminal window or a GUI one?"**
**A:** It is **BOTH**. 
*   On your local machine, it's a high-performance **GUI** that *mimics* a terminal. 
*   On a VPS, it's a standard **TUI**.

**Q: "Can I snap it in VS Code?"**
**A:** **Yes.** But not as a "GUI window." You use the VS Code **Terminal Panel**.
*   VS Code cannot easily embed external GUI windows (like Chrome or Calculator) into its layout.
*   However, VS Code has a *great* terminal emulator built-in. 
*   By running `tiles` inside that terminal pane, you get the "Snap" behavior for free.

**Q: "How are we different from Zed or Dolphin?"**
*   **Dolphin:** Pure GUI. Cannot run over SSH.
*   **Zed:** Pure GUI (GPU accelerated). Cannot run over SSH (yet, effectively).
*   **Tiles:** **Hybrid.** We give you the "Dolphin experience" (images, mouse) when local, but we don't abandon you when you SSH into a server. We just degrade gracefully to text mode.

## 3. The "Terminal Emulator" Clarification
You asked: *"can't we just install our own terminal like wes [WezTerm] and kitty?"*

**Tiles is NOT a Terminal Emulator (yet).**
*   **WezTerm/Kitty:** Programs that *run shells* (bash/zsh).
*   **Tiles:** An *application* (File Manager / Dashboard).

*However*, because **Terma** (our engine) renders its own grid, `tiles` in "GUI Mode" is technically **emulating a terminal grid** to display itself. We skipped the "middle man" (the terminal emulator) to draw directly to the OS window.

## 4. The Vision
We aren't building a generic terminal to run `ls`. We are building a **Post-Terminal User Interface (PTUI)**. 
*   It has the **speed** of a CLI.
*   It has the **visuals** of a GUI.
*   It has the **portability** of `vim`.