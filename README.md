# 👹 THE DRACON SOVEREIGN STACK

Welcome to the **Sovereign Developer Environment**. This repository contains **Tiles**, the first application built on the **Terma Sovereign Engine**.

## 🚀 Projects in this Repo

### 1. [Tiles (The Application)](./plan.md)
A high-performance, modular data commander.
- **Role:** File Manager + Container Orchestrator + System Dashboard.
- **Tech:** Standalone OS Window, 60FPS Grid Rendering.

### 2. [Terma (The Engine)](./TERMA_ENGINE_DEFINITION.md)
The Unreal Engine of the TUI world.
- **Nature:** An application engine, not an emulator.
- **Key Files:** [Engine Definition](./TERMA_ENGINE_DEFINITION.md) | [Window Architecture](./WINDOW_ARCHITECTURE.md).

## 🏛️ Manifesto & Philosophy
- [**The Sovereign Window**](./SOVEREIGN_WINDOW.md): Why we bypass the terminal emulator.
- [**Hybrid Identity**](./HYBRID_IDENTITY.md): Combining WezTerm, Zellij, and Yazi.
- [**Memory Efficiency**](./MEMORY_EFFICIENCY.md): How we stay under 20MB while others use 500MB+.

## 🛠️ Getting Started
```bash
# Run the Sovereign Window
cargo run

# Run the input debugger
cargo run -p terma --example input_debug
```

---
*Built for the Agent Director Era. Owned by you.*
