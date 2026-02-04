use crate::app::{App, AppEvent, AppMode, CurrentView, DropTarget, SidebarTarget};
use std::collections::HashSet;
use terma::input::event::{
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use tokio::sync::mpsc;

pub mod editor;
pub mod file_manager;
pub mod git;
pub mod input;
pub mod modals;
pub mod monitor;

pub fn handle_event(
    evt: Event,
    app: &mut App,
    event_tx: mpsc::Sender<AppEvent>,
    panes_needing_refresh: &mut HashSet<usize>,
) -> bool {
    // 1. Input Shield / Cooldown
    if let Some(until) = app.input_shield_until {
        if std::time::Instant::now() < until {
            if let Event::Resize(w, h) = evt {
                app.terminal_size = (w, h);
            }
            return true;
        }
    }

    // 2. Global Resize
    if let Event::Resize(w, h) = evt {
        app.terminal_size = (w, h);
        return true;
    }

    // 3. Mode-specific logic (Modals, Overlays)
    if !matches!(app.mode, AppMode::Normal) {
        crate::app::log_debug(&format!(
            "DEBUG: Dispatching to modals::handle_modal_events (Mode: {:?})",
            app.mode
        ));
        if modals::handle_modal_events(&evt, app, &event_tx) {
            return true;
        }
    }

    // 4. View-specific logic (Keyboard)
    match &evt {
        Event::Key(key) => {
            if key.kind != KeyEventKind::Press {
                return false;
            }

            let has_control = key.modifiers.contains(KeyModifiers::CONTROL);

            // Global Quit
            if (key.code == KeyCode::Char('q') || key.code == KeyCode::Char('Q')) && has_control {
                app.running = false;
                return true;
            }

            // Global Escape (Ctrl+[)
            if has_control && key.code == KeyCode::Char('[') {
                return handle_global_escape(app, &event_tx);
            }

            // --- GLOBAL OVERRIDES (High Priority) ---
            if has_control {
                match key.code {
                    KeyCode::Char('p') | KeyCode::Char('P') => {
                        app.toggle_split();
                        app.save_current_view_prefs();
                        let _ = crate::config::save_state(app);
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                        return true;
                    }
                    KeyCode::Char('b') | KeyCode::Char('B') => {
                        app.show_sidebar = !app.show_sidebar;
                        app.save_current_view_prefs();
                        return true;
                    }
                    KeyCode::Char('e') | KeyCode::Char('E') => {
                        let _ = event_tx.try_send(AppEvent::Editor);
                        return true;
                    }
                    KeyCode::Char('l') | KeyCode::Char('L') => {
                        let _ = event_tx.try_send(AppEvent::GitHistory);
                        return true;
                    }
                    _ => {}
                }
            }

            match app.current_view {
                CurrentView::Editor => {
                    if editor::handle_editor_events(&evt, app, &event_tx) {
                        return true;
                    }
                }
                CurrentView::Processes => {
                    if monitor::handle_monitor_events(&evt, app, &event_tx) {
                        return true;
                    }
                }
                CurrentView::Git => {
                    if git::handle_git_events(&evt, app, &event_tx) {
                        return true;
                    }
                }
                CurrentView::Files => {
                    if file_manager::handle_file_events(&evt, app, &event_tx) {
                        return true;
                    }
                }
            }
        }
        Event::Mouse(me) => {
            return handle_general_mouse(me, app, &event_tx, panes_needing_refresh);
        }
        Event::Paste(text) => {
            if let AppMode::Editor = app.mode {
                if let Some(preview) = &mut app.editor_state {
                    if let Some(editor) = &mut preview.editor {
                        editor.insert_string(text);
                        if app.auto_save {
                            let _ = event_tx.try_send(AppEvent::SaveFile(
                                preview.path.clone(),
                                editor.get_content(),
                            ));
                            editor.modified = false;
                        }
                        return true;
                    }
                }
            }
        }
        _ => {}
    }

    false
}

fn handle_global_escape(app: &mut App, event_tx: &mpsc::Sender<AppEvent>) -> bool {
    if matches!(app.mode, AppMode::Normal) {
        match app.current_view {
            CurrentView::Git | CurrentView::Processes => {
                app.current_view = CurrentView::Files;
                return true;
            }
            CurrentView::Editor => {
                app.save_current_view_prefs();
                app.current_view = CurrentView::Files;
                app.load_view_prefs(CurrentView::Files);
                for pane in &mut app.panes {
                    pane.preview = None;
                }
                app.input.clear(); // Ensure no stray inputs remain
                                   // Increase shield to catch escape sequences
                app.input_shield_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(150));
                // Force a refresh to prevent "path display" glitches or empty lists
                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                return true;
            }
            _ => {}
        }
    } else {
        app.mode = AppMode::Normal;
        app.input.clear();
        app.rename_selected = false;
        return true;
    }
    false
}

fn handle_general_mouse(
    me: &terma::input::event::MouseEvent,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
    panes_needing_refresh: &mut HashSet<usize>,
) -> bool {
    let column = me.column;
    let row = me.row;
    let (w, h) = app.terminal_size;
    app.mouse_pos = (column, row);

    crate::app::log_debug(&format!(
        "DEBUG: Mouse Event: {:?} at ({}, {})",
        me.kind, column, row
    ));

    if let MouseEventKind::Down(MouseButton::Middle) = me.kind {
        crate::app::log_debug("DEBUG: Middle Button Down detected in handle_general_mouse");
    }

    // 1. Sidebar Resizing
    if app.is_resizing_sidebar {
        match me.kind {
            MouseEventKind::Drag(_) | MouseEventKind::Moved => {
                app.sidebar_width_percent = (column as f32 / w as f32 * 100.0) as u16;
                app.sidebar_width_percent = app.sidebar_width_percent.clamp(5, 50);
                return true;
            }
            MouseEventKind::Up(_) => {
                app.is_resizing_sidebar = false;
                let _ = crate::config::save_state(app);
                return true;
            }
            _ => {}
        }
    }

    // 2. View-specific routing
    if app.current_view == CurrentView::Processes {
        return monitor::handle_monitor_mouse(me, app, event_tx);
    }
    if app.current_view == CurrentView::Git {
        return git::handle_git_mouse(me, app, event_tx);
    }

    // 3. Header Icons (Row 0)
    if row == 0 {
        if let MouseEventKind::Down(_) = me.kind {
            if let Some((_, action_id)) = app
                .header_icon_bounds
                .iter()
                .find(|(r, _)| column >= r.x && column < r.x + r.width && row == r.y)
            {
                match action_id.as_str() {
                    "back" => {
                        crate::event_helpers::navigate_back(app);
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                    }
                    "forward" => {
                        crate::event_helpers::navigate_forward(app);
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                    }
                    "split" => {
                        app.toggle_split();
                        app.save_current_view_prefs();
                        let _ = crate::config::save_state(app);
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                    }
                    "burger" => {
                        app.save_current_view_prefs();
                        app.mode = AppMode::Settings;
                        app.settings_scroll = 0;
                    }
                    "monitor" => {
                        let _ = event_tx.try_send(AppEvent::SystemMonitor);
                    }
                    "git" => {
                        let _ = event_tx.try_send(AppEvent::GitHistory);
                    }
                    "project" => {
                        let _ = event_tx.try_send(AppEvent::Editor);
                    }
                    _ => {}
                }
                app.sidebar_focus = false;
                return true;
            }
        }
        // Hover
        if let Some((_, id)) = app
            .header_icon_bounds
            .iter()
            .find(|(r, _)| r.contains(ratatui::layout::Position { x: column, y: row }))
        {
            app.hovered_header_icon = Some(id.clone());
        } else {
            app.hovered_header_icon = None;
        }
    }

    // 4. Tabs
    if let Some((_, p_idx, t_idx)) = app
        .tab_bounds
        .iter()
        .find(|(r, _, _)| r.contains(ratatui::layout::Position { x: column, y: row }))
        .cloned()
    {
        match me.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(p) = app.panes.get_mut(p_idx) {
                    p.active_tab_index = t_idx;
                    app.focused_pane_index = p_idx;
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(p_idx));
                }
                app.sidebar_focus = false;
                return true;
            }
            MouseEventKind::Down(MouseButton::Right) => {
                if let Some(p) = app.panes.get_mut(p_idx) {
                    if p.tabs.len() > 1 {
                        p.tabs.remove(t_idx);
                        if p.active_tab_index >= p.tabs.len() {
                            p.active_tab_index = p.tabs.len() - 1;
                        }
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(p_idx));
                    }
                }
                return true;
            }
            _ => {}
        }
    }

    // 5. Sidebar vs Panes
    let sw = app.sidebar_width();
    if column < sw {
        return handle_sidebar_mouse(me, app, event_tx);
    } else {
        // Sidebar Resizing check (MUST BE LEFT CLICK ONLY)
        if let MouseEventKind::Down(MouseButton::Left) = me.kind {
            if column >= sw.saturating_sub(1) && column <= sw + 1 {
                app.is_resizing_sidebar = true;
                return true;
            }
        }

        let is_editor_mode = matches!(
            app.mode,
            AppMode::Editor
                | AppMode::Viewer
                | AppMode::EditorSearch
                | AppMode::EditorReplace
                | AppMode::EditorGoToLine
        );
        if app.current_view == CurrentView::Editor || is_editor_mode {
            crate::app::log_debug("DEBUG: Routing mouse to editor::handle_editor_mouse");
            return editor::handle_editor_mouse(me, app, event_tx);
        } else {
            return file_manager::handle_file_mouse(me, app, event_tx, panes_needing_refresh);
        }
    }
}

fn handle_sidebar_mouse(
    me: &terma::input::event::MouseEvent,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
) -> bool {
    let column = me.column;
    let row = me.row;

    match me.kind {
        MouseEventKind::Down(button) => {
            app.sidebar_focus = true;
            app.drag_start_pos = Some((column, row));
            if let Some(b) = app.sidebar_bounds.iter().find(|b| b.y == row).cloned() {
                app.sidebar_index = b.index;
                if button == MouseButton::Left {
                    match &b.target {
                        SidebarTarget::Favorite(path) => {
                            if let Some(fs) = app.current_file_state_mut() {
                                fs.current_path = path.clone();
                                fs.selection.clear();
                                crate::event_helpers::push_history(fs, path.clone());
                                let _ = event_tx
                                    .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                            }
                        }
                        SidebarTarget::Remote(idx) => {
                            let _ = event_tx
                                .try_send(AppEvent::ConnectToRemote(app.focused_pane_index, *idx));
                        }
                        SidebarTarget::Project(path) => {
                            if path.is_dir() {
                                if app.expanded_folders.contains(path) {
                                    app.expanded_folders.remove(path);
                                } else {
                                    app.expanded_folders.insert(path.clone());
                                }
                            } else {
                                let _ = event_tx.try_send(AppEvent::PreviewRequested(
                                    app.focused_pane_index,
                                    path.clone(),
                                ));
                                app.sidebar_focus = false;
                            }
                        }
                        _ => {}
                    }
                }
                if let SidebarTarget::Favorite(ref p) = b.target {
                    app.drag_source = Some(p.clone());
                }
            }
            true
        }
        MouseEventKind::Up(_) => {
            if let Some(target) = app.hovered_drop_target.take() {
                if let Some(source_path) = app.drag_source.take() {
                    match target {
                        DropTarget::ReorderFavorite(target_idx) => {
                            // Find source index
                            if let Some(source_idx) =
                                app.starred.iter().position(|p| p == &source_path)
                            {
                                if source_idx != target_idx {
                                    let item = app.starred.remove(source_idx);
                                    // Adjust target index if shifting
                                    let insert_idx = if source_idx < target_idx {
                                        target_idx
                                    } else {
                                        target_idx
                                    };
                                    if insert_idx <= app.starred.len() {
                                        app.starred.insert(insert_idx, item);
                                    }
                                    let _ = crate::config::save_state(app);
                                    let _ = event_tx
                                        .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            app.is_dragging = false;
            app.drag_source = None;
            app.hovered_drop_target = None;
            true
        }
        MouseEventKind::Drag(_) => {
            if let Some((sx, sy)) = app.drag_start_pos {
                if ((column as i16 - sx as i16).pow(2) + (row as i16 - sy as i16).pow(2)) as f32
                    >= 1.0
                {
                    app.is_dragging = true;
                }
            }
            true
        }
        _ => false,
    }
}
