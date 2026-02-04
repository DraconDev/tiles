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

    // Simple equal width assumption for now (must match UI)
    // In UI we might do: if col_count == 1 { 100% }, else { 50% last, others shared? }
    // Let's assume standardized width logic.
    // Use `get_column_rects` helper if we shared it, but for now approximate:
    let col_width = w as usize / col_count.max(1);

    match me.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let col_idx = (me.column as usize) / col_width.max(1);
            if col_idx < app.tree_state.active_columns.len() {
                // Focus this column?
                // Miller columns usually strictly focus the right-most,
                // but clicking a previous column usually truncates the stack to that point + selection.
                // Let's truncate stack to col_idx + 1 if clicked.

                // Calculate row index relative to list
                // y=0 is top? No, y starts at content.
                // assuming full screen, No Header.
                // y=0.
                let row = me.row as usize;

                let column = &app.tree_state.active_columns[col_idx];
                let clicked_idx = column.offset + row;

                if clicked_idx < column.items.len() {
                    // Select this item
                    app.tree_state.active_columns[col_idx].selected = clicked_idx;

                    // Truncate columns beyond this one
                    app.tree_state.active_columns.truncate(col_idx + 1);
                    app.tree_state.focus_col_idx = col_idx;

                    // If it's a dir, expand it (push next col)
                    enter_directory(app, event_tx);
                    return true;
                }
            }
        }
        MouseEventKind::ScrollDown => {
            move_msg(app, 1); // Scroll active column?
            return true;
        }
        MouseEventKind::ScrollUp => {
            move_msg(app, -1);
            return true;
        }
        _ => {}
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

    let mut new_sel = col.selected as i32 + delta;
    if new_sel < 0 {
        new_sel = 0;
    }
    if new_sel >= len as i32 {
        new_sel = len as i32 - 1;
    }

    col.selected = new_sel as usize;

    // Scroll
    // height approximate
    let (_, h) = app.terminal_size;
    let view_h = h as usize;

    if col.selected >= col.offset + view_h {
        col.offset = col.selected + 1 - view_h;
    } else if col.selected < col.offset {
        col.offset = col.selected;
    }
}

fn enter_directory(app: &mut App, event_tx: &mpsc::Sender<AppEvent>) {
    if app.tree_state.active_columns.is_empty() {
        return;
    }
    let last_idx = app.tree_state.active_columns.len() - 1;
    let col = &app.tree_state.active_columns[last_idx];

    if col.items.is_empty() {
        return;
    }
    let item = &col.items[col.selected];

    if item.is_dir {
        let path = item.path.clone();

        // Check if next column is already this path (avoid dupes if user spams right)
        // Actually we typically replace any existing next columns when navigating.
        // But if we are already "previewing" it?
        // Miller Columns: Right => Push new column.

        // Push new column
        let new_col = load_column(&path);
        app.tree_state.active_columns.push(new_col);
        app.tree_state.focus_col_idx = app.tree_state.active_columns.len() - 1;
    } else {
        // File Open
        let _ = event_tx.try_send(AppEvent::PreviewRequested(0, item.path.clone()));
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
                .push(load_column(&fs.current_path));
        }
        return;
    }

    // Re-load each column
    // We must preserve selection if possible.
    for col in &mut app.tree_state.active_columns {
        let new_col = load_column(&col.path);
        // Restore selection
        if new_col.items.len() > col.selected {
            // Maybe verify name match?
            // For now simple index preservation logic or 0
            col.items = new_col.items;
        } else {
            col.items = new_col.items;
            col.selected = 0;
        }
    }
}

fn load_column(path: &Path) -> TreeColumn {
    let mut items = Vec::new();
    if let Ok(entries) = std::fs::read_dir(path) {
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
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
        selected: 0,
        offset: 0,
    }
}
