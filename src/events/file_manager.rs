use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use terma::input::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use tokio::sync::mpsc;

use crate::app::{
    App, AppEvent, AppMode, ContextMenuTarget, CurrentView, SidebarTarget, UndoAction,
};
use crate::events::input::delete_word_backwards;
use crate::state::DropTarget;

pub fn handle_file_events(evt: &Event, app: &mut App, event_tx: &mpsc::Sender<AppEvent>) -> bool {
    if let Event::Key(key) = evt {
        let has_control = key.modifiers.contains(KeyModifiers::CONTROL);
        let has_alt = key.modifiers.contains(KeyModifiers::ALT);

        match &app.mode {
            AppMode::Normal => {
                // Global Shortcuts
                match key.code {
                    KeyCode::Char('i') | KeyCode::Char('I') if has_control => {
                        app.mode = AppMode::Properties;
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
                    KeyCode::Char('n') | KeyCode::Char('N') if has_control => {
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
                    KeyCode::Char('k') | KeyCode::Char('K') if has_control => {
                        if let Some(fs) = app.current_file_state() {
                            let _ = event_tx.try_send(AppEvent::SpawnTerminal {
                                path: fs.current_path.clone(),
                                new_tab: false,
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
                                let _ = event_tx
                                    .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
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
                    for pane in &mut app.panes {
                        pane.preview = None;
                    }

                    if let Some(fs) = app.current_file_state_mut() {
                        fs.selection.clear_multi();
                        fs.selection.anchor = None;
                        if !fs.search_filter.is_empty() {
                            fs.search_filter.clear();
                            fs.selection.selected = Some(0);
                            *fs.table_state.offset_mut() = 0;
                            let _ =
                                event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                        }
                    }
                    return true;
                }

                match key.code {
                    KeyCode::F(5) => {
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                        return true;
                    }
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
                    KeyCode::Char('z')
                        if has_control && !key.modifiers.contains(KeyModifiers::SHIFT) =>
                    {
                        if let Some(action) = app.undo_stack.pop() {
                            match action.clone() {
                                UndoAction::Rename(old, new) | UndoAction::Move(old, new) => {
                                    let _ = std::fs::rename(&old, &new);
                                    app.redo_stack.push(action);
                                }
                                UndoAction::Copy(src, dest) => {
                                    let _ = if dest.is_dir() {
                                        std::fs::remove_dir_all(&dest)
                                    } else {
                                        std::fs::remove_file(&dest)
                                    };
                                    app.redo_stack.push(UndoAction::Copy(src, dest));
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
                                UndoAction::Rename(old, new) | UndoAction::Move(old, new) => {
                                    let _ = std::fs::rename(&old, &new);
                                    app.undo_stack.push(action);
                                }
                                UndoAction::Copy(src, dest) => {
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
                    KeyCode::Char('z')
                        if has_control && key.modifiers.contains(KeyModifiers::SHIFT) =>
                    {
                        if let Some(action) = app.redo_stack.pop() {
                            match action.clone() {
                                UndoAction::Rename(old, new) | UndoAction::Move(old, new) => {
                                    let _ = std::fs::rename(&old, &new);
                                    app.undo_stack.push(action);
                                }
                                UndoAction::Copy(src, dest) => {
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
                    KeyCode::Char('Z') if has_control => {
                        if let Some(action) = app.redo_stack.pop() {
                            match action.clone() {
                                UndoAction::Rename(old, new) | UndoAction::Move(old, new) => {
                                    let _ = std::fs::rename(&old, &new);
                                    app.undo_stack.push(action);
                                }
                                UndoAction::Copy(src, dest) => {
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
                                        *fs.table_state.offset_mut() =
                                            next_idx.saturating_sub(fs.view_height - 1);
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
                        if has_alt && app.sidebar_focus {
                            // Reorder Favorites: Find actual starred index from sidebar_bounds
                            if let Some(bound) = app
                                .sidebar_bounds
                                .iter()
                                .find(|b| b.index == app.sidebar_index)
                            {
                                if let SidebarTarget::Favorite(ref path) = bound.target {
                                    if let Some(starred_idx) =
                                        app.starred.iter().position(|p| p == path)
                                    {
                                        if starred_idx > 0 {
                                            app.starred.swap(starred_idx, starred_idx - 1);
                                            app.sidebar_index = app.sidebar_index.saturating_sub(1);
                                            let _ = crate::config::save_state(app);
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(
                                                app.focused_pane_index,
                                            ));
                                        }
                                    }
                                }
                            }
                            return true;
                        }
                        app.move_up(shift);
                        return true;
                    }
                    KeyCode::Down => {
                        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
                        if has_alt && app.sidebar_focus {
                            // Reorder Favorites: Find actual starred index from sidebar_bounds
                            if let Some(bound) = app
                                .sidebar_bounds
                                .iter()
                                .find(|b| b.index == app.sidebar_index)
                            {
                                if let SidebarTarget::Favorite(ref path) = bound.target {
                                    if let Some(starred_idx) =
                                        app.starred.iter().position(|p| p == path)
                                    {
                                        if starred_idx < app.starred.len().saturating_sub(1) {
                                            app.starred.swap(starred_idx, starred_idx + 1);
                                            app.sidebar_index += 1;
                                            let _ = crate::config::save_state(app);
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(
                                                app.focused_pane_index,
                                            ));
                                        }
                                    }
                                }
                            }
                            return true;
                        }
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
                        handle_rename_shortcut(app);
                        return true;
                    }
                    KeyCode::F(3) => {
                        app.selection_mode = !app.selection_mode;
                        if !app.selection_mode {
                            if let Some(fs) = app.current_file_state_mut() {
                                fs.selection.clear_multi();
                            }
                        }
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
                    KeyCode::Char(c)
                        if !key.modifiers.intersects(
                            KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                        ) =>
                    {
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
                            let _ =
                                event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
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
                            let _ =
                                event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                        }
                        return true;
                    }
                    KeyCode::Backspace
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            || key.modifiers.contains(KeyModifiers::ALT) =>
                    {
                        let is_sidebar = app.sidebar_focus;
                        if let Some(fs) = app.current_file_state_mut() {
                            delete_word_backwards(&mut fs.search_filter);
                            if !is_sidebar {
                                fs.selection.selected = Some(0);
                                *fs.table_state.offset_mut() = 0;
                            } else {
                                app.sidebar_index = 0;
                            }
                            let _ =
                                event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
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
                            let _ =
                                event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
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
                            let _ =
                                event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
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

pub fn handle_file_mouse(
    me: &terma::input::event::MouseEvent,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
    _panes_needing_refresh: &mut HashSet<usize>,
) -> bool {
    let column = me.column;
    let row = me.row;
    let (w, _h) = app.terminal_size;
    let sw = app.sidebar_width();

    if let MouseEventKind::Down(_) = me.kind {
        if column >= sw {
            let cw = w.saturating_sub(sw);
            let pc = app.panes.len();
            if pc == 0 {
                return false;
            }
            let pw = cw / pc as u16;
            if pw == 0 {
                return false;
            }
            let cp = (column.saturating_sub(sw) / pw) as usize;
            if cp < pc {
                app.focused_pane_index = cp;
                app.sidebar_focus = false;
            }
        }
    }

    match me.kind {
        MouseEventKind::Down(button) => {
            // 1. Breadcrumb Click
            if let Some(fs) = app.current_file_state_mut() {
                if let Some((_, path)) = fs
                    .breadcrumb_bounds
                    .iter()
                    .find(|(r, _)| r.contains(ratatui::layout::Position { x: column, y: row }))
                {
                    let target_path = path.clone();
                    let current_path = fs.current_path.clone();

                    // Smart Selection
                    if current_path.starts_with(&target_path) && current_path != target_path {
                        if let Ok(prefix) = current_path.strip_prefix(&target_path) {
                            if let Some(component) = prefix.components().next() {
                                let child_name = component.as_os_str();
                                fs.pending_select_path = Some(target_path.join(child_name));
                            }
                        }
                    }

                    fs.current_path = target_path.clone();
                    fs.selection.clear();
                    fs.search_filter.clear();
                    *fs.table_state.offset_mut() = 0;
                    crate::event_helpers::push_history(fs, target_path);
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                    app.sidebar_focus = false;
                    return true;
                }
            }

            // 2. Sorting (Header Clicks)
            if row == 1 || row == 2 {
                if let MouseEventKind::Down(MouseButton::Left) = me.kind {
                    if column >= sw {
                        let cw = w.saturating_sub(sw);
                        let pc = app.panes.len();
                        if pc == 0 {
                            return false;
                        }
                        let pw = cw / pc as u16;
                        if pw == 0 {
                            return false;
                        }
                        let cp = (column.saturating_sub(sw) / pw) as usize;
                        if let Some(fs) = app.panes.get_mut(cp).and_then(|p| p.current_state_mut())
                        {
                            for (r, col) in &fs.column_bounds {
                                if column >= r.x
                                    && column < r.x.saturating_add(r.width).saturating_add(1)
                                {
                                    if fs.sort_column == *col {
                                        fs.sort_ascending = !fs.sort_ascending;
                                    } else {
                                        fs.sort_column = *col;
                                        fs.sort_ascending = true;
                                    }
                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(cp));
                                    return true;
                                }
                            }
                        }
                    }
                }
            }

            // 3. File Row Interaction
            if row >= 3 {
                let idx = crate::event_helpers::fs_mouse_index(row, app);
                let mut sp = None;
                let mut is_dir = false;
                let is_shift = me.modifiers.contains(KeyModifiers::SHIFT)
                    || me.modifiers.contains(KeyModifiers::ALT);
                let is_ctrl = me.modifiers.contains(KeyModifiers::CONTROL);
                let has_mods = is_shift || is_ctrl;
                app.prevent_mouse_up_selection_cleanup = has_mods;

                let sel_mode = app.selection_mode;
                if let Some(fs) = app.current_file_state_mut() {
                    if idx < fs.files.len() {
                        let is_divider = fs.files[idx].to_string_lossy() == "__DIVIDER__";
                        if is_divider {
                            return true;
                        }

                        if button == MouseButton::Left {
                            fs.selection.handle_click(
                                idx,
                                is_shift,
                                is_ctrl,
                                sel_mode && !is_shift,
                            );
                            fs.table_state.select(fs.selection.selected);
                        }

                        let p = fs.files[idx].clone();
                        is_dir = fs.metadata.get(&p).map(|m| m.is_dir).unwrap_or(false);
                        sp = Some(p);
                    } else if button == MouseButton::Left && !has_mods {
                        fs.selection.clear();
                        fs.table_state.select(None);
                    } else if button == MouseButton::Right {
                        let target = ContextMenuTarget::EmptySpace;
                        let actions = crate::event_helpers::get_context_menu_actions(&target, app);
                        app.mode = AppMode::ContextMenu {
                            x: column,
                            y: row,
                            target,
                            actions,
                            selected_index: None,
                        };
                        return true;
                    }
                }

                if let Some(path) = sp {
                    if button == MouseButton::Right {
                        let target = if is_dir {
                            ContextMenuTarget::Folder(idx)
                        } else {
                            ContextMenuTarget::File(idx)
                        };
                        let actions = crate::event_helpers::get_context_menu_actions(&target, app);
                        app.mode = AppMode::ContextMenu {
                            x: column,
                            y: row,
                            target,
                            actions,
                            selected_index: None,
                        };
                        return true;
                    }
                    if button == MouseButton::Middle {
                        if is_dir {
                            if let Some(p) = app.panes.get_mut(app.focused_pane_index) {
                                if let Some(fs) = p.current_state() {
                                    let mut nfs = fs.clone();
                                    nfs.current_path = path.clone();
                                    nfs.selection.clear();
                                    crate::event_helpers::push_history(&mut nfs, path);
                                    p.open_tab(nfs);
                                    let _ = event_tx
                                        .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                }
                            }
                        } else {
                            let _ = event_tx.try_send(AppEvent::PreviewRequested(
                                if app.focused_pane_index == 0 { 1 } else { 0 },
                                path,
                            ));
                        }
                        return true;
                    }
                    app.drag_source = Some(path.clone());
                    app.drag_start_pos = Some((column, row));

                    // Double Click
                    if button == MouseButton::Left
                        && app.mouse_last_click.elapsed() < Duration::from_millis(500)
                        && app.mouse_click_pos == (column, row)
                    {
                        if path.is_dir() {
                            if let Some(fs) = app.current_file_state_mut() {
                                fs.current_path = path.clone();
                                fs.selection.clear();
                                crate::event_helpers::push_history(fs, path);
                                let _ = event_tx
                                    .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                            }
                        } else {
                            terma::utils::spawn_detached(
                                "xdg-open",
                                vec![path.to_string_lossy().to_string()],
                            );
                        }
                    }
                    app.mouse_last_click = std::time::Instant::now();
                    app.mouse_click_pos = (column, row);
                }
            }

            if button == MouseButton::Middle {
                if let Some(text) = terma::utils::get_primary_selection_text() {
                    if let Some(fs) = app.current_file_state_mut() {
                        fs.search_filter.push_str(&text);
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                    }
                }
            }
            true
        }
        MouseEventKind::Up(_) => {
            if app.is_dragging {
                // Drop Logic
                if let Some(target) = app.hovered_drop_target.take() {
                    if let DropTarget::Folder(target_path) = target {
                        if let Some(source_path) = app.drag_source.take() {
                            if source_path != target_path {
                                app.mode = AppMode::DragDropMenu {
                                    sources: vec![source_path],
                                    target: target_path,
                                };
                            }
                        }
                    }
                }
                app.is_dragging = false;
            }
            let sel_mode = app.selection_mode;
            if row >= 3
                && !app.prevent_mouse_up_selection_cleanup
                && !sel_mode
                && !me.modifiers.contains(KeyModifiers::SHIFT)
            {
                let idx = crate::event_helpers::fs_mouse_index(row, app);
                if let Some(fs) = app.current_file_state_mut() {
                    if idx < fs.files.len() {
                        fs.selection.clear();
                        fs.selection.selected = Some(idx);
                        fs.table_state.select(Some(idx));
                    }
                }
            }
            app.drag_start_pos = None;
            app.drag_source = None;
            app.hovered_drop_target = None;
            true
        }
        MouseEventKind::Moved | MouseEventKind::Drag(_) => {
            let mut changed = false;
            if let Some((sx, sy)) = app.drag_start_pos {
                let dist_sq =
                    (column as f32 - sx as f32).powi(2) + (row as f32 - sy as f32).powi(2);
                if dist_sq >= 1.0 {
                    if !me.modifiers.contains(KeyModifiers::SHIFT) && !app.selection_mode {
                        if !app.is_dragging {
                            app.is_dragging = true;
                            changed = true;
                        }
                    }
                }
            }

            // Update drop target if dragging
            if app.is_dragging {
                let prev_target = app.hovered_drop_target.clone();
                app.hovered_drop_target = None;
                if column >= sw {
                    if let Some(fs) = app.current_file_state() {
                        // Breadcrumb drop target (e.g., move to parent path quickly).
                        if let Some((_, crumb_path)) = fs.breadcrumb_bounds.iter().find(|(r, _)| {
                            r.contains(ratatui::layout::Position { x: column, y: row })
                        }) {
                            if let Some(src) = &app.drag_source {
                                if src != crumb_path {
                                    app.hovered_drop_target =
                                        Some(DropTarget::Folder(crumb_path.clone()));
                                }
                            }
                        }
                    }

                    // File row folder targets.
                    if app.hovered_drop_target.is_none() && row >= 3 {
                        let idx = crate::event_helpers::fs_mouse_index(row, app);
                        if let Some(fs) = app.current_file_state() {
                            if let Some(path) = fs.files.get(idx) {
                                if path.is_dir() {
                                    if let Some(src) = &app.drag_source {
                                        if src != path {
                                            app.hovered_drop_target =
                                                Some(DropTarget::Folder(path.clone()));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if app.hovered_drop_target != prev_target {
                    changed = true;
                }
            }

            // Selection extension
            let sel_mode = app.selection_mode;
            if row >= 3
                && column >= sw
                && (me.modifiers.contains(KeyModifiers::SHIFT) || sel_mode)
                && !app.is_dragging
            {
                let idx = crate::event_helpers::fs_mouse_index(row, app);
                if let Some(fs) = app.current_file_state_mut() {
                    if !fs.files.is_empty() {
                        let idx = idx.min(fs.files.len().saturating_sub(1));
                        let anchor = fs
                            .selection
                            .anchor
                            .unwrap_or(fs.selection.selected.unwrap_or(0));
                        fs.selection.clear_multi();
                        for i in std::cmp::min(anchor, idx)..=std::cmp::max(anchor, idx) {
                            fs.selection.add(i);
                        }
                        fs.selection.selected = Some(idx);
                        fs.table_state.select(Some(idx));
                        changed = true;
                    }
                }
            }

            if app.is_dragging {
                // Keep repainting while dragging to move drag ghost with cursor.
                true
            } else {
                changed
            }
        }
        MouseEventKind::ScrollUp => {
            if let Some(fs) = app.current_file_state_mut() {
                let new_offset = fs.table_state.offset().saturating_sub(1);
                *fs.table_state.offset_mut() = new_offset;
            }
            true
        }
        MouseEventKind::ScrollDown => {
            if let Some(fs) = app.current_file_state_mut() {
                let max_offset = fs
                    .files
                    .len()
                    .saturating_sub(fs.view_height.saturating_sub(3));
                let new_offset = fs.table_state.offset().saturating_add(1).min(max_offset);
                *fs.table_state.offset_mut() = new_offset;
            }
            true
        }
        _ => false,
    }
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
                let target_pane = app.focused_pane_index.min(app.panes.len().saturating_sub(1));

                let _ = event_tx.try_send(AppEvent::PreviewRequested(target_pane, path));
                app.save_current_view_prefs();
                app.current_view = CurrentView::Editor;
                app.load_view_prefs(CurrentView::Editor);
                app.show_sidebar = true; // Ensure sidebar is visible for "file view on left"

                if app.panes.len() == 1 {
                    app.focused_pane_index = 0;
                } else {
                    app.focused_pane_index = target_pane;
                }
                app.sidebar_focus = false;
            }
        }
    }
}

fn handle_enter_key(app: &mut App, event_tx: &mpsc::Sender<AppEvent>) {
    if app.sidebar_focus {
        let target_opt = app
            .sidebar_bounds
            .iter()
            .find(|b| b.index == app.sidebar_index)
            .map(|b| b.target.clone());
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
                    let _ =
                        event_tx.try_send(AppEvent::ConnectToRemote(app.focused_pane_index, idx));
                }
                SidebarTarget::Project(path) => {
                    if path.is_dir() {
                        if app.expanded_folders.contains(&path) {
                            app.expanded_folders.remove(&path);
                        } else {
                            app.expanded_folders.insert(path);
                        }
                    } else {
                        let _ = event_tx
                            .try_send(AppEvent::PreviewRequested(app.focused_pane_index, path));
                        app.sidebar_focus = false;
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
                if path.is_dir() {
                    navigate_to = Some(path.clone());
                } else {
                    terma::utils::spawn_detached(
                        "xdg-open",
                        vec![path.to_string_lossy().to_string()],
                    );
                }
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
            to_rename = Some(
                p.file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new("root"))
                    .to_string_lossy()
                    .to_string(),
            );
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
            if app.confirm_delete {
                app.mode = AppMode::Delete;
            } else {
                let mut paths = Vec::new();
                if !fs.selection.is_empty() {
                    for &idx in fs.selection.multi_selected_indices() {
                        if let Some(p) = fs.files.get(idx) {
                            paths.push(p.clone());
                        }
                    }
                } else if let Some(idx) = fs.selection.selected {
                    if let Some(p) = fs.files.get(idx) {
                        paths.push(p.clone());
                    }
                }
                for p in paths {
                    let _ = event_tx.try_send(AppEvent::Delete(p));
                }
            }
        }
    }
}

fn handle_quick_copy(app: &mut App, event_tx: &mpsc::Sender<AppEvent>, _to_left: bool) {
    let other_pane_idx = if app.focused_pane_index == 0 { 1 } else { 0 };
    if let Some(dest_path) = app
        .panes
        .get(other_pane_idx)
        .and_then(|p| p.current_state())
        .map(|fs| fs.current_path.clone())
    {
        if let Some(fs) = app.current_file_state() {
            let mut paths = Vec::new();
            if !fs.selection.is_empty() {
                for &idx in fs.selection.multi_selected_indices() {
                    if let Some(p) = fs.files.get(idx) {
                        paths.push(p.clone());
                    }
                }
            } else if let Some(idx) = fs.selection.selected {
                if let Some(p) = fs.files.get(idx) {
                    paths.push(p.clone());
                }
            }
            for p in paths {
                let dest = path_join(
                    &dest_path,
                    p.file_name()
                        .unwrap_or_else(|| std::ffi::OsStr::new("root")),
                );
                let _ = event_tx.try_send(AppEvent::Copy(p, dest));
            }
        }
    }
}

fn path_join(base: &PathBuf, name: &std::ffi::OsStr) -> PathBuf {
    base.join(name)
}
