# 📜 The Sovereign Blueprint: Tiles & Terma

**Version:** 5.0 (The Universal Terminal Era)
**Status:** Core Pivot Complete

## 1. The Core Vision

We have refined the vision from a "Sovereign Window" to a **Universal Interface**.
`tiles` is not just an app; it is the **Ultimate Terminal Dashboard**.

- **Terma (The Engine)**: A robust TUI engine wrapper around `ratatui` that standardizes input handling and event loops.
- **Tiles (The Body)**: A high-performance remote/local file manager and dashboard that lives inside your existing terminal.
- **Philosophy**: "Any Terminal, Anywhere".

## 2. Architecture: Single-Stack Efficiency

We have simplified the architecture to a single, robust TTY runtime.

| Runtime           | Technology              | Use Case                          | Status        |
| :---------------- | :---------------------- | :-------------------------------- | :------------ |
| **Universal TTY** | `crossterm` + `ratatui` | SSH, VS Code, Linux Console, tmux | ✅ **Active** |

**Why TTY Only?**

- **Zero Dependencies**: No X11/Wayland libraries required. Builds instantly on any Linux machine.
- **Zero Latency**: Direct terminal rendering.
- **Maximum Portability**: Works over 3G SSH connections, inside docker containers, and within IDE terminals.

## 3. The "Secret Weapon": AI Introspection

Standard TUIs are opaque to AI (screens of text).
**Tiles is Transparent.**
We have created the **Introspection Module** (`tiles/src/modules/introspection.rs`).

- **Mechanism**: Serializes the `WorldState` (Tabs, Focus, Items) into a semantic structure.
- **Result**: The AI "reads" the mind of the app, knowing state directly without OCR or screen scraping.

## 4. Aesthetic Strategy: "Terminal High-Fidelity"

We prove that "Terminal" doesn't mean "Ugly".

- **Design System**: Strict usage of rounded borders, consistent spacing, and semantic coloring.
- **Input Mastery**: Full mouse support (including SGR extended mode) makes the TUI feel like a GUI.

## 5. Roadmap

1.  **Stabilize Core**: Ensure perfect scrolling and mouse interaction (Done).
2.  **Connect Demon**: Hook up the `Introspection` module to the actual AI Agent API.
3.  **Expansion**: Add more modules (Docker management, Git interface) now that the core is stable.

---

_The Sovereign Window is now the Sovereign Terminal. Everywhere you go, there you are._
