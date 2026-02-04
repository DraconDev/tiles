use crate::app::{App, AppEvent};
use crate::state::{TreeItem, TreeState};
use ratatui::style::Color;
use std::path::{Path, PathBuf};
use terma::input::event::{Event, KeyCode, KeyEventKind};
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
                move_selection(app, 1);
                return true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                move_selection(app, -1);
                return true;
            }
            KeyCode::Right | KeyCode::Char('l') => {
                expand_current(app);
                return true;
            }
            KeyCode::Left | KeyCode::Char('h') => {
                collapse_current(app);
                return true;
            }
            KeyCode::Enter => {
                open_current(app, event_tx);
                return true;
            }
            _ => {}
        }
    }
    false
}

fn move_selection(app: &mut App, delta: i32) {
    let len = app.tree_state.flat_items.len();
    if len == 0 {
        return;
    }
    let mut new_idx = app.tree_state.selected as i32 + delta;
    if new_idx < 0 {
        new_idx = 0;
    } else if new_idx >= len as i32 {
        new_idx = len as i32 - 1;
    }
    app.tree_state.selected = new_idx as usize;

    // Scroll Logic
    let height = app.tree_state.view_height.saturating_sub(2); // Reduced padding
    if app.tree_state.selected >= app.tree_state.offset + height {
        app.tree_state.offset = app.tree_state.selected + 1 - height;
    } else if app.tree_state.selected < app.tree_state.offset {
        app.tree_state.offset = app.tree_state.selected;
    }
}

fn expand_current(app: &mut App) {
    if app.tree_state.flat_items.is_empty() {
        return;
    }
    let item = &app.tree_state.flat_items[app.tree_state.selected];
    if item.is_dir {
        if !item.expanded {
            app.tree_state.expanded_paths.insert(item.path.clone());
            refresh_tree(app);
        }
    }
}

fn collapse_current(app: &mut App) {
    if app.tree_state.flat_items.is_empty() {
        return;
    }
    let item = app.tree_state.flat_items[app.tree_state.selected].clone();
    if item.is_dir && item.expanded {
        app.tree_state.expanded_paths.remove(&item.path);
        refresh_tree(app);
    } else {
        // Jump to parent logic would go here (search backwards for depth - 1)
        if item.depth > 0 {
            for i in (0..app.tree_state.selected).rev() {
                if app.tree_state.flat_items[i].depth == item.depth - 1 {
                    app.tree_state.selected = i;
                    // Ensure collapsed
                    if app.tree_state.flat_items[i].expanded {
                        app.tree_state
                            .expanded_paths
                            .remove(&app.tree_state.flat_items[i].path.clone());
                        refresh_tree(app);
                    }
                    return;
                }
            }
        }
    }
}

fn open_current(app: &mut App, event_tx: &mpsc::Sender<AppEvent>) {
    if app.tree_state.flat_items.is_empty() {
        return;
    }
    let item = &app.tree_state.flat_items[app.tree_state.selected];
    if !item.is_dir {
        let _ = event_tx.try_send(AppEvent::CreateFile(item.path.clone())); // Reuse create file? No, OPEN.
                                                                            // Trigger file open logic. For now, maybe just preview?
                                                                            // We can use AppEvent::PreviewRequested or similar.
                                                                            // Or switch to Editor.
        let _ = event_tx.try_send(AppEvent::PreviewRequested(0, item.path.clone()));
    } else {
        expand_current(app);
    }
}

// Ensure this function is public or accessible to refresh logic
pub fn refresh_tree(app: &mut App) {
    // Rebuild flattened list starting from root (e.g. focused pane path or Home)
    // For now, let's assume root is `app.current_file_state().current_path`
    // OR we should maintain a persistent root for tree view.
    // Let's use focused pane current path as ROOT.
    if let Some(fs) = app.current_file_state() {
        let root = fs.current_path.clone(); // Clone to avoid borrow issues
        let mut new_items = Vec::new();
        build_tree_recursive(&root, 0, &app.tree_state.expanded_paths, &mut new_items);
        app.tree_state.flat_items = new_items;
    }
}

fn build_tree_recursive(
    path: &Path,
    depth: usize,
    expanded: &std::collections::HashSet<PathBuf>,
    acc: &mut Vec<TreeItem>,
) {
    // Add self? Actually tree usually lists CHILDREN of root.
    // If this is called for root, we might want to list root content.
    // Let's assume this function adds content OF path.

    // Read dir
    if let Ok(entries) = std::fs::read_dir(path) {
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        // Sort: Folders first
        entries.sort_by(|a, b| {
            let ad = a.path().is_dir();
            let bd = b.path().is_dir();
            if ad != bd {
                bd.cmp(&ad) // True (dir) < False (file) in sorting? No, True is Greater. We want Dir First (Less?)
                            // bool ord: false < true.
                            // We want true first. So b.cmp(a).
            } else {
                a.file_name().cmp(&b.file_name())
            }
        });

        for entry in entries {
            let p = entry.path();
            let is_dir = p.is_dir();
            let is_expanded = expanded.contains(&p);
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            acc.push(TreeItem {
                path: p.clone(),
                name,
                depth,
                is_dir,
                expanded: is_expanded,
                color: if is_dir { Color::Blue } else { Color::White }, // basic
            });

            if is_dir && is_expanded {
                build_tree_recursive(&p, depth + 1, expanded, acc);
            }
        }
    }
}
