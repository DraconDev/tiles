use std::path::PathBuf;
use crate::app::{App, AppEvent, AppMode, CurrentView};
use dracon_terminal_engine::contracts::{
    InputEvent as Event, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use tokio::sync::mpsc;
use unicode_width::UnicodeWidthStr;

const DOUBLE_CLICK_MS: u64 = 500;

fn pane_editor_area(app: &App, pane_idx: usize) -> Option<ratatui::layout::Rect> {
    let (w, h) = app.terminal_size;
    let sw = app.sidebar_width();
    let stage_y = 1;
    let stage_h = h.saturating_sub(3);
    let pc = app.panes.len();
    if pc == 0 {
        return None;
    }
    let cw = w.saturating_sub(sw);
    let pw = cw / pc as u16;
    if pw == 0 {
        return None;
    }

    let pane_x = sw + (pane_idx as u16 * pw);
    let pane_w = if pane_idx + 1 == pc {
        w.saturating_sub(pane_x)
    } else {
        pw
    };
    if pane_w < 2 || stage_h < 3 {
        return None;
    }

    let pane_area = ratatui::layout::Rect::new(pane_x, stage_y, pane_w, stage_h);
    let inner = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .inner(pane_area);
    Some(inner)
}

fn commit_editor_area(app: &App) -> ratatui::layout::Rect {
    use ratatui::layout::{Constraint, Direction, Layout, Rect};
    use ratatui::widgets::{Block, Borders};

    let (w, h) = app.terminal_size;
    let area = Rect::new(0, 0, w, h);
    let outer = Block::default().borders(Borders::ALL);
    let inner = outer.inner(area);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);
    let content_block = Block::default().borders(Borders::ALL);
    content_block.inner(layout[4])
}

pub fn handle_editor_events(evt: &Event, app: &mut App, event_tx: &mpsc::Sender<AppEvent>) -> bool {
    let key = match evt {
        Event::Key(k) => k,
        _ => return false,
    };

    let has_control = key.modifiers.contains(KeyModifiers::CONTROL);

    // 1. View-Specific Esc Handling
    if key.code == KeyCode::Esc && matches!(app.mode, AppMode::Normal) {
        if let CurrentView::Editor = app.current_view {
            app.save_current_view_prefs();
            app.current_view = CurrentView::Files;
            app.load_view_prefs(CurrentView::Files);
            app.editor_state = None;
            for pane in &mut app.panes {
                for fs in &mut pane.tabs {
                    fs.preview = None;
                }
            }
            app.input_shield_until =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(50));
            return true;
        }
    }

    // 2. IDE/Editor Mode Key Handling (Pane Editor)
    if app.current_view == CurrentView::Editor
        && !app.sidebar_focus
        && matches!(app.mode, AppMode::Normal)
    {
        let pane_idx = app.focused_pane_index;

        if has_control && key.code == KeyCode::Tab {
            if let Some(pane) = app.panes.get_mut(pane_idx) {
                let tab_count = pane.tabs.len();
                if tab_count > 1 {
                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                        pane.active_tab_index = pane.active_tab_index.saturating_sub(1);
                    } else {
                        pane.active_tab_index = (pane.active_tab_index + 1) % tab_count;
                    }
                }
            }
            return true;
        }

        if has_control && key.code == KeyCode::Char('w') {
            if let Some(pane) = app.panes.get_mut(pane_idx) {
                if pane.tabs.len() > 1 {
                    let removed = pane.tabs.remove(pane.active_tab_index);
                    if pane.active_tab_index >= pane.tabs.len() {
                        pane.active_tab_index = pane.tabs.len() - 1;
                    }
                    if let Some(fs) = pane.current_state_mut() {
                        fs.preview = None;
                    }
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(pane_idx));
                    let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                        "Closed: {}",
                        removed.current_path.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default()
                    )));
                } else {
                    if let Some(fs) = pane.current_state_mut() {
                        fs.preview = None;
                    }
                    let _ = event_tx.try_send(AppEvent::StatusMsg("No more tabs to close".to_string()));
                }
            }
            return true;
        }

        if has_control && key.code == KeyCode::Char('n') {
            if let Some(pane) = app.panes.get(pane_idx) {
                let base_dir = if let Some(fs) = pane.current_state() {
                    if let Some(ref preview) = fs.preview {
                        if preview.path.is_dir() {
                            preview.path.clone()
                        } else {
                            preview.path.parent().unwrap_or(&PathBuf::from("/")).to_path_buf()
                        }
                    } else {
                        fs.current_path.clone()
                    }
                } else {
                    PathBuf::from(".")
                };
                if let Some(fs) = app.current_file_state_mut() {
                    fs.current_path = base_dir;
                }
            }
            app.mode = AppMode::NewFile;
            app.input.clear();
            return true;
        }

        // Ctrl+Enter: run the current file
        if has_control && key.code == KeyCode::Enter {
            if let Some(pane) = app.panes.get(pane_idx) {
                if let Some(fs) = pane.current_state() {
                    if let Some(ref preview) = fs.preview {
                        if let Some((work_dir, program, args)) =
                            crate::modules::files::get_run_command(&preview.path)
                        {
                            let _ = event_tx.try_send(AppEvent::SpawnTerminal {
                                path: work_dir,
                                new_tab: true,
                                remote: fs.remote_session.clone(),
                                command: Some(format!("{} {}", program, args.join(" "))),
                            });
                            let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                "Running: {} {}",
                                program,
                                args.join(" ")
                            )));
                        } else {
                            let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                "No run command for: {}",
                                preview
                                    .path
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .map(|e| format!(".{e}"))
                                    .unwrap_or_else(|| "unknown".to_string())
                            )));
                        }
                    }
                }
            }
            return true;
        }

        let Some(pane_area) = pane_editor_area(app, pane_idx) else {
            return false;
        };

        if let Some(pane) = app.panes.get_mut(pane_idx) {
            if let Some(fs) = pane.current_state_mut() {
                if let Some(preview) = &mut fs.preview {
                    if let Some(editor) = &mut preview.editor {
                        let mut clipboard = app.editor_clipboard.clone();
                        let auto_save = app.auto_save;
                        let mut mode = app.mode.clone();
                        let mut prev_mode = app.previous_mode.clone();

                        let handled = handle_generic_editor_shortcuts(
                            key,
                            editor,
                            &mut clipboard,
                            auto_save,
                            &mut mode,
                            &mut prev_mode,
                            &mut app.input,
                            &mut app.replace_buffer,
                            event_tx,
                            &preview.path,
                            evt,
                            pane_area,
                        );

                        app.editor_clipboard = clipboard;
                        app.mode = mode;
                        app.previous_mode = prev_mode;
                        return handled;
                    }
                }
            }
        }
    }

    // 3. Full-Screen Editor/Viewer Priority
    if let AppMode::Editor | AppMode::Viewer = app.mode {
        let editor_area = if app.current_view == CurrentView::Commit {
            commit_editor_area(app)
        } else {
            let (w, h) = app.terminal_size;
            ratatui::layout::Rect::new(1, 1, w.saturating_sub(2), h.saturating_sub(2))
        };
        if let Some(preview) = &mut app.editor_state {
            if let Some(editor) = &mut preview.editor {
                if key.code == KeyCode::Esc {
                    app.mode = AppMode::Normal;
                    app.editor_state = None;
                    return true;
                }

                // Ctrl+Enter: run the current file (full-screen mode)
                if has_control && key.code == KeyCode::Enter {
                    let remote = app
                        .panes
                        .get(app.focused_pane_index)
                        .and_then(|p| p.current_state())
                        .and_then(|fs| fs.remote_session.clone());
                    if let Some((work_dir, program, args)) =
                        crate::modules::files::get_run_command(&preview.path)
                    {
                        let _ = event_tx.try_send(AppEvent::SpawnTerminal {
                            path: work_dir,
                            new_tab: true,
                            remote,
                            command: Some(format!("{} {}", program, args.join(" "))),
                        });
                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                            "Running: {} {}",
                            program,
                            args.join(" ")
                        )));
                    } else {
                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                            "No run command for: {}",
                            preview
                                .path
                                .extension()
                                .and_then(|e| e.to_str())
                                .map(|e| format!(".{e}"))
                                .unwrap_or_else(|| "unknown".to_string())
                        )));
                    }
                    return true;
                }

                let mut clipboard = app.editor_clipboard.clone();
                let auto_save = app.auto_save;
                let mut mode = app.mode.clone();
                let mut prev_mode = app.previous_mode.clone();

                let handled = handle_generic_editor_shortcuts(
                    key,
                    editor,
                    &mut clipboard,
                    auto_save,
                    &mut mode,
                    &mut prev_mode,
                    &mut app.input,
                    &mut app.replace_buffer,
                    event_tx,
                    &preview.path,
                    evt,
                    editor_area,
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

pub fn handle_editor_mouse(
    me: &MouseEvent,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
) -> bool {
    let (w, h) = app.terminal_size;
    let column = me.column;
    let row = me.row;

    // A. Check for Full-Screen Editor
    if let AppMode::Editor
    | AppMode::Viewer
    | AppMode::EditorSearch
    | AppMode::EditorReplace
    | AppMode::EditorGoToLine = app.mode
    {
        let editor_area = if app.current_view == CurrentView::Commit {
            commit_editor_area(app)
        } else {
            ratatui::layout::Rect::new(1, 1, w.saturating_sub(2), h.saturating_sub(2))
        };
        if let Some(preview) = &mut app.editor_state {
            if let Some(editor) = &mut preview.editor {
                // Header buttons
                if row == 0 {
                    if let MouseEventKind::Down(MouseButton::Left) = me.kind {
                        if column >= w.saturating_sub(10) {
                            app.running = false;
                            return true;
                        } else if column >= w.saturating_sub(20) {
                            app.mode = AppMode::Normal;
                            app.editor_state = None;
                            return true;
                        }
                        return true;
                    }
                }

                let mut clipboard = app.editor_clipboard.clone();
                let handled = handle_text_editor_mouse(
                    me,
                    editor,
                    &mut clipboard,
                    &mut app.mouse_last_click,
                    &mut app.mouse_click_pos,
                    &mut app.mouse_click_count,
                    app.auto_save,
                    editor_area,
                    event_tx,
                    &preview.path,
                );
                app.editor_clipboard = clipboard;
                return handled;
            }
        }
    }

    // B. Check for IDE Mode (Pane Editor)
    if app.current_view == CurrentView::Editor && column >= app.sidebar_width() {
        let sw = app.sidebar_width();
        let pc = app.panes.len();
        if pc == 0 {
            return false;
        }
        let cw = w.saturating_sub(sw);
        let pw = cw / pc as u16;
        if pw == 0 {
            return false;
        }
        let pane_idx = if matches!(me.kind, MouseEventKind::Down(_)) {
            let mut cp = (column.saturating_sub(sw) / pw) as usize;
            if cp >= pc {
                cp = pc - 1;
            }
            app.focused_pane_index = cp;
            app.sidebar_focus = false;
            cp
        } else {
            app.focused_pane_index.min(pc - 1)
        };

        let Some(editor_area) = pane_editor_area(app, pane_idx) else {
            return false;
        };

        if let Some(pane) = app.panes.get_mut(pane_idx) {
            if let Some(fs) = pane.current_state_mut() {
                if let Some(preview) = &mut fs.preview {
                    if let Some(editor) = &mut preview.editor {
                        let mut clipboard = app.editor_clipboard.clone();
                        let handled = handle_text_editor_mouse(
                            me,
                            editor,
                            &mut clipboard,
                            &mut app.mouse_last_click,
                            &mut app.mouse_click_pos,
                            &mut app.mouse_click_count,
                            app.auto_save,
                            editor_area,
                            event_tx,
                            &preview.path,
                        );
                        app.editor_clipboard = clipboard;
                        return handled;
                    }
                }
            }
        }
    }

    false
}

#[allow(clippy::too_many_arguments)]
fn handle_text_editor_mouse(
    me: &MouseEvent,
    editor: &mut dracon_terminal_engine::widgets::TextEditor,
    clipboard: &mut Option<String>,
    mouse_last_click: &mut std::time::Instant,
    mouse_click_pos: &mut (u16, u16),
    mouse_click_count: &mut usize,
    auto_save: bool,
    area: ratatui::layout::Rect,
    event_tx: &mpsc::Sender<AppEvent>,
    path: &std::path::Path,
) -> bool {
    let to_runtime_mouse = |mouse: MouseEvent| -> dracon_terminal_engine::input::event::MouseEvent {
        match dracon_terminal_engine::input::mapping::to_runtime_event(&Event::Mouse(mouse)) {
            dracon_terminal_engine::input::event::Event::Mouse(m) => m,
            _ => unreachable!(),
        }
    };

    match me.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let now = std::time::Instant::now();
            if now.duration_since(*mouse_last_click) < std::time::Duration::from_millis(DOUBLE_CLICK_MS)
                && *mouse_click_pos == (me.column, me.row)
            {
                *mouse_click_count += 1;
            } else {
                *mouse_click_count = 1;
            }

            let rel_row = (me.row as i32 - area.y as i32) as usize;
            let target_row = editor.scroll_row + rel_row;

            match *mouse_click_count {
                2 => {
                    if target_row < editor.lines.len() {
                        let gutter = editor.gutter_width();
                        if me.column >= area.x + gutter as u16 {
                            let rel_col = (me.column - area.x - gutter as u16) as usize;
                            let target_visual = editor.scroll_col + rel_col;
                            let byte_col =
                                editor.get_byte_index_from_visual(target_row, target_visual);
                            editor.select_word_at(target_row, byte_col);
                        }
                    }
                }
                3 => {
                    if target_row < editor.lines.len() {
                        editor.select_line_at(target_row);
                    }
                    *mouse_click_count = 0;
                }
                _ => {
                    editor.handle_mouse_event(to_runtime_mouse(*me), area);
                }
            }
            *mouse_last_click = now;
            *mouse_click_pos = (me.column, me.row);
        }
        MouseEventKind::Down(MouseButton::Middle) => {
            if let Some(text) = dracon_terminal_engine::utils::get_primary_selection_text() {
                editor.insert_string(&text);
                editor.modified = true;
            }
        }
        MouseEventKind::ScrollDown => {
            if me.modifiers.contains(KeyModifiers::CONTROL) {
                if !editor.lines.is_empty() {
                    editor.scroll_row =
                        (editor.scroll_row + 5).min(editor.lines.len().saturating_sub(1));
                }
            } else {
                editor.handle_mouse_event(to_runtime_mouse(*me), area);
            }
        }
        MouseEventKind::ScrollUp => {
            if me.modifiers.contains(KeyModifiers::CONTROL) {
                editor.scroll_row = editor.scroll_row.saturating_sub(5);
            } else {
                editor.handle_mouse_event(to_runtime_mouse(*me), area);
            }
        }
        _ => {
            editor.handle_mouse_event(to_runtime_mouse(*me), area);
        }
    }

    // Sync selection to clipboard
    if let Some(selected_text) = editor.get_selected_text() {
        if selected_text.width() > 1 {
            *clipboard = Some(selected_text.clone());
            dracon_terminal_engine::utils::set_clipboard_text(&selected_text);
        }
    }

    // Auto-save on modification
    if auto_save && editor.modified {
        let _ = event_tx.try_send(AppEvent::SaveFile(path.to_path_buf(), editor.get_content()));
        editor.modified = false;
    }

    true
}

#[allow(clippy::too_many_arguments)]
fn handle_generic_editor_shortcuts(
    key: &dracon_terminal_engine::contracts::KeyEvent,
    editor: &mut dracon_terminal_engine::widgets::TextEditor,
    clipboard: &mut Option<String>,
    auto_save: bool,
    mode: &mut AppMode,
    prev_mode: &mut AppMode,
    input: &mut dracon_terminal_engine::widgets::TextInput,
    replace_buffer: &mut String,
    event_tx: &mpsc::Sender<AppEvent>,
    path: &std::path::Path,
    evt: &Event,
    area: ratatui::layout::Rect,
) -> bool {
    let has_control = key.modifiers.contains(KeyModifiers::CONTROL);

    if has_control && (key.code == KeyCode::Char('s') || key.code == KeyCode::Char('S')) {
        let _ = event_tx.try_send(AppEvent::SaveFile(path.to_path_buf(), editor.get_content()));
        return true;
    }

    if has_control
        && key.modifiers.contains(KeyModifiers::SHIFT)
        && (key.code == KeyCode::Char('s') || key.code == KeyCode::Char('S'))
    {
        *prev_mode = mode.clone();
        *mode = AppMode::SaveAs(path.to_path_buf());
        input.clear();
        return true;
    }

    if has_control
        && (key.code == KeyCode::Char('c')
            || key.code == KeyCode::Char('C')
            || key.code == KeyCode::Insert)
    {
        let content = if let Some(selected) = editor.get_selected_text() {
            selected
        } else {
            editor
                .lines
                .get(editor.cursor_row)
                .cloned()
                .unwrap_or_default()
        };
        *clipboard = Some(content.clone());
        dracon_terminal_engine::utils::set_clipboard_text(&content);
        let _ = event_tx.try_send(AppEvent::StatusMsg("Copied to clipboard".to_string()));
        return true;
    }

    if (has_control && (key.code == KeyCode::Char('x') || key.code == KeyCode::Char('X')))
        || (key.modifiers.contains(KeyModifiers::SHIFT) && key.code == KeyCode::Delete)
    {
        let content = if let Some(selected) = editor.get_selected_text() {
            selected
        } else {
            editor
                .lines
                .get(editor.cursor_row)
                .cloned()
                .unwrap_or_default()
        };
        *clipboard = Some(content.clone());
        dracon_terminal_engine::utils::set_clipboard_text(&content);
        if editor.get_selection_range().is_some() {
            editor.push_history();
            editor.delete_selection();
        } else {
            editor.delete_line(editor.cursor_row);
        }
        let _ = event_tx.try_send(AppEvent::StatusMsg("Cut to clipboard".to_string()));
        if auto_save {
            let _ = event_tx.try_send(AppEvent::SaveFile(path.to_path_buf(), editor.get_content()));
        }
        return true;
    }

    if (has_control && (key.code == KeyCode::Char('v') || key.code == KeyCode::Char('V')))
        || (key.modifiers.contains(KeyModifiers::SHIFT) && key.code == KeyCode::Insert)
    {
        let text_to_paste = clipboard.clone().or_else(dracon_terminal_engine::utils::get_clipboard_text);
        if let Some(text) = text_to_paste {
            editor.insert_string(&text);
            editor.modified = true;
            if auto_save {
                let _ =
                    event_tx.try_send(AppEvent::SaveFile(path.to_path_buf(), editor.get_content()));
                editor.modified = false;
            }
        }
        return true;
    }

    if has_control && !key.modifiers.contains(KeyModifiers::SHIFT) && key.code == KeyCode::Char('z')
    {
        editor.handle_event(&dracon_terminal_engine::input::mapping::to_runtime_event(evt), area);
        return true;
    }
    if has_control
        && (key.code == KeyCode::Char('y')
            || key.code == KeyCode::Char('Y')
            || key.code == KeyCode::Char('Z'))
    {
        editor.handle_event(&dracon_terminal_engine::input::mapping::to_runtime_event(evt), area);
        return true;
    }

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
                let _ = event_tx.try_send(AppEvent::StatusMsg(
                    "Replace: Type term to FIND, then press Enter/Tab".to_string(),
                ));
                return true;
            }
            _ => {}
        }
    }

    if key.code == KeyCode::F(2) {
        let name = path
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("root"))
            .to_string_lossy()
            .to_string();
        *prev_mode = mode.clone();
        *mode = AppMode::Rename;
        input.set_value(name.clone());
        if let Some(idx) = name.rfind('.') {
            input.cursor_position = if idx > 0 { idx } else { name.len() };
        } else {
            input.cursor_position = name.len();
        }
        return true;
    }

    if editor.handle_event(&dracon_terminal_engine::input::mapping::to_runtime_event(evt), area) {
        if auto_save && editor.modified {
            let _ = event_tx.try_send(AppEvent::SaveFile(path.to_path_buf(), editor.get_content()));
            editor.modified = false;
        }
        return true;
    }

    false
}
