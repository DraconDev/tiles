use terma::input::event::{Event, KeyCode, KeyModifiers};
use tokio::sync::mpsc;
use std::path::PathBuf;
use std::time::Duration;
use unicode_width::UnicodeWidthStr;
use terma::utils::get_visual_width;

use crate::app::{App, AppEvent, AppMode, CurrentView, SidebarTarget, ContextMenuTarget, FileColumn, CommandAction, DropTarget};
use crate::icons::Icon;
use crate::events::input::delete_word_backwards;

pub fn handle_file_events(evt: &Event, app: &mut App, event_tx: &mpsc::Sender<AppEvent>) -> bool {
    if let Event::Key(key) = evt {
        let has_control = key.modifiers.contains(KeyModifiers::CONTROL);
        let has_alt = key.modifiers.contains(KeyModifiers::ALT);

        match &app.mode {
            AppMode::Normal => {
                // Global Shortcuts
                match key.code {
                    KeyCode::Char('i') | KeyCode::Char('I') if has_control => {
                        let state = crate::modules::introspection::WorldState::capture(app);
                        if let Ok(json) = serde_json::to_string_pretty(&state) {
                            let _ = std::fs::write("introspection.json", json);
                            app.last_action_msg = Some((
                                "World state dumped to introspection.json".to_string(),
                                std::time::Instant::now(),
                            ));
                        }
                        return true;
                    }
                    KeyCode::Enter if has_alt => {
                        app.mode = AppMode::Properties;
                        return true;
                    }
                    KeyCode::Char('h') | KeyCode::Char('H') if has_control => {
                        let idx = app.toggle_hidden();
                        if let Some(fs) = app.panes.get(idx).and_then(|p| p.current_state()) {
                            app.default_show_hidden = fs.show_hidden;
                        }
                        let _ = crate::config::save_state(app);
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(idx));
                        return true;
                    }
                    KeyCode::Backspace if has_control => {
                        let idx = app.toggle_hidden();
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(idx));
                        return true;
                    }
                    KeyCode::Char('g') | KeyCode::Char('G') if has_control => {
                        app.mode = AppMode::Settings;
                        app.settings_scroll = 0;
                        return true;
                    }
                    KeyCode::Char('n')
                    | KeyCode::Char('N')
                    | KeyCode::Char('o')
                    | KeyCode::Char('O')
                        if has_control =>
                    {
                        if let Some(fs) = app.current_file_state() {
                            let _ = event_tx.try_send(AppEvent::SpawnTerminal {
                                path: fs.current_path.clone(),
                                new_tab: true,
                                remote: fs.remote_session.clone(),
                                command: None,
                            });
                        }
                        return true;
                    }
                    KeyCode::Char('t') | KeyCode::Char('T') if has_control => {
                        if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                            if let Some(fs) = pane.current_state() {
                                let new_fs = fs.clone();
                                pane.open_tab(new_fs);
                                let _ =
                                    event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                            }
                        }
                        return true;
                    }
                    KeyCode::Left if has_alt => {
                        app.resize_sidebar(-2);
                        return true;
                    }
                    KeyCode::Right if has_alt => {
                        app.resize_sidebar(2);
                        return true;
                    }
                    KeyCode::Char(' ') if has_control => {
                        app.input.clear();
                        app.mode = AppMode::CommandPalette;
                        crate::event_helpers::update_commands(app);
                        return true;
                    }
                    KeyCode::Left if has_control => {
                        if app.sidebar_focus {
                            app.resize_sidebar(-2);
                        } else {
                            app.move_to_other_pane();
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                        }
                        return true;
                    }
                    KeyCode::Right if has_control => {
                        if app.sidebar_focus {
                            app.resize_sidebar(2);
                        } else {
                            app.move_to_other_pane();
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                        }
                        return true;
                    }
                    _ => {}
                }

                // Standard Navigation
                if key.code == KeyCode::Esc {
                    if let Some(fs) = app.current_file_state_mut() {
                        fs.selection.clear_multi();
                        fs.selection.anchor = None;
                        if !fs.search_filter.is_empty() {
                            fs.search_filter.clear();
                            fs.selection.selected = Some(0);
                            *fs.table_state.offset_mut() = 0;
                            let _ = event_tx
                                .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                        }
                    }
                    return true;
                }

                match key.code {
                    KeyCode::Char('c') if has_control => {
                        if let Some(fs) = app.current_file_state() {
                            if let Some(idx) = fs.selection.selected {
                                if let Some(path) = fs.files.get(idx) {
                                    app.clipboard =
                                        Some((path.clone(), crate::app::ClipboardOp::Copy));
                                }
                            }
                        }
                        return true;
                    }
                    KeyCode::Char('x') if has_control => {
                        if let Some(fs) = app.current_file_state() {
                            if let Some(idx) = fs.selection.selected {
                                if let Some(path) = fs.files.get(idx) {
                                    app.clipboard =
                                        Some((path.clone(), crate::app::ClipboardOp::Cut));
                                }
                            }
                        }
                        return true;
                    }
                    KeyCode::Char('v') if has_control => {
                        if let Some((src, op)) = app.clipboard.clone() {
                            if let Some(fs) = app.current_file_state() {
                                let dest = fs.current_path.join(
                                    src.file_name()
                                        .unwrap_or_else(|| std::ffi::OsStr::new("root")),
                                );
                                match op {
                                    crate::app::ClipboardOp::Copy => {
                                        let _ = event_tx.try_send(AppEvent::Copy(src, dest));
                                    }
                                    crate::app::ClipboardOp::Cut => {
                                        let _ = event_tx.try_send(AppEvent::Rename(src, dest));
                                        app.clipboard = None;
                                    }
                                }
                            }
                        }
                        return true;
                    }
                    KeyCode::Char('a') if has_control => {
                        if let Some(fs) = app.current_file_state_mut() {
                            fs.selection.select_all(fs.files.len());
                        }
                        return true;
                    }
                    KeyCode::Char('z') if has_control => {
                        if let Some(action) = app.undo_stack.pop() {
                            match action.clone() {
                                crate::app::UndoAction::Rename(old, new) | crate::app::UndoAction::Move(old, new) => {
                                    let _ = std::fs::rename(&old, &new);
                                    app.redo_stack.push(action);
                                }
                                crate::app::UndoAction::Copy(src, dest) => {
                                    let _ = if dest.is_dir() {
                                        std::fs::remove_dir_all(&dest)
                                    } else {
                                        std::fs::remove_file(&dest)
                                    };
                                    app.redo_stack.push(crate::app::UndoAction::Copy(src, dest));
                                }
                                _ => {}
                            }
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                        } else if let Some(fs) = app.current_file_state_mut() {
                            if !fs.search_filter.is_empty() {
                                fs.search_filter.clear();
                                let _ = event_tx
                                    .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                            }
                        }
                        return true;
                    }
                    KeyCode::Char('y') if has_control => {
                        if let Some(action) = app.redo_stack.pop() {
                            match action.clone() {
                                crate::app::UndoAction::Rename(old, new) | crate::app::UndoAction::Move(old, new) => {
                                    let _ = std::fs::rename(&old, &new);
                                    app.undo_stack.push(action);
                                }
                                crate::app::UndoAction::Copy(src, dest) => {
                                    let _ = crate::modules::files::copy_recursive(&src, &dest);
                                    app.undo_stack.push(action);
                                }
                                _ => {}
                            }
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                        }
                        return true;
                    }
                    KeyCode::Char('f') if has_control => {
                        app.mode = AppMode::Search;
                        return true;
                    }
                    KeyCode::Insert => {
                        let mut should_save = false;
                        if let Some(fs) = app.current_file_state_mut() {
                            if let Some(idx) = fs.selection.selected {
                                fs.selection.toggle(idx);
                                should_save = true;
                                // Move down after toggle
                                if idx < fs.files.len().saturating_sub(1) {
                                    let next_idx = idx + 1;
                                    fs.selection.selected = Some(next_idx);
                                    fs.selection.anchor = Some(next_idx);
                                    fs.table_state.select(Some(next_idx));
                                    if next_idx >= fs.table_state.offset() + fs.view_height {
                                        *fs.table_state.offset_mut() = next_idx.saturating_sub(fs.view_height - 1);
                                    }
                                }
                            }
                        }
                        if should_save {
                            let _ = crate::config::save_state(app);
                        }
                        return true;
                    }
                    KeyCode::Char(' ') => {
                        handle_space_key(app, event_tx);
                        return true;
                    }
                    KeyCode::Up => {
                        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
                        app.move_up(shift);
                        return true;
                    }
                    KeyCode::Down => {
                        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
                        app.move_down(shift);
                        return true;
                    }
                    KeyCode::Left => {
                        if key.modifiers.contains(KeyModifiers::SHIFT) && !app.sidebar_focus {
                            handle_quick_copy(app, event_tx, true);
                            return true;
                        }
                        if app.panes.len() > 1 && app.focused_pane_index > 0 {
                            app.focused_pane_index -= 1;
                        } else {
                            app.sidebar_focus = true;
                        }
                        return true;
                    }
                    KeyCode::Right => {
                        if key.modifiers.contains(KeyModifiers::SHIFT) && !app.sidebar_focus {
                            handle_quick_copy(app, event_tx, false);
                            return true;
                        }
                        if app.sidebar_focus {
                            app.sidebar_focus = false;
                        } else if app.panes.len() > 1
                            && app.focused_pane_index < app.panes.len() - 1
                        {
                            app.focused_pane_index += 1;
                        }
                        return true;
                    }
                    KeyCode::Enter => {
                        handle_enter_key(app, event_tx);
                        return true;
                    }
                    KeyCode::F(2) => {
                         app.selection_mode = !app.selection_mode;
                         if !app.selection_mode {
                             if let Some(fs) = app.current_file_state_mut() {
                                 fs.selection.clear_multi();
                             }
                         }
                        return true;
                    }
                    KeyCode::F(6) => {
                        handle_rename_shortcut(app);
                        return true;
                    }
                    KeyCode::Delete => {
                        handle_delete_key(app, event_tx);
                        return true;
                    }
                    KeyCode::Char('~') => {
                        if let Some(fs) = app.current_file_state_mut() {
                            if let Some(home) = dirs::home_dir() {
                                fs.current_path = home.clone();
                                fs.selection.selected = Some(0);
                                fs.selection.anchor = Some(0);
                                fs.selection.clear_multi();
                                *fs.table_state.offset_mut() = 0;
                                crate::event_helpers::push_history(fs, home);
                                let _ = event_tx
                                    .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                return true;
                            }
                        }
                        return false;
                    }
                    KeyCode::Char('r') if key.modifiers.is_empty() => {
                        handle_rename_shortcut(app);
                        return true;
                    }
                    KeyCode::Char(c) if key.modifiers.is_empty() => {
                        if (c as u32) < 32 || c == '\x7f' || c == '\x1b' {
                            return false;
                        }
                        
                        let is_sidebar = app.sidebar_focus;
                        if let Some(fs) = app.current_file_state_mut() {
                            fs.search_filter.push(c);
                            if !is_sidebar {
                                fs.selection.selected = Some(0);
                                fs.selection.anchor = Some(0);
                                *fs.table_state.offset_mut() = 0;
                            } else {
                                app.sidebar_index = 0;
                            }
                            let _ = event_tx
                                .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                        }
                        return true;
                    }
                    KeyCode::Backspace if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        let mut handled_search = false;
                        let is_sidebar = app.sidebar_focus;
                        if let Some(fs) = app.current_file_state_mut() {
                            if !fs.search_filter.is_empty() {
                                fs.search_filter.pop();
                                if !is_sidebar {
                                    fs.selection.selected = Some(0);
                                    fs.selection.anchor = Some(0);
                                    *fs.table_state.offset_mut() = 0;
                                } else {
                                    app.sidebar_index = 0;
                                }
                                let _ = event_tx
                                    .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                handled_search = true;
                            }
                        }

                        if !handled_search {
                            crate::event_helpers::navigate_up(app);
                            let _ = event_tx
                                .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                        }
                        return true;
                    }
                    KeyCode::Backspace if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) => {
                        let is_sidebar = app.sidebar_focus;
                        if let Some(fs) = app.current_file_state_mut() {
                            delete_word_backwards(&mut fs.search_filter);
                            if !is_sidebar {
                                fs.selection.selected = Some(0);
                                *fs.table_state.offset_mut() = 0;
                            } else {
                                app.sidebar_index = 0;
                            }
                            let _ = event_tx
                                .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                        }
                        return true;
                    }
                    KeyCode::Char('w') if has_control => {
                        let is_sidebar = app.sidebar_focus;
                        if let Some(fs) = app.current_file_state_mut() {
                            delete_word_backwards(&mut fs.search_filter);
                            if !is_sidebar {
                                fs.selection.selected = Some(0);
                                *fs.table_state.offset_mut() = 0;
                            } else {
                                app.sidebar_index = 0;
                            }
                            let _ = event_tx
                                .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                        }
                        return true;
                    }
                    KeyCode::Char('u') if has_control => {
                        let is_sidebar = app.sidebar_focus;
                        if let Some(fs) = app.current_file_state_mut() {
                            fs.search_filter.clear();
                            if !is_sidebar {
                                fs.selection.selected = Some(0);
                                fs.selection.anchor = Some(0);
                                *fs.table_state.offset_mut() = 0;
                            } else {
                                app.sidebar_index = 0;
                            }
                            let _ = event_tx
                                .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                        }
                        return true;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    false
}

fn handle_space_key(app: &mut App, event_tx: &mpsc::Sender<AppEvent>) {
    if let Some(fs) = app.current_file_state_mut() {
        if fs.selection.selected.is_none() && !fs.files.is_empty() {
            fs.selection.selected = Some(0);
            fs.table_state.select(Some(0));
            fs.selection.anchor = Some(0);
        }

        if let Some(idx) = fs.selection.selected {
            if let Some(path) = fs.files.get(idx).cloned() {
                let is_dir = path.is_dir();
                if is_dir {
                    app.mode = AppMode::Properties;
                } else {
                    let mut target_pane = app.focused_pane_index;
                    let will_go_single = !app.view_prefs.editor.is_split_mode || (app.is_split_mode && {
                        let other_idx = if app.focused_pane_index == 0 { 1 } else { 0 };
                        app.panes.get(other_idx).map(|p| p.preview.is_none()).unwrap_or(true)
                    });

                    if will_go_single {
                        target_pane = 0;
                    }

                    let _ = event_tx.try_send(AppEvent::PreviewRequested(target_pane, path));
                    app.save_current_view_prefs();
                    app.current_view = CurrentView::Editor;
                    app.load_view_prefs(CurrentView::Editor);

                    if app.is_split_mode {
                        let other_idx = if app.focused_pane_index == 0 { 1 } else { 0 };
                        if let Some(other_pane) = app.panes.get(other_idx) {
                            if other_pane.preview.is_none() {
                                app.apply_split_mode(false);
                                app.save_current_view_prefs();
                            }
                        }
                    }

                    if app.panes.len() == 1 {
                        app.focused_pane_index = 0;
                    }
                    app.sidebar_focus = false;
                }
            }
        }
    }
}

fn handle_enter_key(app: &mut App, event_tx: &mpsc::Sender<AppEvent>) {
    if app.sidebar_focus {
        let target_opt = app.sidebar_bounds.iter().find(|b| b.index == app.sidebar_index).map(|b| b.target.clone());
        if let Some(target) = target_opt {
            match target {
                SidebarTarget::Favorite(path) => {
                     if let Some(fs) = app.current_file_state_mut() {
                         fs.current_path = path.clone();
                         fs.selection.selected = Some(0);
                         fs.selection.anchor = Some(0);
                         fs.selection.clear_multi();
                         crate::event_helpers::push_history(fs, path.clone());
                         let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                         app.sidebar_focus = false;
                     }
                }
                SidebarTarget::Remote(idx) => {
                    let _ = event_tx.try_send(AppEvent::ConnectToRemote(app.focused_pane_index, idx));
                }
                SidebarTarget::Project(path) => {
                    if path.is_dir() {
                        if app.expanded_folders.contains(&path) { app.expanded_folders.remove(&path); }
                        else { app.expanded_folders.insert(path); }
                    } else {
                        let _ = event_tx.try_send(AppEvent::PreviewRequested(app.focused_pane_index, path));
                        app.sidebar_focus = false;
                    }
                }
                SidebarTarget::Disk(name) => {
                    if let Some(disk) = app.system_state.disks.iter().find(|d| d.name == name) {
                        if disk.is_mounted {
                            let mp = PathBuf::from(&disk.name);
                             if let Some(fs) = app.current_file_state_mut() {
                                 fs.current_path = mp.clone();
                                 fs.selection.selected = Some(0);
                                 fs.selection.anchor = Some(0);
                                 fs.selection.clear_multi();
                                 crate::event_helpers::push_history(fs, mp.clone());
                                 let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                 app.sidebar_focus = false;
                             }
                        } else {
                            let _ = event_tx.try_send(AppEvent::MountDisk(name.clone()));
                        }
                    }
                }
                _ => {}
            }
        }
        return;
    }

    let mut navigate_to = None;
     if let Some(fs) = app.current_file_state() {
         if let Some(idx) = fs.selection.selected {
             if let Some(path) = fs.files.get(idx) {
                 if path.is_dir() { navigate_to = Some(path.clone()); }
                 else { terma::utils::spawn_detached("xdg-open", vec![path.to_string_lossy().to_string()]); }
             }
         }
     }
    if let Some(p) = navigate_to {
         if let Some(fs) = app.current_file_state() {
             let path = fs.current_path.clone();
             let idx = fs.selection.selected.unwrap_or(0);
             app.folder_selections.insert(path, idx);
         }

         if let Some(fs) = app.current_file_state_mut() {
             fs.current_path = p.clone();
             fs.selection.selected = Some(0);
             fs.selection.anchor = Some(0);
             fs.selection.clear_multi();
             fs.search_filter.clear();
             *fs.table_state.offset_mut() = 0;
             crate::event_helpers::push_history(fs, p);
             let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
         }
    }
}

fn handle_rename_shortcut(app: &mut App) {
    let mut to_rename = None;
    if let Some(fs) = app.current_file_state() {
        if let Some(p) = fs.selection.selected.and_then(|idx| fs.files.get(idx)) {
            to_rename = Some(p.file_name().unwrap_or_else(|| std::ffi::OsStr::new("root")).to_string_lossy().to_string());
        }
    }
    if let Some(name) = to_rename {
        app.mode = AppMode::Rename;
        app.input.set_value(name.clone());
        if let Some(idx) = name.rfind('.') {
            app.input.cursor_position = if idx > 0 { idx } else { name.len() };
        } else {
            app.input.cursor_position = name.len();
        }
        app.rename_selected = true;
    }
}

fn handle_delete_key(app: &mut App, event_tx: &mpsc::Sender<AppEvent>) {
    if let Some(fs) = app.current_file_state() {
        if fs.selection.selected.is_some() {
            if app.confirm_delete { app.mode = AppMode::Delete; }
            else {
                let mut paths = Vec::new();
                if !fs.selection.is_empty() {
                    for &idx in fs.selection.multi_selected_indices() {
                        if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); }
                    }
                } else if let Some(idx) = fs.selection.selected {
                    if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); }
                }
                for p in paths { let _ = event_tx.try_send(AppEvent::Delete(p)); }
            }
        }
    }
}

fn handle_quick_copy(app: &mut App, event_tx: &mpsc::Sender<AppEvent>, _to_left: bool) {
    let other_pane_idx = if app.focused_pane_index == 0 { 1 } else { 0 };
    if let Some(dest_path) = app.panes.get(other_pane_idx).and_then(|p| p.current_state()).map(|fs| fs.current_path.clone()) {
         if let Some(fs) = app.current_file_state() {
             let mut paths = Vec::new();
             if !fs.selection.is_empty() {
                 for &idx in fs.selection.multi_selected_indices() {
                     if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); }
                 }
             } else if let Some(idx) = fs.selection.selected {
                 if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); }
             }
             for p in paths {
                 let dest = dest_path.join(p.file_name().unwrap_or_else(|| std::ffi::OsStr::new("root")));
                 let _ = event_tx.try_send(AppEvent::Copy(p, dest));
             }
         }
     }
}
