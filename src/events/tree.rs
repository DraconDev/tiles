use crate::app::{App, AppEvent};
use crate::state::{TreeColumn, TreeItem};
use ratatui::style::Color;
use std::path::Path;
use terma::input::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use tokio::sync::mpsc;

pub fn handle_tree_events(evt: &Event, app: &mut App, event_tx: &mpsc::Sender<AppEvent>) -> bool {
    if let Event::Key(key) = evt {
        if key.kind != KeyEventKind::Press {
            return false;
        }

        match key.code {
            KeyCode::Esc => {
                app.current_view = crate::app::CurrentView::Files;
                app.mode = crate::app::AppMode::Normal;
                // Clear any previews that might have been triggered
                for pane in &mut app.panes {
                    pane.preview = None;
                }
                return true;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                move_msg(app, 1);
                return true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                move_msg(app, -1);
                return true;
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => {
                enter_directory(app, event_tx);
                return true;
            }
            // Update key handler to support toggle hidden
            KeyCode::Char('h')
                if key
                    .modifiers
                    .contains(terma::input::event::KeyModifiers::CONTROL) =>
            {
                app.tree_state.show_hidden = !app.tree_state.show_hidden;
                refresh_tree(app);
                return true;
            }
            KeyCode::Left | KeyCode::Char('h') => {
                leave_directory(app);
                return true;
            }
            _ => {}
        }
    }
    false
}

// Mouse handling for Miller Columns
// Requires calculating column widths to know which column was clicked.
pub fn handle_tree_mouse(
    me: &MouseEvent,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
) -> bool {
    let (w, _h) = app.terminal_size;
    let col_count = app.tree_state.active_columns.len();
    if col_count == 0 {
        return false;
    }

    // Calculate widths to find which column was clicked
    let mut current_x: u16 = 0;
    let mut target_col_idx = None;

    for i in 0..app.tree_state.active_columns.len() {
        let col = &app.tree_state.active_columns[i];
        let width = col.width() as u16;

        // Determine if click is within this column [current_x, current_x + width)
        let end_x = current_x + width;

        if me.column >= current_x && me.column < end_x {
            target_col_idx = Some(i);
            break;
        }

        current_x += width;

        if current_x >= w {
            break; // Clicked beyond screen width?
        }
    }

    if let Some(col_idx) = target_col_idx {
        match me.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Calculate the Y offset for this column based on cascade layout
                // Each column starts at Y = sum of focus indices of all previous columns
                let mut col_y_offset: i32 = 0;
                for i in 0..col_idx {
                    col_y_offset += app.tree_state.active_columns[i].focus_index as i32;
                }

                // Tree area starts at y=0 (no header offset needed)
                let tree_area_y = 0;
                let global_scroll = app.tree_state.cascade_scroll as i32;
                let col_top_y = tree_area_y + col_y_offset - global_scroll;

                // Calculate local row within this column
                let local_row = me.row as i32 - col_top_y;

                if local_row < 0 {
                    return false; // Clicked above this column's content
                }

                let column = &app.tree_state.active_columns[col_idx];

                // Check if we clicked in the "spacer" area (gap for child column)
                let child_h = if col_idx + 1 < app.tree_state.active_columns.len() {
                    // Calculate child's expanded height
                    let heights = app
                        .tree_state
                        .calculate_expanded_heights(app.terminal_size.1);
                    heights.get(col_idx + 1).copied().unwrap_or(0)
                } else {
                    0
                };
                let spacer_size = child_h.saturating_sub(1);
                let old_focus_idx = column.focus_index;

                let clicked_idx: usize;
                let local_row_usize = local_row as usize;

                if spacer_size > 0 && col_idx + 1 < app.tree_state.active_columns.len() {
                    // This column has a child, so it has spacers after focus_index
                    if local_row_usize <= old_focus_idx {
                        clicked_idx = local_row_usize;
                    } else if local_row_usize <= old_focus_idx + spacer_size {
                        // Clicked in the spacer gap - ignore
                        return false;
                    } else {
                        clicked_idx = local_row_usize - spacer_size;
                    }
                } else {
                    clicked_idx = local_row_usize;
                }

                if clicked_idx < column.items.len() {
                    // Check if we're clicking on a DIFFERENT item than the current focus
                    // If so, truncate subsequent columns. If same item, do nothing special.
                    let is_different_item = clicked_idx != old_focus_idx;

                    // Update focus index
                    app.tree_state.active_columns[col_idx].focus_index = clicked_idx;
                    app.tree_state.focus_col_idx = col_idx;

                    // Only truncate if clicking on a different item (changing the path)
                    if is_different_item {
                        app.tree_state.active_columns.truncate(col_idx + 1);
                    }

                    // Check if clicked item is a directory and expand it
                    let is_dir = app.tree_state.active_columns[col_idx].items[clicked_idx].is_dir;
                    if is_dir
                        && (is_different_item || col_idx + 1 >= app.tree_state.active_columns.len())
                    {
                        enter_directory(app, event_tx);
                    }
                    return true;
                }
            }
            MouseEventKind::ScrollDown => {
                // Global Cascade Scroll
                app.tree_state.cascade_scroll = app.tree_state.cascade_scroll.saturating_add(3);
                return true;
            }
            MouseEventKind::ScrollUp => {
                app.tree_state.cascade_scroll = app.tree_state.cascade_scroll.saturating_sub(3);
                return true;
            }
            _ => {}
        }
    } else {
        // Clicked outside columns or on empty space?
        // Maybe handle scroll if general?
        match me.kind {
            MouseEventKind::ScrollDown => {
                app.tree_state.cascade_scroll = app.tree_state.cascade_scroll.saturating_add(3);
                return true;
            }
            MouseEventKind::ScrollUp => {
                app.tree_state.cascade_scroll = app.tree_state.cascade_scroll.saturating_sub(3);
                return true;
            }
            _ => {}
        }
    }

    false
}

fn move_msg(app: &mut App, delta: i32) {
    if app.tree_state.active_columns.is_empty() {
        return;
    }
    let idx = app.tree_state.active_columns.len() - 1;
    let col = &mut app.tree_state.active_columns[idx];

    let len = col.items.len();
    if len == 0 {
        return;
    }

    let mut new_sel = col.focus_index as i32 + delta;
    if new_sel < 0 {
        new_sel = 0;
    }
    if new_sel >= len as i32 {
        new_sel = len as i32 - 1;
    }

    col.focus_index = new_sel as usize;

    // Auto-select for keyboard nav (Single Selection behavior)
    col.selections.clear();
    let color = if col.items[col.focus_index].is_dir {
        Color::Blue
    } else {
        Color::Green
    };
    col.selections.insert(col.focus_index, color);

    // Scroll
    // height approximate
    let (_, h) = app.terminal_size;
    let view_h = h as usize;

    if col.focus_index >= col.offset + view_h {
        col.offset = col.focus_index + 1 - view_h;
    } else if col.focus_index < col.offset {
        col.offset = col.focus_index;
    }

    // Auto-expand on move? Or wait for enter?
    // Mac Finder auto-expands on move.
    // We need to call enter_directory logic but careful about lifetimes.
}

fn enter_directory(app: &mut App, event_tx: &mpsc::Sender<AppEvent>) {
    if app.tree_state.active_columns.is_empty() {
        return;
    }
    let last_idx = app.tree_state.active_columns.len() - 1;
    let focus_idx = app.tree_state.active_columns[last_idx].focus_index;

    if focus_idx >= app.tree_state.active_columns[last_idx].items.len() {
        return;
    }

    // Get the focused item
    let (is_dir, path) = {
        let item = &app.tree_state.active_columns[last_idx].items[focus_idx];
        (item.is_dir, item.path.clone())
    };

    if is_dir {
        // Load the directory contents as a simple flat column
        let new_col = load_column(&path, app.tree_state.show_hidden);
        app.tree_state.active_columns.push(new_col);
        app.tree_state.focus_col_idx = app.tree_state.active_columns.len() - 1;
    } else {
        // File - request preview
        let _ = event_tx.try_send(AppEvent::PreviewRequested(0, path));
    }
}

fn leave_directory(app: &mut App) {
    if app.tree_state.active_columns.len() > 1 {
        app.tree_state.active_columns.pop();
        app.tree_state.focus_col_idx = app.tree_state.active_columns.len() - 1;
    }
}

pub fn refresh_tree(app: &mut App) {
    // Reload all columns in the stack to ensure they are consistent with disk.
    // If stack is empty, load root.

    if app.tree_state.active_columns.is_empty() {
        // Load Root
        if let Some(fs) = app.current_file_state() {
            app.tree_state
                .active_columns
                .push(load_column(&fs.current_path, app.tree_state.show_hidden));
        }
        return;
    }

    // Re-load each column
    // We must preserve selection if possible.
    for col in &mut app.tree_state.active_columns {
        // Skip virtual columns (Multi-selection stacks)
        // Refreshing them using 'load_column' would try to read "Multi" path and return empty items,
        // causing a panic in renderer because 'sections' would still point to non-existent items.
        if col.path.to_string_lossy() == "Multi" || !col.sections.is_empty() {
            continue;
        }

        let new_col = load_column(&col.path, app.tree_state.show_hidden);
        // Restore selection
        if new_col.items.len() > col.focus_index {
            // Keep focus
            col.items = new_col.items;
        } else {
            col.items = new_col.items;
            col.focus_index = 0;
            col.selections.clear();
        }
    }
}

// Update signature of load_column to accept show_hidden
fn load_column(path: &Path, show_hidden: bool) -> TreeColumn {
    let mut items = Vec::new();
    if let Ok(entries) = std::fs::read_dir(path) {
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        // Filter hidden files
        if !show_hidden {
            entries.retain(|e| !e.file_name().to_string_lossy().starts_with('.'));
        }

        entries.sort_by(|a, b| {
            let ad = a.path().is_dir();
            let bd = b.path().is_dir();
            if ad != bd {
                bd.cmp(&ad)
            } else {
                a.file_name().cmp(&b.file_name())
            }
        });

        for e in entries {
            let p = e.path();
            let is_dir = p.is_dir();
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            items.push(TreeItem {
                path: p,
                name,
                depth: 0,
                is_dir,
                expanded: false,
                color: if is_dir { Color::Blue } else { Color::White },
            });
        }
    }

    TreeColumn {
        path: path.to_path_buf(),
        items,
        selections: std::collections::HashMap::new(),
        focus_index: 0,
        offset: 0,
        sections: Vec::new(),
    }
}
