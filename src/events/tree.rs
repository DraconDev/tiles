use crate::app::{App, AppEvent};
use crate::state::TreeItem;
use ratatui::style::Color;
use std::path::{Path, PathBuf};
use terma::input::event::{
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use tokio::sync::mpsc;

pub fn handle_tree_events(evt: &Event, app: &mut App, event_tx: &mpsc::Sender<AppEvent>) -> bool {
    if let Event::Key(key) = evt {
        if key.kind != KeyEventKind::Press {
            return false;
        }

        // Handle modifier keys for toggling hidden files
        if key.code == KeyCode::Char('h') && key.modifiers.contains(KeyModifiers::CONTROL) {
            app.tree_state.show_hidden = !app.tree_state.show_hidden;
            // Reload root to reflect changes
            refresh_tree(app);
            return true;
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
            KeyCode::Right | KeyCode::Char('l') => {
                expand_current(app);
                return true;
            }
            KeyCode::Enter => {
                // If File -> Open
                // If Dir -> Toggle Expand? Or just Expand?
                // Standard: Enter on Dir toggles or enters.
                // Let's make Enter Open File, and Expand Dir
                toggle_expand_current(app, event_tx);
                return true;
            }
            KeyCode::Left | KeyCode::Char('h') => {
                collapse_current(app);
                return true;
            }
            _ => {}
        }
    }
    false
}

// Mouse handling for Recursive Tree
pub fn handle_tree_mouse(
    me: &MouseEvent,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
) -> bool {
    let (_, h) = app.terminal_size;

    match me.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // Determine clicked row
            let tree_area_y = 0; // Assuming full screen minus specific offsets handled in render
            let row = me.row.saturating_sub(tree_area_y) as usize;

            // Map row to tree item
            // We need the flattened list of visible items
            let visible_items = flatten_tree_for_hit_testing(app);
            let scroll_offset = app.tree_state.scroll_offset;

            let clicked_row_index = row + scroll_offset;

            if clicked_row_index < visible_items.len() {
                let clicked_item_path = visible_items[clicked_row_index].path.clone();
                let is_dir = visible_items[clicked_row_index].is_dir;

                // Select
                app.tree_state.selected_path = Some(clicked_item_path.clone());

                // Toggle expansion if it's a dir?
                // Or just select behavior?
                // Usually click selects. Double click opens.
                // But for TUI, maybe click also toggles if it's the arrow?
                // Let's implement Click -> Select.
                // If already selected and Dir -> Toggle?

                // Simple version: Just Select.
                // Wait, user asked for opening.
                // "Mosue Open Click"
                if is_dir {
                    toggle_expansion_by_path(app, &clicked_item_path);
                } else {
                    let _ = event_tx.try_send(AppEvent::PreviewRequested(0, clicked_item_path));
                }
                return true;
            }
        }
        MouseEventKind::ScrollDown => {
            app.tree_state.scroll_offset = app.tree_state.scroll_offset.saturating_add(3);
            return true;
        }
        MouseEventKind::ScrollUp => {
            app.tree_state.scroll_offset = app.tree_state.scroll_offset.saturating_sub(3);
            return true;
        }
        _ => {}
    }

    false
}

// Flattens the tree visible state just for hit testing / counting
// Returns Vec<TreeItem> (clones, expensive but needed if we don't have separate view model)
fn flatten_tree_for_hit_testing(app: &App) -> Vec<TreeItem> {
    let mut result = Vec::new();
    for item in &app.tree_state.root_items {
        flatten_item(item, &mut result);
    }
    result
}

fn flatten_item(item: &TreeItem, result: &mut Vec<TreeItem>) {
    result.push(item.clone());
    if item.expanded {
        if let Some(children) = &item.children {
            for child in children {
                flatten_item(child, result);
            }
        }
    }
}

pub fn refresh_tree(app: &mut App) {
    if app.tree_state.root_items.is_empty() {
        if let Some(fs) = app.current_file_state() {
            app.tree_state.root_items = load_children(&fs.current_path, app.tree_state.show_hidden);
            // Default selection to first item
            if !app.tree_state.root_items.is_empty() {
                app.tree_state.selected_path = Some(app.tree_state.root_items[0].path.clone());
            }
        }
    } else {
        // Reload recursively to preserve state
        // This is tricky. Simplified: Just reload root for now?
        // Or implement a deep reload preserving expansion?
        // Let's reload root but carry over expansion state.
        let old_roots = app.tree_state.root_items.clone();
        if let Some(fs) = app.current_file_state() {
            let mut new_roots = load_children(&fs.current_path, app.tree_state.show_hidden);
            restore_expansion(&mut new_roots, &old_roots);
            app.tree_state.root_items = new_roots;
        }
    }
}

fn restore_expansion(new_items: &mut Vec<TreeItem>, old_items: &[TreeItem]) {
    for new_item in new_items.iter_mut() {
        if let Some(old) = old_items.iter().find(|o| o.path == new_item.path) {
            if old.expanded {
                new_item.expanded = true;
                if new_item.is_dir {
                    new_item.children = Some(load_children(&new_item.path, false)); // show_hidden passed? Fix later to propagate.
                                                                                    // Recurse
                    if let Some(new_children) = &mut new_item.children {
                        if let Some(old_children) = &old.children {
                            restore_expansion(new_children, old_children);
                        }
                    }
                }
            }
        }
    }
}

fn load_children(path: &Path, show_hidden: bool) -> Vec<TreeItem> {
    let mut items = Vec::new();
    if let Ok(entries) = std::fs::read_dir(path) {
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
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

            // Check children
            let has_children = if is_dir {
                std::fs::read_dir(&p)
                    .map(|mut rd| {
                        if show_hidden {
                            rd.next().is_some()
                        } else {
                            rd.filter_map(|e| e.ok())
                                .any(|e| !e.file_name().to_string_lossy().starts_with('.'))
                        }
                    })
                    .unwrap_or(false)
            } else {
                false
            };

            items.push(TreeItem {
                path: p,
                name,
                is_dir,
                expanded: false,
                has_children,
                color: if is_dir { Color::Blue } else { Color::White },
                children: None,
            });
        }
    }
    items
}

fn move_msg(app: &mut App, delta: i32) {
    // We need to flatten the visible tree to find current index and move
    let visible = flatten_tree_for_hit_testing(app);
    if visible.is_empty() {
        return;
    }

    let current_idx = if let Some(p) = &app.tree_state.selected_path {
        visible.iter().position(|it| &it.path == p).unwrap_or(0)
    } else {
        0
    };

    let new_idx = (current_idx as i32 + delta).clamp(0, visible.len() as i32 - 1) as usize;
    app.tree_state.selected_path = Some(visible[new_idx].path.clone());

    // Auto-scroll
    let (_, h) = app.terminal_size;
    let view_h = h as usize; // simplified

    if new_idx >= app.tree_state.scroll_offset + view_h {
        app.tree_state.scroll_offset = new_idx + 1 - view_h;
    } else if new_idx < app.tree_state.scroll_offset {
        app.tree_state.scroll_offset = new_idx;
    }
}

fn expand_current(app: &mut App) {
    if let Some(p) = app.tree_state.selected_path.clone() {
        set_expansion(app, &p, true);
    }
}

fn collapse_current(app: &mut App) {
    // If currently expanded, collapse.
    // If leaf or collapsed, select parent.
    if let Some(p) = app.tree_state.selected_path.clone() {
        let is_expanded = is_expanded(app, &p);
        if is_expanded {
            set_expansion(app, &p, false);
        } else {
            // Jump to parent
            if let Some(parent) = p.parent() {
                // Check if parent is in tree (up to root)
                // We assume root is visible.
                app.tree_state.selected_path = Some(parent.to_path_buf());
            }
        }
    }
}

fn toggle_expand_current(app: &mut App, event_tx: &mpsc::Sender<AppEvent>) {
    if let Some(p) = app.tree_state.selected_path.clone() {
        // Find item
        let is_dir = p.is_dir();
        if is_dir {
            toggle_expansion_by_path(app, &p);
        } else {
            let _ = event_tx.try_send(AppEvent::PreviewRequested(0, p));
        }
    }
}

fn toggle_expansion_by_path(app: &mut App, path: &PathBuf) {
    // Helper to find mutable ref
    let should_expand = !is_expanded(app, path);
    set_expansion(app, path, should_expand);
}

fn is_expanded(app: &App, path: &PathBuf) -> bool {
    // Search recursively
    check_expanded_recursive(&app.tree_state.root_items, path)
}

fn check_expanded_recursive(items: &[TreeItem], path: &PathBuf) -> bool {
    for item in items {
        if &item.path == path {
            return item.expanded;
        }
        if let Some(children) = &item.children {
            if check_expanded_recursive(children, path) {
                return true; // Found in children? No, wait.
                             // We return bool if FOUND item's expanded.
                             // Revisit logic.
            }
        }
    }
    false
}

// Better approach for modification: recursive update
fn set_expansion(app: &mut App, path: &PathBuf, expand: bool) {
    update_expansion_recursive(
        &mut app.tree_state.root_items,
        path,
        expand,
        app.tree_state.show_hidden,
    );
}

fn update_expansion_recursive(
    items: &mut Vec<TreeItem>,
    path: &PathBuf,
    expand: bool,
    show_hidden: bool,
) {
    for item in items {
        if &item.path == path {
            item.expanded = expand;
            if expand && item.children.is_none() {
                item.children = Some(load_children(path, show_hidden));
            }
            return;
        }
        if let Some(children) = &mut item.children {
            update_expansion_recursive(children, path, expand, show_hidden);
        }
    }
}
