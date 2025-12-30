# ⚡ TERMA: THE APPLICATION ENGINE DEFINITION

**"Terma is not a vessel for shells. It is the nervous system for sovereign applications."**

## 1. The Critical Distinction: Engine vs. Emulator

There is a fundamental misunderstanding in the world of TUI development. Most people think they are building "Terminal Apps." They are actually building "Guests" that live inside "Terminal Emulators."

### The Terminal Emulator (The Vessel)
- **Examples:** Alacritty, Kitty, WezTerm, iTerm2.
- **Goal:** To host a `shell` (bash, zsh, fish).
- **Architecture:** A generic container that interprets a stream of 40-year-old ANSI escape codes and draws them. It has no idea what the application inside is doing.
- **Role:** A Television Set. It doesn't care what show is playing.

### Terma (The Application Engine)
- **Examples:** Used to build **Tiles**, **Demon**, and **Source**.
- **Goal:** To provide a high-performance, grid-based graphical environment for **Logic Modules**.
- **Architecture:** A specialized framework (like a Game Engine) that integrates UI, Input, and Logic into a single memory space.
- **Role:** The **Game Engine (Unreal/Unity)**. It is built *into* the game, not around it.

## 2. The Shared Template (The Dracon Framework)
Terma serves as the **Standardized Foundation** for the entire Dracon ecosystem. By using Terma, all our applications gain:

1.  **Unified Aesthetic:** The "Director" design language (Monospace, high-density, industrial) is baked into the engine.
2.  **Shared Performance:** Every app uses the same ultra-fast Glyph Cache and pixel-blitter.
3.  **Input Consistency:** Complex key-chords and 5-button mouse events work identically in Tiles as they do in Demon.
4.  **Module Tiling:** Because Terma treats "Tiles" as first-class objects, a module built for **Tiles** can be hot-swapped into **Demon** without a rewrite.

## 3. Why This Path?
We are not building a generic terminal emulator because **Generic Emulators are limited by the lowest common denominator.** 

- If we want to show a 60FPS real-time CPU waveform overlaying a file list, we don't want to "ask" a terminal to support it. We just blit the pixels.
- If we want an AI Agent (**DEMON**) to move the cursor based on visual recognition, we don't want to parse ANSI strings. We want direct access to the Compositor State.

## 4. Summary: The Identity
Terma is a **TUI-Construction Kit** for developers who want the **speed/ergonomics of a Terminal** but the **power/sovereignty of a Native GPU App.**

- **Alacritty** is a place where you run `ls`.
- **Terma** is the logic that defines what `Tiles` is.

---
*We are not building a better TV. We are building the broadcast station.*
