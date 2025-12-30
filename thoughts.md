# Identity Crisis: The "Fish" vs "Window" Distinction

## The User's Confusion
The user asks: *"you can't snap files into vscode... but you can do fish... which is a different terminal, so which one is us?"*

They are conflating **Shells** (Fish/Bash), **Terminal Emulators** (VS Code Terminal/Kitty), and **GUI Apps** (Chrome/Dolphin).

## The Technical Reality
1.  **Fish:** A CLI Program (Text only). Runs *inside* a terminal. Snaps into VS Code.
2.  **Dolphin:** A GUI App (Pixels). Runs *outside* terminals. Independent window.
3.  **Tiles (Current Code):** A GUI App (Pixels).
    *   It uses `winit` to create a window.
    *   It uses `softbuffer` to draw pixels.
    *   **Behavior:** If run from VS Code, it **pops up** a new window. It does *not* snap.

## The "Hybrid" Goal
To satisfy the user's desire to "snap in VS Code" AND have "our own terminal window":
*   We need **Runtime Detection**.
*   `if is_tty && !force_gui`: Run as TUI (Text mode, snaps in VS Code).
*   `else`: Run as GUI (Window mode, independent).

## Answer Strategy
1.  **Clarify Fish:** Fish isn't a terminal; it's a tenant. VS Code is the building.
2.  **Define Tiles:** Currently, we are a **new building** (GUI Window). We sit *next* to VS Code.
3.  **The Switch:** We can allow Tiles to *visit* VS Code (Text Mode) to get that "snap" behavior, but we lose the high-res images in that mode.
