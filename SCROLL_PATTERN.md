# The Ratatui "Free Scroll" Pattern

**Problem:** Standard Ratatui `Table` and `List` widgets aggressively auto-scroll to keep the `selected` item in view during rendering. This causes "stuck" scrolling or "jump back" glitches when you try to implement manual mouse scrolling (viewport manipulation) that moves the view away from the selected item.

**Solution:** **Render-Time Selection Masking**. We decouple the "Model" (App State) from the "View" (Render State).

## Core Principles

1.  **Manual Authority:** The App State (`scroll_offset`) is the single source of truth for the viewport position.
2.  **Passive Renderer:** The rendering logic uses a **temporary** state. It never writes back to the App State.
3.  **Selection Masking:** We only tell the Widget about the selection *if and only if* it is currently visible in our manual viewport. If it's out of view, we tell the Widget "nothing is selected". This prevents the Widget's internal auto-scroll logic from triggering.

## Implementation Recipe

### 1. The App State (Model)
Store the selection and offset separately. Do not rely on `TableState` persistence for logic.

```rust
struct FileState {
    pub items: Vec<String>,
    pub selected_index: Option<usize>, // The logical selection
    pub table_state: TableState,       // Persists only the Offset
}
```

### 2. Manual Scroll Logic (Mouse)
Update the offset directly. Do not touch the selection.

```rust
fn scroll_down(&mut self) {
    let max_offset = self.items.len().saturating_sub(1);
    // Move viewport independently
    let new_offset = (self.table_state.offset() + 1).min(max_offset);
    *self.table_state.offset_mut() = new_offset;
}
```

### 3. Manual Auto-Scroll (Keyboard)
Since we disabled the Widget's auto-scroll, we must implement "keep selection in view" manually when moving the cursor.

```rust
fn move_cursor_down(&mut self) {
    // 1. Update Selection
    self.selected_index += 1;
    
    // 2. Manual Auto-Scroll
    let offset = self.table_state.offset();
    let capacity = self.view_height.saturating_sub(2); // Header padding
    
    if self.selected_index >= offset + capacity {
        // Scroll forward to keep item in view
        *self.table_state.offset_mut() = self.selected_index - capacity + 1;
    }
}
```

### 4. The Renderer (The Trick)
In your draw function, create a **Temporary State** and apply the masking logic.

```rust
fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    let fs = &mut app.file_state;
    
    // A. Create Temporary State
    let mut render_state = TableState::default();
    
    // B. Force Offset Authority
    *render_state.offset_mut() = fs.table_state.offset();

    // C. Smart Selection Masking
    if let Some(sel) = fs.selected_index {
        let offset = fs.table_state.offset();
        let height = area.height as usize;
        let capacity = height.saturating_sub(2); // Adjust for headers/borders
        
        // Strict Visibility Check:
        // Only select if strictly inside the view (sel > offset).
        // If it touches the top edge (sel == offset), standard widgets might still snap.
        // Adjust strictness based on widget behavior (Table is sensitive).
        if sel >= offset && sel < offset + capacity {
            render_state.select(Some(sel));
        } else {
            // HACK: Tell widget "nothing selected" so it doesn't auto-scroll
            render_state.select(None);
        }
    }

    // D. Render with Temporary State
    f.render_stateful_widget(Table::new(...), area, &mut render_state);
    
    // CRITICAL: Do NOT sync render_state.offset() back to fs.table_state!
}
```
