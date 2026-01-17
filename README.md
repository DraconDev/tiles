# 👹 THE DRACON SOVEREIGN STACK !!!

Welcome to the **Sovereign Developer Environment**. This repository contains **Tiles**, the first application built on the **Terma Sovereign Engine**.

## 🚀 Projects in this Repo

### 1. [Tiles (The Application)](./plan.md)

A high-performance, modular data commander.

- **Role:** File Manager + Container Orchestrator + System Dashboard.
- **Tech:** Universal TTY, 60FPS Grid Rendering.

### 2. [Terma (The Engine)](./TERMA_ENGINE_DEFINITION.md)

The Unreal Engine of the TUI world.

- **Nature:** An application engine wrapper around Ratatui.
- **Key Files:** [Engine Definition](./TERMA_ENGINE_DEFINITION.md).

## 🏛️ Manifesto & Philosophy

- [**The Sovereign Terminal**](./BLUEPRINT.md): Why we utilize the terminal as our platform.
- [**Hybrid Identity**](./HYBRID_IDENTITY.md): Combining WezTerm, Zellij, and Yazi.
- [**Memory Efficiency**](./MEMORY_EFFICIENCY.md): How we stay under 20MB while others use 500MB+.

## 🛠️ Installation & Releases

### Download Pre-compiled Binaries
You can download the latest pre-compiled binaries for Linux, macOS, and Windows from the [GitHub Releases](https://github.com/DraconDev/tiles/releases) page.

### Build from Source
If you have Rust installed, you can build and install Tiles directly:

```bash
# Clone and install locally
git clone https://github.com/DraconDev/tiles
cd tiles
cargo install --path .

# Or run without installing
cargo run --release
```

---

_Built for the Agent Director Era. Owned by you._
