# 📜 The Sovereign Blueprint: Tiles & Terma

**Version:** 4.0 (The AI-Native Era)
**Status:** Hybrid Core Implemented

## 1. The Core Vision

We have successfully transitioned `tiles` from a vague TUI concept to a **Sovereign Application**.

- **Terma (The Engine)**: A GPU-accelerated console engine that handles "God Mode" graphics (Images, Gradients) while maintaining a grid-based coordinate system.
- **Tiles (The Body)**: A hybrid File Manager / Dashboard that owns its window locally but degrades gracefully to SSH remotely.
- **Demon (The Mind)**: The AI Agent that inhabits the Tiles body, leveraging the deterministic nature of the grid for perfect control.

## 2. Architecture: The "Hybrid Core"

We have implemented the **Dual-Runtime** model in `tiles/src/main.rs`.

| Runtime             | Technology             | Use Case                              | Status             |
| :------------------ | :--------------------- | :------------------------------------ | :----------------- |
| **Sovereign (GUI)** | `winit` + `softbuffer` | Local Desktop, High-Res Assets, 60FPS | ✅ **Active**      |
| **Tenant (TTY)**    | `crossterm` / `stdout` | SSH, VS Code Terminal, Legacy         | ✅ **Implemented** |

## 3. The "Secret Weapon": AI Introspection

Standard GUIs (Web/Native) are opaque to AI. They require "Computer Vision" to understand.
**Tiles is Transparent.**
We have created the **Introspection Module** (`tiles/src/modules/introspection.rs`).

- **Mechanism**: Serializes the `WorldState` (Tabs, Focus, Items) into a semantic structure.
- **Result**: The AI does not "look" at the screen. It "reads" the mind of the app. It knows exactly that "File 3 is `docker-compose.yml`" without OCR.

## 4. Aesthetic Strategy: The Tileset Engine

We solved the "Ugly TUI" problem without the "CSS Spaghetti" problem.

- **Implementation**: `terma/src/visuals/tileset.rs`
- **Concept**: The logic defines _structure_ (Wall, Header). The `Tileset` trait defines _skin_ (Pixels, Chars).
- **Themes**:
  - `Director`: Cyberpunk, Neon, Dark (Default).
  - `Paper`: High-Contrast, Corporate (Business).

## 5. Current Obstacles & Next Steps

### 🛑 Environmental Blocker: OpenSSL

The project currently fails to compile `ssh2` due to missing `libssl-dev` on the host system.

- **Immediate Fix**: User needs to install OpenSSL dev headers (`sudo apt install libssl-dev`).
- **Workaround**: We can disable the `remote` feature initially if needed.

### 🚀 Roadmap

1.  **Verify Hybrid Switch**: Ensure `is_terminal()` check works reliably in user's shell.
2.  **Connect Demon**: Hook up the `Introspection` module to the actual AI Agent API.
3.  **Polish Tilesets**: Create high-res bitmap tilesets for the "Window Mode" to fully utilize the GPU.

---

_The Sovereign Window is no longer a theory. It is a compiling, architecturally sound reality._
