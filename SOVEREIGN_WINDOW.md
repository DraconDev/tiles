# 🔲 THE SOVEREIGN WINDOW: ARCHITECTURAL MANIFESTO

**Version:** 1.0
**Project:** Tiles / Terma
**Philosophy:** "The Grid is the Truth. The Window is the Fortress."

## 1. The Problem: The "Terminal Chaos"
Traditional TUI development is a battle against 40 years of legacy.
-   **Inconsistency:** Your app looks different in `xterm`, `iterm2`, `kitty`, and `cmd.exe`.
-   **Image Hell:** Every terminal has a different (often broken) protocol for high-res graphics (Sixel, Kitty, ITerm2).
-   **Input Bottleneck:** Terminals use ANSI escape codes. Capturing a "Forward Mouse Button" or a "Ctrl+Shift+A" is a nightmare of regex parsing and terminal-specific hacks.
-   **Performance:** You are sending text bytes to a 3rd party program (the terminal emulator), which then has to render them. This adds latency and overhead.

## 2. The Solution: The Sovereign Window
Instead of running **Tiles** *inside* a terminal, **Tiles *is* the terminal.**

By using the `terma` engine to spawn a native OS window (via `winit`) and rendering pixels directly to a framebuffer (via `softbuffer`), we seize control of the entire stack.

### How it differs from a GUI (Zed, Nautilus, VS Code):
| Feature | Traditional GUI | **Sovereign TUI (Tiles)** |
| :--- | :--- | :--- |
| **Layout** | Pixel-based / Flexbox | **Cell-based (Rows/Cols)** |
| **Navigation** | Free-form / Proportional | **Deterministic Grid (Predictable)** |
| **Density** | Low (Padding/Margins) | **High (Director Aesthetic)** |
| **Rendering** | Heavy Widget Trees | **Raw Pixel Blitting** |
| **Input** | Event-heavy (Focus/Blur) | **Command-first (Vim-style)** |

## 3. The "Grid is the API" Philosophy
We are not building a GUI. We are building a **Hardware-Accelerated TUI**.
-   **Deterministic UI:** Every element is addressable by `(col, row)`. This makes the interface 100% predictable for both humans and AI Agents (**DEMON**).
-   **Vim-Flow:** Because the grid is fixed, muscle memory never fails. `5j` always moves the cursor to the exact same vertical spot.
-   **Embedded Assets:** We don't rely on the user having a "Nerd Font" installed. We embed the font directly in the binary. Icons always work.

## 4. The Remote Superpower (Smart Client)
This is the "Killer Feature" for DevOps and SysAdmins.

### The Old Way (Dumb Terminal):
1.  SSH into a VPS.
2.  Run `tiles`.
3.  The VPS calculates the UI, encodes it as ANSI, and sends it over the wire.
4.  **Result:** Laggy UI, no images, high bandwidth.

### The Sovereign Way (Smart Client):
1.  Run `tiles` **locally** (The Sovereign Window).
2.  Connect to the VPS via SSH inside the app.
3.  The VPS sends **raw data** (JSON/Protobuf) about the files.
4.  The local window renders the UI at **60FPS** with local GPU acceleration.
5.  **Result:** Zero-lag navigation, high-res thumbnails of remote files, perfect mouse support.

## 5. Technical Stack
-   **Windowing:** `winit` (Native cross-platform windows).
-   **Graphics:** `softbuffer` (Software framebuffer for 2D efficiency).
-   **Fonts:** `rusttype` + Embedded **JetBrains Mono Nerd Font**.
-   **Logic:** `ratatui` (Layout management) + `tokio` (Async background tasks).

---
*Built for the Agent Director Era. No fallbacks. No compromises.*
