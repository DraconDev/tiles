use terma::input::event::{Event, KeyCode, KeyModifiers};
use tokio::sync::mpsc;
use crate::app::{App, AppEvent, AppMode, CurrentView};

pub fn handle_editor_events(evt: &Event, app: &mut App, event_tx: &mpsc::Sender<AppEvent>) -> bool {
    let key = match evt {
        Event::Key(k) => k,
        _ => return false,
    };

    // 1. View-Specific Esc Handling
    if key.code == KeyCode::Esc && matches!(app.mode, AppMode::Normal) {
        if let CurrentView::Editor = app.current_view {
            app.save_current_view_prefs();
            app.current_view = CurrentView::Files;
            app.load_view_prefs(CurrentView::Files);
            for pane in &mut app.panes {
                pane.preview = None;
            }
            app.input_shield_until = Some(std::time::Instant::now() + std::time::Duration::from_millis(50));
            return true;
        }
    }

    // 2. IDE/Editor Mode Key Handling (Pane Editor)
    if app.current_view == CurrentView::Editor && !app.sidebar_focus && matches!(app.mode, AppMode::Normal) {
        let (w, h) = app.terminal_size;
        let sw = app.sidebar_width();
        let pc = app.panes.len();
        let cw = w.saturating_sub(sw);
        let pw = if pc > 0 { cw / pc as u16 } else { cw };
        let pane_idx = app.focused_pane_index;

        let pane_area = ratatui::layout::Rect::new(
            sw + (pane_idx as u16 * pw),
            1, 
            pw,
            h.saturating_sub(1),
        );

        if let Some(pane) = app.panes.get_mut(pane_idx) {
            if let Some(preview) = &mut pane.preview {
                if let Some(editor) = &mut preview.editor {
                    let mut clipboard = app.editor_clipboard.clone();
                    let auto_save = app.auto_save;
                    let mut mode = app.mode.clone();
                    let mut prev_mode = app.previous_mode.clone();

                    let handled = handle_generic_editor_shortcuts(
                        key, editor, &mut clipboard, auto_save, &mut mode, &mut prev_mode, 
                        &mut app.input, &mut app.replace_buffer, event_tx, &preview.path, evt, pane_area
                    );

                    app.editor_clipboard = clipboard;
                    app.mode = mode;
                    app.previous_mode = prev_mode;
                    return handled;
                }
            }
        }
    }

    // 3. Full-Screen Editor Priority
    if let AppMode::Editor = app.mode {
        if let Some(preview) = &mut app.editor_state {
            if let Some(editor) = &mut preview.editor {
                if key.code == KeyCode::Esc {
                    app.mode = AppMode::Normal;
                    app.editor_state = None;
                    return true;
                }

                let (w, h) = app.terminal_size;
                let editor_area = ratatui::layout::Rect::new(1, 1, w.saturating_sub(2), h.saturating_sub(2));

                let mut clipboard = app.editor_clipboard.clone();
                let auto_save = app.auto_save;
                let mut mode = app.mode.clone();
                let mut prev_mode = app.previous_mode.clone();

                let handled = handle_generic_editor_shortcuts(
                    key, editor, &mut clipboard, auto_save, &mut mode, &mut prev_mode,
                    &mut app.input, &mut app.replace_buffer, event_tx, &preview.path, evt, editor_area
                );

                app.editor_clipboard = clipboard;
                app.mode = mode;
                app.previous_mode = prev_mode;
                return handled;
            }
        }
    }

    false
}

fn handle_generic_editor_shortcuts(
    key: &terma::input::event::KeyEvent,
    editor: &mut terma::widgets::TextEditor,
    clipboard: &mut Option<String>,
    auto_save: bool,
    mode: &mut AppMode,
    prev_mode: &mut AppMode,
    input: &mut terma::widgets::TextInput,
    replace_buffer: &mut String,
    event_tx: &mpsc::Sender<AppEvent>,
    path: &std::path::PathBuf,
    evt: &Event,
    area: ratatui::layout::Rect,
) -> bool {
    let has_control = key.modifiers.contains(KeyModifiers::CONTROL);

    if has_control && (key.code == KeyCode::Char('s') || key.code == KeyCode::Char('S')) {
        let _ = event_tx.try_send(AppEvent::SaveFile(path.clone(), editor.get_content()));
        editor.modified = false;
        return true;
    }

    if (has_control && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('C'))) || (has_control && key.code == KeyCode::Insert) {
        let content = if let Some(selected) = editor.get_selected_text() { selected } else { editor.lines.get(editor.cursor_row).cloned().unwrap_or_default() };
        *clipboard = Some(content.clone());
        terma::utils::set_clipboard_text(&content);
        let _ = event_tx.try_send(AppEvent::StatusMsg("Copied to clipboard".to_string()));
        return true;
    }

    if (has_control && (key.code == KeyCode::Char('x') || key.code == KeyCode::Char('X'))) || (key.modifiers.contains(KeyModifiers::SHIFT) && key.code == KeyCode::Delete) {
        let content = if let Some(selected) = editor.get_selected_text() { selected } else { editor.lines.get(editor.cursor_row).cloned().unwrap_or_default() };
        *clipboard = Some(content.clone());
        terma::utils::set_clipboard_text(&content);
        if editor.get_selection_range().is_some() { editor.push_history(); editor.delete_selection(); }
        else { editor.delete_line(editor.cursor_row); }
        let _ = event_tx.try_send(AppEvent::StatusMsg("Cut to clipboard".to_string()));
        if auto_save { let _ = event_tx.try_send(AppEvent::SaveFile(path.clone(), editor.get_content())); editor.modified = false; }
        return true;
    }

    if (has_control && (key.code == KeyCode::Char('v') || key.code == KeyCode::Char('V'))) || (key.modifiers.contains(KeyModifiers::SHIFT) && key.code == KeyCode::Insert) {
        let text_to_paste = clipboard.clone().or_else(|| terma::utils::get_clipboard_text());
        if let Some(text) = text_to_paste {
            editor.insert_string(&text);
            editor.modified = true;
            if auto_save { let _ = event_tx.try_send(AppEvent::SaveFile(path.clone(), editor.get_content())); editor.modified = false; }
        }
        return true;
    }

    if has_control && (key.code == KeyCode::Char('z') || key.code == KeyCode::Char('Z')) { editor.handle_event(evt, area); return true; }
    if has_control && (key.code == KeyCode::Char('y') || key.code == KeyCode::Char('Y')) { editor.handle_event(evt, area); return true; }

    if has_control {
        match key.code {
            KeyCode::Char('f') | KeyCode::Char('F') => {
                *prev_mode = mode.clone();
                *mode = AppMode::EditorSearch;
                input.set_value(editor.filter_query.clone());
                return true;
            }
            KeyCode::Char('g') | KeyCode::Char('G') => {
                *prev_mode = mode.clone();
                *mode = AppMode::EditorGoToLine;
                input.clear();
                return true;
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                *prev_mode = mode.clone();
                *mode = AppMode::EditorReplace;
                input.clear();
                replace_buffer.clear();
                let _ = event_tx.try_send(AppEvent::StatusMsg("Replace: Type term to FIND, then press Enter/Tab".to_string()));
                return true;
            }
            _ => {}
        }
    }
    if key.code == KeyCode::F(2) {
        *prev_mode = mode.clone(); *mode = AppMode::EditorReplace; input.clear(); replace_buffer.clear();
        let _ = event_tx.try_send(AppEvent::StatusMsg("Replace: Type term to FIND, then press Enter/Tab".to_string()));
        return true;
    }

    if editor.handle_event(evt, area) {
        if auto_save && editor.modified {
            let _ = event_tx.try_send(AppEvent::SaveFile(path.clone(), editor.get_content()));
            editor.modified = false;
        }
        return true;
    }

    false
}