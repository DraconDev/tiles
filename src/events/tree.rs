use crate::app::{App, AppEvent};
use crate::state::{TreeColumn, TreeItem};
use ratatui::style::Color;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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

    // We need to calculate EXACT widths to know which column was clicked.
    // This logic must match 'draw_tree_view' exactly.
    // We assume 'scroll_offset_col' is correct from the last draw.

    let start_col = app.tree_state.scroll_offset_col;
    let mut current_x = 0; // Relative to tree area X (which we assume is 0 or we subtract area.x if we knew it)
                           // The Event 'me.column' is global screen coordinates.
                           // If our Tree View starts at x=0 (which it does in full screen), then me.column is correct.
                           // If there is sidebar/padding, we might need adjustments.
                           // Assuming effective full screen or main pane.

    // We iterate visible columns starting from scroll offset
    let mut target_col_idx = None;

    for i in start_col..app.tree_state.active_columns.len() {
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
                // Determine Row
                // Calculate row index relative to list
                // y=0 is top content line (assuming no header or header handled by rect.y)
                let row = me.row.saturating_sub(1) as usize; // Adjust if header exists

                let column = &app.tree_state.active_columns[col_idx];

                let clicked_idx;

                if column.sections.is_empty() {
                    clicked_idx = column.offset + row;
                } else {
                    // Stacked Column Logic
                    // We need to replicate the layout to verify which section was clicked.
                    // Assume area height is terminal_height - 2 (Header + Footer)?
                    // Better to rely on what UI uses. ui/mod.rs calls draw_tree_view(f, f.area(), app) IF CurrentView::Tree.
                    // If CurrentView::Tree, typically f.area() is full screen (minus nothing?).
                    // But main.rs usually renders Header/Footer and THEN the view.
                    // Let's assume view starts at y=1 and height is h-2.
                    let (_, h) = app.terminal_size;
                    let area_height = h.saturating_sub(2);

                    let heights = column.calculate_section_heights(area_height);
                    let mut current_y = 0;
                    let mut found_idx = None;

                    for (sec_idx, &h) in heights.iter().enumerate() {
                        // Section visual height is h + 2 (borders)
                        let total_h = (h + 2) as usize;

                        if row >= current_y && row < current_y + total_h {
                            // Clicked in this section!
                            // Check if on border (first or last line of section)
                            let local_y = row - current_y;
                            if local_y == 0 || local_y == total_h - 1 {
                                // Border click - ignore or select section header?
                                // Ignore for now.
                            } else {
                                // Content click
                                // 1-indexed inside section (due to top border)
                                let content_y = local_y - 1;
                                let start_idx = column.sections[sec_idx].start_index;
                                let section_item_idx = start_idx + content_y;
                                if section_item_idx < column.sections[sec_idx].end_index {
                                    found_idx = Some(section_item_idx);
                                }
                            }
                            break;
                        }
                        current_y += total_h;
                    }

                    if let Some(idx) = found_idx {
                        clicked_idx = idx;
                    } else {
                        return false; // Clicked on border or gap
                    }
                }

                if clicked_idx < column.items.len() {
                    // Update focus index
                    app.tree_state.active_columns[col_idx].focus_index = clicked_idx;

                    // Handle Selection (Ctrl for multi, else Additive/Toggle as per recent change)
                    let color = if app.tree_state.active_columns[col_idx].items[clicked_idx].is_dir
                    {
                        Color::Blue
                    } else {
                        Color::Green
                    };

                    if app.tree_state.active_columns[col_idx]
                        .selections
                        .contains_key(&clicked_idx)
                    {
                        // Deselect
                        app.tree_state.active_columns[col_idx]
                            .selections
                            .remove(&clicked_idx);
                    } else {
                        // Select
                        app.tree_state.active_columns[col_idx]
                            .selections
                            .insert(clicked_idx, color);
                    }

                    // Truncate columns beyond this one
                    app.tree_state.active_columns.truncate(col_idx + 1);
                    app.tree_state.focus_col_idx = col_idx;

                    // Expand selections
                    enter_directory(app, event_tx);
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
