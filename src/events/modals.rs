use terma::input::event::{Event, KeyCode, KeyModifiers, MouseEventKind, MouseButton};
use tokio::sync::mpsc;
use crate::app::{App, AppEvent, AppMode, ContextMenuAction, DropTarget, SidebarTarget, ContextMenuTarget, FileColumn};
use crate::events::input::delete_word_backwards;
use crate::icons::Icon;
use unicode_width::UnicodeWidthStr;
use terma::utils::get_visual_width;

pub fn handle_modal_events(evt: &Event, app: &mut App, event_tx: &mpsc::Sender<AppEvent>) -> bool {
    match evt {
        Event::Key(key) => handle_modal_keys(key, app, event_tx, evt),
        Event::Mouse(me) => handle_modal_mouse(me, app, event_tx),
        _ => false,
    }
}

fn handle_modal_keys(key: &terma::input::event::KeyEvent, app: &mut App, event_tx: &mpsc::Sender<AppEvent>, evt: &Event) -> bool {
    let has_control = key.modifiers.contains(KeyModifiers::CONTROL);
    
    match &app.mode {
        AppMode::ContextMenu { actions, target, selected_index, .. } => {
            handle_context_menu_keys(key, app, event_tx, actions, target, *selected_index)
        }
        AppMode::DragDropMenu { sources, target } => {
            handle_drag_drop_keys(key, app, event_tx, sources, target)
        }
        AppMode::EditorReplace => {
            handle_editor_replace_keys(key, app, event_tx, evt)
        }
        AppMode::EditorSearch => {
            handle_editor_search_keys(key, app, event_tx, evt)
        }
        AppMode::EditorGoToLine => {
            handle_editor_goto_keys(key, app, event_tx, evt)
        }
        AppMode::CommandPalette => {
            handle_command_palette_keys(key, app, event_tx, evt)
        }
        AppMode::AddRemote(idx) => {
            handle_add_remote_keys(key, app, event_tx, *idx, evt)
        }
        AppMode::Highlight => {
            handle_highlight_keys(key, app)
        }
        AppMode::NewFile | AppMode::NewFolder | AppMode::Rename | AppMode::Delete | AppMode::DeleteFile(_) => {
            handle_input_modals_keys(key, app, event_tx)
        }
        AppMode::Header(idx) => {
            handle_header_keys(key, app, event_tx, *idx)
        }
        _ => false,
    }
}

fn handle_context_menu_keys(key: &terma::input::event::KeyEvent, app: &mut App, event_tx: &mpsc::Sender<AppEvent>, actions: &[ContextMenuAction], target: &ContextMenuTarget, selected_index: Option<usize>) -> bool {
    match key.code {
        KeyCode::Esc => { app.mode = AppMode::Normal; true }
        KeyCode::Up => {
            let mut new_idx = match selected_index {
                Some(idx) => if idx > 0 { idx - 1 } else { actions.len().saturating_sub(1) },
                None => actions.len().saturating_sub(1),
            };
            if let Some(ContextMenuAction::Separator) = actions.get(new_idx) {
                if new_idx > 0 { new_idx -= 1; }
            }
            if let AppMode::ContextMenu { selected_index: ref mut si, .. } = app.mode { *si = Some(new_idx); }
            true
        }
        KeyCode::Down => {
            let mut new_idx = match selected_index {
                Some(idx) => if idx < actions.len().saturating_sub(1) { idx + 1 } else { 0 },
                None => 0,
            };
            if let Some(ContextMenuAction::Separator) = actions.get(new_idx) {
                if new_idx < actions.len().saturating_sub(1) { new_idx += 1; }
            }
            if let AppMode::ContextMenu { selected_index: ref mut si, .. } = app.mode { *si = Some(new_idx); }
            true
        }
        KeyCode::Enter => {
            if let Some(idx) = selected_index {
                if let Some(action) = actions.get(idx) {
                    if *action != ContextMenuAction::Separator {
                        let action = action.clone();
                        let target = target.clone();
                        app.mode = AppMode::Normal;
                        crate::event_helpers::handle_context_menu_action(&action, &target, app, event_tx.clone());
                    }
                }
            }
            true
        }
        _ => true,
    }
}

fn handle_drag_drop_keys(key: &terma::input::event::KeyEvent, app: &mut App, event_tx: &mpsc::Sender<AppEvent>, sources: &[std::path::PathBuf], target: &std::path::Path) -> bool {
    match key.code {
        KeyCode::Char('c') | KeyCode::Char('C') => {
            for source in sources {
                let dest = target.join(source.file_name().unwrap_or_else(|| std::ffi::OsStr::new("root")));
                let _ = event_tx.try_send(AppEvent::Copy(source.clone(), dest));
            }
            app.mode = AppMode::Normal; true
        }
        KeyCode::Char('m') | KeyCode::Char('M') => {
            for source in sources {
                let dest = target.join(source.file_name().unwrap_or_else(|| std::ffi::OsStr::new("root")));
                let _ = event_tx.try_send(AppEvent::Rename(source.clone(), dest));
            }
            if let Some(fs) = app.current_file_state_mut() { fs.selection.clear_multi(); fs.selection.anchor = None; }
            app.mode = AppMode::Normal; true
        }
        KeyCode::Char('l') | KeyCode::Char('L') => {
            for source in sources {
                let dest = target.join(source.file_name().unwrap_or_else(|| std::ffi::OsStr::new("root")));
                let _ = event_tx.try_send(AppEvent::Symlink(source.clone(), dest));
            }
            app.mode = AppMode::Normal; true
        }
        KeyCode::Esc => { app.mode = AppMode::Normal; true }
        _ => true,
    }
}

fn handle_editor_replace_keys(key: &terma::input::event::KeyEvent, app: &mut App, event_tx: &mpsc::Sender<AppEvent>, evt: &Event) -> bool {
    match key.code {
        KeyCode::Esc => { app.mode = app.previous_mode.clone(); app.input.clear(); app.replace_buffer.clear(); true }
        KeyCode::Tab | KeyCode::Enter => {
            if app.replace_buffer.is_empty() {
                app.replace_buffer = app.input.value.clone(); app.input.clear();
                let _ = event_tx.try_send(AppEvent::StatusMsg(format!("Replace '{}' with: (Enter: next, ^Enter: all)", app.replace_buffer)));
            } else {
                let replace_term = app.input.value.clone();
                let find_term = app.replace_buffer.clone();
                let is_all = key.modifiers.contains(KeyModifiers::CONTROL);

                // Check global editor
                if let Some(preview) = &mut app.editor_state {
                    if let Some(editor) = &mut preview.editor {
                        editor.push_history();
                        if is_all {
                            editor.replace_all(&find_term, &replace_term);
                            let _ = event_tx.try_send(AppEvent::StatusMsg(format!("Replaced all '{}' with '{}'", find_term, replace_term)));
                        } else {
                            editor.replace_next(&find_term, &replace_term);
                            let (w, h) = app.terminal_size;
                            editor.ensure_cursor_centered(ratatui::layout::Rect::new(1, 1, w.saturating_sub(2), h.saturating_sub(2)));
                        }
                    }
                }
                // Check IDE editor
                let focused_idx = app.focused_pane_index;
                if let Some(pane) = app.panes.get_mut(focused_idx) {
                    if let Some(preview) = &mut pane.preview {
                        if let Some(editor) = &mut preview.editor {
                            editor.push_history();
                            if is_all {
                                editor.replace_all(&find_term, &replace_term);
                            } else {
                                editor.replace_next(&find_term, &replace_term);
                            }
                        }
                    }
                }
                app.mode = app.previous_mode.clone(); app.input.clear(); app.replace_buffer.clear();
            }
            true
        }
        _ => {
            let res = app.input.handle_event(evt);
            if res && app.replace_buffer.is_empty() && app.input.value.is_empty() {
                app.mode = app.previous_mode.clone(); app.input.clear(); app.replace_buffer.clear();
            }
            res
        }
    }
}

fn handle_editor_search_keys(key: &terma::input::event::KeyEvent, app: &mut App, _event_tx: &mpsc::Sender<AppEvent>, evt: &Event) -> bool {
    match key.code {
        KeyCode::Esc | KeyCode::Enter => {
            let clear_filter = |ed: &mut terma::widgets::TextEditor| ed.set_filter("");
            if let Some(preview) = &mut app.editor_state { if let Some(editor) = &mut preview.editor { clear_filter(editor); } }
            if let Some(pane) = app.panes.get_mut(app.focused_pane_index) { if let Some(preview) = &mut pane.preview { if let Some(editor) = &mut preview.editor { clear_filter(editor); } } }
            app.mode = app.previous_mode.clone(); app.input.clear(); true
        }
        KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown => {
            if let Some(preview) = &mut app.editor_state { if let Some(editor) = &mut preview.editor { editor.handle_event(evt, ratatui::layout::Rect::new(1, 1, app.terminal_size.0.saturating_sub(2), app.terminal_size.1.saturating_sub(2))); } }
            if let Some(pane) = app.panes.get_mut(app.focused_pane_index) { if let Some(preview) = &mut pane.preview { if let Some(editor) = &mut preview.editor { editor.handle_event(evt, ratatui::layout::Rect::new(0, 0, 100, 100)); } } }
            true
        }
        _ => {
            let handled = app.input.handle_event(evt);
            if handled {
                let filter = app.input.value.clone();
                if filter.is_empty() { app.mode = app.previous_mode.clone(); app.input.clear(); return true; }
                if let Some(preview) = &mut app.editor_state { if let Some(editor) = &mut preview.editor { editor.set_filter(&filter); } }
                if let Some(pane) = app.panes.get_mut(app.focused_pane_index) { if let Some(preview) = &mut pane.preview { if let Some(editor) = &mut preview.editor { editor.set_filter(&filter); } } }
            }
            handled
        }
    }
}

fn handle_editor_goto_keys(key: &terma::input::event::KeyEvent, app: &mut App, _event_tx: &mpsc::Sender<AppEvent>, evt: &Event) -> bool {
    match key.code {
        KeyCode::Esc => { app.mode = app.previous_mode.clone(); app.input.clear(); true }
        KeyCode::Enter => {
            if let Ok(line_num) = app.input.value.parse::<usize>() {
                let target = line_num.saturating_sub(1);
                if let Some(preview) = &mut app.editor_state { if let Some(editor) = &mut preview.editor { editor.cursor_row = std::cmp::min(target, editor.lines.len().saturating_sub(1)); editor.cursor_col = 0; } }
                if let Some(pane) = app.panes.get_mut(app.focused_pane_index) { if let Some(preview) = &mut pane.preview { if let Some(editor) = &mut preview.editor { editor.cursor_row = std::cmp::min(target, editor.lines.len().saturating_sub(1)); editor.cursor_col = 0; } } }
            }
            app.mode = app.previous_mode.clone(); app.input.clear(); true
        }
        _ => app.input.handle_event(evt),
    }
}

fn handle_command_palette_keys(key: &terma::input::event::KeyEvent, app: &mut App, event_tx: &mpsc::Sender<AppEvent>, evt: &Event) -> bool {
    match key.code {
        KeyCode::Esc => { app.mode = AppMode::Normal; true }
        KeyCode::Enter => {
            if let Some(cmd) = app.filtered_commands.get(app.command_index).cloned() { crate::event_helpers::execute_command(cmd.action, app, event_tx.clone()); }
            app.mode = AppMode::Normal; app.input.clear(); true
        }
        _ => {
            let handled = app.input.handle_event(evt);
            if handled { crate::event_helpers::update_commands(app); }
            handled
        }
    }
}

fn handle_add_remote_keys(key: &terma::input::event::KeyEvent, app: &mut App, _event_tx: &mpsc::Sender<AppEvent>, idx: usize, evt: &Event) -> bool {
    match key.code {
        KeyCode::Esc => { app.mode = AppMode::Normal; app.input.clear(); true }
        KeyCode::Tab | KeyCode::Enter => {
            let val = app.input.value.clone();
            match idx { 0 => app.pending_remote.name = val, 1 => app.pending_remote.host = val, 2 => app.pending_remote.user = val, 3 => app.pending_remote.port = val.parse().unwrap_or(22), 4 => app.pending_remote.key_path = if val.is_empty() { None } else { Some(std::path::PathBuf::from(val)) }, _ => {} }
            if idx < 4 { app.mode = AppMode::AddRemote(idx + 1); app.input.set_value(String::new()); }
            else { app.remote_bookmarks.push(app.pending_remote.clone()); let _ = crate::config::save_state(app); app.mode = AppMode::Normal; app.input.clear(); }
            true
        }
        _ => app.input.handle_event(evt),
    }
}

fn handle_highlight_keys(key: &terma::input::event::KeyEvent, app: &mut App) -> bool {
    if let KeyCode::Char(c) = key.code {
        if let Some(digit) = c.to_digit(10) {
            if digit <= 6 {
                let color = if digit == 0 { None } else { Some(digit as u8) };
                if let Some(fs) = app.current_file_state() {
                    let mut paths = Vec::new();
                    if !fs.selection.is_empty() { for &idx in fs.selection.multi_selected_indices() { if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); } } }
                    else if let Some(idx) = fs.selection.selected { if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); } }
                    for p in paths { if let Some(col) = color { app.path_colors.insert(p, col); } else { app.path_colors.remove(&p); } }
                    let _ = crate::config::save_state(app);
                }
                app.mode = AppMode::Normal; true
            } else { false }
        } else { false }
    } else if key.code == KeyCode::Esc { app.mode = AppMode::Normal; true }
    else { false }
}

fn handle_input_modals_keys(key: &terma::input::event::KeyEvent, app: &mut App, event_tx: &mpsc::Sender<AppEvent>) -> bool {
    match key.code {
        KeyCode::Esc => { app.mode = AppMode::Normal; app.input.clear(); app.rename_selected = false; true }
        KeyCode::Enter => {
            let input = app.input.value.clone();
            if let AppMode::DeleteFile(ref path) = app.mode {
                if input.trim().to_lowercase() == "y" || !app.confirm_delete {
                    let _ = event_tx.try_send(AppEvent::Delete(path.clone()));
                    app.mode = AppMode::Normal;
                } else { app.mode = AppMode::Normal; }
                app.input.clear(); return true;
            }
            if let Some(fs) = app.current_file_state() {
                let path = fs.current_path.join(&input);
                match app.mode {
                    AppMode::NewFile => { let _ = event_tx.try_send(AppEvent::CreateFile(path)); }
                    AppMode::NewFolder => { let _ = event_tx.try_send(AppEvent::CreateFolder(path)); }
                    AppMode::Rename => { if let Some(idx) = fs.selection.selected { if let Some(old) = fs.files.get(idx) { let _ = event_tx.try_send(AppEvent::Rename(old.clone(), old.parent().unwrap().join(&input))); } } }
                    AppMode::Delete => { if input.trim().to_lowercase() == "y" || !app.confirm_delete { /* delete logic */ } }
                    _ => {}
                }
            }
            app.mode = AppMode::Normal; app.input.clear(); true
        }
        _ => app.input.handle_event(&Event::Key(key.clone())),
    }
}

fn handle_header_keys(key: &terma::input::event::KeyEvent, app: &mut App, event_tx: &mpsc::Sender<AppEvent>, idx: usize) -> bool {
    match key.code {
        KeyCode::Esc => { app.mode = AppMode::Normal; true }
        KeyCode::Enter => {
            if idx <= 6 { /* header icon logic */ }
            else { /* tab logic */ }
            app.mode = AppMode::Normal; true
        }
        KeyCode::Left => { if idx > 0 { app.mode = AppMode::Header(idx - 1); } true }
        KeyCode::Right => { app.mode = AppMode::Header(idx + 1); true }
        _ => true,
    }
}

fn handle_modal_mouse(me: &terma::input::event::MouseEvent, app: &mut App, event_tx: &mpsc::Sender<AppEvent>) -> bool {
    match &app.mode {
        AppMode::ContextMenu { .. } => handle_context_menu_mouse(me, app, event_tx),
        AppMode::Highlight => handle_highlight_mouse(me, app),
        _ => false,
    }
}

fn handle_context_menu_mouse(me: &terma::input::event::MouseEvent, app: &mut App, event_tx: &mpsc::Sender<AppEvent>) -> bool {
    if let MouseEventKind::Down(_) = me.kind {
        // click logic
        true
    } else { false }
}

fn handle_highlight_mouse(me: &terma::input::event::MouseEvent, app: &mut App) -> bool {
    if let MouseEventKind::Down(_) = me.kind {
        // click logic
        true
    } else { false }
}
