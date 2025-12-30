# 🛠️ THE DIRECTOR'S PALETTE: SYMBOLIC UI ELEMENTS

This document catalogs Unicode characters and techniques for creating high-fidelity "Tiles" in a standard terminal environment (no images required).

## 1. █ Block Elements (Solid UI & Geometry)
Used for solid backgrounds, thick borders, and building custom shapes.
*   `█` : Full Block (The primary building block)
*   `▀` : Top Half Block
*   `▄` : Bottom Half Block
*   `▌` : Left Half Block
*   `▐` : Right Half Block
*   `▛`, `▜`, `▙`, `▟` : Quadrant Blocks (Corners/Edges)

## 2. ░ Shade Elements (Texture & Depth)
Used for semi-transparent looks, shadows, and "glowing" panels.
*   `░` : Light Shade (25% opacity) - Good for ambient glows.
*   `▒` : Medium Shade (50% opacity) - Good for panel backgrounds.
*   `▓` : Dark Shade (75% opacity) - Good for deep shadows or "etched" metal.

## 3. │ Box Drawing (Sleek Linework)
For borders that don't look like "text boxes."
*   **Thin:** `│`, `─`, `┌`, `┐`, `└`, `┘`, `├`, `┤`, `┬`, `┴`, `┼`
*   **Double:** `║`, `═`, `╔`, `╗`, `╚`, `╝`, `╠`, `╣`, `╦`, `╩`, `╬` (Industrial look)
*   **Rounded:** `╭`, `╮`, `╯`, `╰` (Only if terminal font supports well)

## 4. ⣿ Braille Patterns (High-Res "Pixels")
Each cell is a 2x4 grid. Used for custom icons, small logos, and sparklines.
*   Example range: `⠁`, `⠂`, `⠄`, `⡀`, `⢀`, `⠠`, `⠐`, `⠈`
*   Full block: `⣿`
*   **Technique:** OR-ing bitmasks allows multiple agents to draw on one cell.

## 5. ◢ Geometric Details (Indicators & Slants)
*   `◢`, `◣`, `◥`, `◤` : Large triangles (Good for tab edges or "active" indicators).
*   `◆`, `◇`, `◈` : Diamonds (Status indicators).
*   `●`, `○`, `⦿` : Circles (Bullet points / Toggle states).

## 6. ▂ Data Visualization (Progress & Vitals)
*   Range: ` `, `▂`, `▃`, `▄`, `▅`, `▆`, `▇`, `█` (Lower blocks for bar graphs).
*   `▕`, `▏` : Vertical bar slices.

---

## 🎨 STYLING TECHNIQUES

### A. The "Etched" Look (Low-Contrast)
Set the **Background** to `RGB(12, 12, 12)` and the **Foreground** to `RGB(25, 25, 25)`.
Use `│` or `█`. The line will look like a physical groove in the UI rather than a bright line.

### B. High-Fidelity Buttons (3-Slice Block)
*   **Left:** `█` (FG: Accent Color, BG: Main BG)
*   **Middle:** `█` (FG: Main Panel Color, BG: Main BG)
*   **Right:** `█` (FG: Shadow Color, BG: Main BG)
*   Combine with text on top for a "Hardware" feel.

### C. The "Glass" Effect
Use `▒` (Medium Shade) with a Foreground color that is a mix of the background and a "light" color (e.g., Deep Blue). It creates a "translucent plastic" look.
