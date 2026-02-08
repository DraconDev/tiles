use crate::app::{
    App, AppEvent, AppMode, ContextMenuTarget, CurrentView, DropTarget, SidebarTarget,
};
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

            // Plain Escape should exit non-files views.
            if key.code == KeyCode::Esc
                && matches!(
                    app.current_view,
                    CurrentView::Git | CurrentView::Processes | CurrentView::Editor | CurrentView::Commit
                )
            {
                return handle_global_escape(app, &event_tx);
            }

            // Global Escape (Ctrl+[)
            if has_control && key.code == KeyCode::Char('[') {
                return handle_global_escape(app, &event_tx);
            }

            // --- GLOBAL OVERRIDES (High Priority) ---
            if has_control {
                match key.code {
                    KeyCode::Char('m') | KeyCode::Char('M') => {
                        if app.current_view == CurrentView::Editor {
                            app.show_main_stage = !app.show_main_stage;
                            return true;
                        }
                    }
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
                CurrentView::Commit => {
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
    if app.current_view == CurrentView::Commit {
        app.current_view = CurrentView::Git;
        app.mode = AppMode::Normal;
        app.editor_state = None;
        app.sidebar_focus = false;
        app.input.clear();
        app.input_shield_until =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(60));
        return true;
    }

    if matches!(app.mode, AppMode::Normal) {
        match app.current_view {
            CurrentView::Git | CurrentView::Processes => {
                if let Some(fs) = app.current_file_state_mut() {
                    fs.search_filter.clear();
                    fs.git_pending_state.select(None);
                    fs.git_history_state.select(None);
                }
                // Prevent Git-only previews from leaking into file view.
                for pane in &mut app.panes {
                    if let Some(preview) = &pane.preview {
                        let p = preview.path.to_string_lossy();
                        if p.starts_with("git://") || p.starts_with("git-diff://") {
                            pane.preview = None;
                        }
                    }
                }
                app.mode = AppMode::Normal;
                app.input.clear();
                app.current_view = CurrentView::Files;
                app.input_shield_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(150));
                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                return true;
            }
            CurrentView::Editor => {
                // Save before closing if modified
                if let Some(preview) = &app.editor_state {
                    if let Some(editor) = &preview.editor {
                        if editor.modified {
                            let _ = event_tx.try_send(AppEvent::SaveFile(
                                preview.path.clone(),
                                editor.get_content(),
                            ));
                        }
                    }
                }
                for pane in &mut app.panes {
                    if let Some(preview) = &pane.preview {
                        if let Some(editor) = &preview.editor {
                            if editor.modified {
                                let _ = event_tx.try_send(AppEvent::SaveFile(
                                    preview.path.clone(),
                                    editor.get_content(),
                                ));
                            }
                        }
                    }
                    pane.preview = None;
                }

                app.save_current_view_prefs();
                app.current_view = CurrentView::Files;
                app.load_view_prefs(CurrentView::Files);
                app.editor_state = None;
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
    let (w, _) = app.terminal_size;
    app.mouse_pos = (column, row);

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
    if app.current_view == CurrentView::Commit {
        return editor::handle_editor_mouse(me, app, event_tx);
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
    if app.current_view == CurrentView::Editor
        && matches!(
            me.kind,
            MouseEventKind::Down(_) | MouseEventKind::Up(_) | MouseEventKind::Drag(_)
        )
        && column >= sw
    {
        let pane_count = app.panes.len();
        if pane_count > 0 {
            let content_w = w.saturating_sub(sw);
            let pane_w = content_w / pane_count as u16;
            if pane_w > 0 {
                let mut pane_idx = (column.saturating_sub(sw) / pane_w) as usize;
                if pane_idx >= pane_count {
                    pane_idx = pane_count - 1;
                }
                app.focused_pane_index = pane_idx;
                app.sidebar_focus = false;
                if matches!(me.kind, MouseEventKind::Down(_)) {
                    app.mouse_click_pos = (column, row);
                }
            }
        }
    }
    if column < sw {
        handle_sidebar_mouse(me, app, event_tx)
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
            editor::handle_editor_mouse(me, app, event_tx)
        } else {
            file_manager::handle_file_mouse(me, app, event_tx, panes_needing_refresh)
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
            if button == MouseButton::Left {
                app.drag_start_pos = Some((column, row));
            }
            if let Some(b) = app.sidebar_bounds.iter().find(|b| b.y == row).cloned() {
                app.sidebar_index = b.index;
                match button {
                    MouseButton::Left => match &b.target {
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
                                let target_pane = {
                                    let pane_count = app.panes.len();
                                    if pane_count <= 1 {
                                        0
                                    } else {
                                        let sidebar_w = app.sidebar_width();
                                        let content_w = app.terminal_size.0.saturating_sub(sidebar_w);
                                        let pane_w = content_w / pane_count as u16;
                                        if pane_w == 0 {
                                            app.focused_pane_index.min(pane_count - 1)
                                        } else if app.mouse_click_pos.0 >= sidebar_w {
                                            ((app.mouse_click_pos.0.saturating_sub(sidebar_w) / pane_w) as usize)
                                                .min(pane_count - 1)
                                        } else {
                                            app.focused_pane_index.min(pane_count - 1)
                                        }
                                    }
                                };
                                app.focused_pane_index = target_pane;
                                let _ = event_tx.try_send(AppEvent::PreviewRequested(
                                    target_pane,
                                    path.clone(),
                                ));
                                app.sidebar_focus = false;
                            }
                        }
                        _ => {}
                    },
                    MouseButton::Right => {
                        if let SidebarTarget::Favorite(path) = &b.target {
                            let target = ContextMenuTarget::SidebarFavorite(path.clone());
                            let actions =
                                crate::event_helpers::get_context_menu_actions(&target, app);
                            app.mode = AppMode::ContextMenu {
                                x: column,
                                y: row,
                                target,
                                actions,
                                selected_index: None,
                            };
                        }
                    }
                    _ => {}
                }
                if let SidebarTarget::Favorite(ref p) = b.target {
                    if button == MouseButton::Left {
                        app.drag_source = Some(p.clone());
                    }
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
                                // Bounds check to prevent crash
                                if target_idx < app.starred.len() && source_idx != target_idx {
                                    let item = app.starred.remove(source_idx);
                                    // After removal, if source was before target, indices shift down
                                    // Fix: When dragging DOWN (source < target), we want to insert at target_idx
                                    // because items shifted. That places it AFTER the original target item (which moved up).
                                    // When dragging UP (source > target), we want to insert at target_idx
                                    // which places it BEFORE the target item.
                                    let insert_idx = target_idx;

                                    // Ensure we don't exceed bounds
                                    let insert_idx = insert_idx.min(app.starred.len());
                                    app.starred.insert(insert_idx, item);
                                    let _ = crate::config::save_state(app);
                                    let _ = event_tx
                                        .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                }
                            }
                        }
                        DropTarget::Favorites => {
                            // Add folder to favorites when dropped on FAVORITES header
                            if source_path.is_dir() && !app.starred.contains(&source_path) {
                                app.starred.push(source_path.clone());
                                let _ = crate::config::save_state(app);
                                let _ = event_tx
                                    .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                    "Added to favorites: {}",
                                    source_path
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                )));
                            }
                        }
                        _ => {} // Handle other DropTarget variants
                    }
                }
            }
            app.is_dragging = false;
            app.drag_source = None;
            app.hovered_drop_target = None;
            true
        }
        MouseEventKind::Drag(_) | MouseEventKind::Moved => {
            if let Some((sx, sy)) = app.drag_start_pos {
                let dist_sq =
                    (column as f32 - sx as f32).powi(2) + (row as f32 - sy as f32).powi(2);
                if dist_sq >= 1.0 {
                    if !app.is_dragging {
                        app.is_dragging = true;
                    }
                }
            }
            // Update hovered drop target during drag for visual feedback
            if app.is_dragging {
                let prev_target = app.hovered_drop_target.clone();
                app.hovered_drop_target = None;
                // Find what sidebar item we're hovering over
                for bound in &app.sidebar_bounds {
                    if bound.y == row {
                        match &bound.target {
                            SidebarTarget::Favorite(ref _path) => {
                                // Find the favorite index from its position in starred
                                if let Some(fav_idx) = app.starred.iter().position(|p| {
                                    if let SidebarTarget::Favorite(ref bp) = bound.target {
                                        p == bp
                                    } else {
                                        false
                                    }
                                }) {
                                    app.hovered_drop_target =
                                        Some(DropTarget::ReorderFavorite(fav_idx));
                                }
                            }
                            SidebarTarget::Header(name) if name == "FAVORITES" => {
                                // Dragging over FAVORITES header - allow adding to favorites
                                app.hovered_drop_target = Some(DropTarget::Favorites);
                            }
                            _ => {}
                        }
                        break;
                    }
                }
                if app.hovered_drop_target != prev_target {
                    return true;
                }
                // Keep repainting while dragging to move drag ghost with cursor.
                return true;
            }
            false
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{CurrentView, SidebarBounds, SidebarTarget};
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use terma::compositor::engine::TilePlacement;
    use terma::input::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton};
    use tokio::sync::mpsc;

    fn test_app() -> App {
        let queue: Arc<Mutex<Vec<TilePlacement>>> = Arc::new(Mutex::new(Vec::new()));
        App::new(queue)
    }

    #[test]
    fn esc_exits_git_view_to_files() {
        let (tx, mut rx) = mpsc::channel(8);
        let mut app = test_app();
        app.current_view = CurrentView::Git;
        app.mode = AppMode::Normal;

        let mut refresh = HashSet::new();
        let changed = handle_event(
            Event::Key(KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::empty(),
                kind: KeyEventKind::Press,
            }),
            &mut app,
            tx,
            &mut refresh,
        );

        assert!(changed);
        assert_eq!(app.current_view, CurrentView::Files);
        match rx.try_recv() {
            Ok(AppEvent::RefreshFiles(_)) => {}
            other => panic!("expected RefreshFiles event, got {:?}", other),
        }
    }

    #[test]
    fn editor_sidebar_open_targets_last_clicked_pane() {
        let (tx, mut rx) = mpsc::channel(8);
        let mut app = test_app();
        app.current_view = CurrentView::Editor;
        app.mode = AppMode::Normal;
        app.terminal_size = (120, 40);
        app.apply_split_mode(true);
        app.focused_pane_index = 0;
        app.mouse_click_pos = (90, 10); // right pane side
        let test_path = PathBuf::from("/tmp/tiles_editor_sidebar_target.txt");
        app.sidebar_bounds.push(SidebarBounds {
            y: 5,
            index: 0,
            target: SidebarTarget::Project(test_path.clone()),
        });

        let handled = handle_sidebar_mouse(
            &terma::input::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row: 5,
                modifiers: KeyModifiers::empty(),
            },
            &mut app,
            &tx,
        );

        assert!(handled);
        assert_eq!(app.focused_pane_index, 1);

        match rx.try_recv() {
            Ok(AppEvent::PreviewRequested(pane_idx, path)) => {
                assert_eq!(pane_idx, 1);
                assert_eq!(path, test_path);
            }
            other => panic!("expected PreviewRequested event, got {:?}", other),
        }
    }

    #[test]
    fn esc_from_commit_view_returns_to_git() {
        let (tx, _rx) = mpsc::channel(8);
        let mut app = test_app();
        app.current_view = CurrentView::Commit;
        app.mode = AppMode::Viewer;

        let mut refresh = HashSet::new();
        let changed = handle_event(
            Event::Key(KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::empty(),
                kind: KeyEventKind::Press,
            }),
            &mut app,
            tx,
            &mut refresh,
        );

        assert!(changed);
        assert_eq!(app.current_view, CurrentView::Git);
    }
}
