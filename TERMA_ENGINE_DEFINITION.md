# ⚡ TERMA: THE UNIVERSAL TTY ENGINE

**"Terma is the nervous system for sovereign terminal applications."**

## 1. The Core Philosophy

Terma is a high-performance **Application Engine** built on top of `ratatui` and `crossterm`.

### The Distinction

- **Terminal Emulator (Alacritty/WezTerm):** The container. It handles font rendering and the OS window.
- **Terma (The Engine):** The content. It handles the **Logic**, **Input**, and **Grid State** of the application running inside the container.

We do not fight the terminal; we inhabit it.

## 2. The Architecture

Terma provides a standardized foundation for building complex TUI applications like **Tiles** and **Demon**.

### A. The Compositor

Instead of immediate-mode printing, Terma maintains a **Grid State** ("The Truth").

- **Resolution:** `Cols x Rows`.
- **Layers:** Supports Z-Index layering for popups, modals, and floating tiles.

### B. Input Normalization

Terma abstracts raw ANSI/Crossterm events into a semantic Event System.

- **Mouse:** Full support for `MouseMoved`, `Drag`, `Scroll`, and `Click` events (via SGR protocol).
- **Keyboard:** Complex chord handling.

### C. The Module System

Terma treats UI components as "Tiles".

- A **Tile** is a self-contained logic unit (e.g., File Browser, Docker Monitor).
- Modules are **Hot-Swappable**. A module built for Tiles can be loaded into other Terma-based apps.

## 3. Why Terma?

We believe the Terminal is the ultimate developer platform, but it lacks a standardized "Game Engine" for building complex tools.
most TUI libraries give you widgets. Terma gives you a **Runtime**.

- **Unified Aesthetic:** Round borders, consistent spacing, and "Director" design language.
- **Shared State:** All modules share the same memory space, enabling powerful interaction (e.g., AI Introspection).

---

_Terma is the engine. Tiles is the vehicle._
