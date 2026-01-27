fn handle_event(evt: Event, app: &mut App, event_tx: mpsc::Sender<AppEvent>) -> bool {
    match evt {
        Event::Resize(w, h) => {
            if let Some(until) = app.ignore_resize_until {
                if std::time::Instant::now() < until { return true; }
            }
            app.terminal_size = (w, h);
            return true;
        }
        Event::Key(key) => {  
            let has_control = key.modifiers.contains(KeyModifiers::CONTROL);
            let has_alt = key.modifiers.contains(KeyModifiers::ALT);

            // 1. Full-Screen Editor Priority (Traps all input)
            if let AppMode::Editor = app.mode {
                if let Some(preview) = &mut app.editor_state {
                    if let Some(editor) = &mut preview.editor {
                        if key.code == KeyCode::Esc {
                            app.mode = AppMode::Normal;
                            app.editor_state = None;
                            return true;
                        }
                        if let KeyCode::Char('s') | KeyCode::Char('S') = key.code {
                            if has_control {
                                let _ = event_tx.try_send(AppEvent::SaveFile(preview.path.clone(), editor.get_content()));
                                editor.modified = false;
                                return true;
                            }
                        }
                        if let KeyCode::Char('c') | KeyCode::Char('C') = key.code {
                            if has_control {
                                let line = editor.lines[editor.cursor_row].clone();
                                let mut stdout = std::io::stdout();
                                let _ = terma::visuals::osc::copy_to_clipboard(&mut stdout, &line);
                                let _ = event_tx.try_send(AppEvent::StatusMsg("Copied line to clipboard".to_string()));
                                return true;
                            }
                        }
                        
                        let (w, h) = app.terminal_size;
                        let editor_area = ratatui::layout::Rect::new(0, 0, w, h.saturating_sub(1));
                        if editor.handle_event(&evt, editor_area) {
                            if app.auto_save && editor.modified {
                                let _ = event_tx.try_send(AppEvent::SaveFile(preview.path.clone(), editor.get_content()));
                                editor.modified = false;
                            }
                            return true;
                        }
                        return true; 
                    } else {
                        if key.code == KeyCode::Esc {
                            app.mode = AppMode::Normal;
                            app.editor_state = None;
                            return true;
                        }
                        return true;
                    }
                }
            }

            // 2. Global Shortcuts
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') if has_control => { app.running = false; return true; }
                KeyCode::Char('b') | KeyCode::Char('B') if has_control => { app.show_sidebar = !app.show_sidebar; return true; }
                KeyCode::Char('i') | KeyCode::Char('I') if has_control => {
                    let state = crate::modules::introspection::WorldState::capture(app);
                    if let Ok(json) = serde_json::to_string_pretty(&state) {
                        let _ = std::fs::write("introspection.json", json);
                        app.last_action_msg = Some(("World state dumped to introspection.json".to_string(), std::time::Instant::now()));
                    }
                    return true;
                }
                KeyCode::Char('p') | KeyCode::Char('P') if has_control => { app.toggle_split(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); return true; }
                KeyCode::Char('\') if has_control => { app.toggle_split(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); return true; }
                KeyCode::Char('h') | KeyCode::Char('H') if has_control => { let idx = app.toggle_hidden(); let _ = event_tx.try_send(AppEvent::RefreshFiles(idx)); return true; }
                KeyCode::Char('g') | KeyCode::Char('G') if has_control => { app.mode = AppMode::Settings; app.settings_scroll = 0; return true; }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('o') | KeyCode::Char('O') if has_control => {
                    if let Some(fs) = app.current_file_state() {
                        let _ = event_tx.try_send(AppEvent::SpawnTerminal { path: fs.current_path.clone(), new_tab: true, remote: fs.remote_session.clone(), command: None });
                    }
                    return true;
                }
                KeyCode::Char('t') | KeyCode::Char('T') if has_control => {
                    if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                        if let Some(fs) = pane.current_state() {
                            let new_fs = fs.clone();
                            pane.open_tab(new_fs);
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                        }
                    }
                    return true;
                }
                KeyCode::Char(' ') if has_control => { 
                    app.input.clear(); app.mode = AppMode::CommandPalette; update_commands(app); return true; 
                }
                KeyCode::Left if has_control => {
                    if app.sidebar_focus { app.resize_sidebar(-2); } 
                    else { app.move_to_other_pane(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); }
                    return true;
                }
                KeyCode::Right if has_control => {
                    if app.sidebar_focus { app.resize_sidebar(2); } 
                    else { app.move_to_other_pane(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); }
                    return true;
                }
                _ => {}
            }

            // 3. Modal Handling
            match &app.mode {
                AppMode::CommandPalette => match key.code {
                    KeyCode::Esc => { app.mode = AppMode::Normal; return true; }
                    KeyCode::Enter => { 
                        if let Some(cmd) = app.filtered_commands.get(app.command_index).cloned() { execute_command(cmd.action, app, event_tx.clone()); } 
                        app.mode = AppMode::Normal; app.input.clear(); return true;
                    }
                    _ => { let handled = app.input.handle_event(&evt); if handled { update_commands(app); } return handled; }
                },
                AppMode::AddRemote(idx) => {
                    let idx = *idx;
                    match key.code {
                        KeyCode::Esc => { app.mode = AppMode::Normal; app.input.clear(); return true; }
                        KeyCode::Tab | KeyCode::Enter => {
                            let val = app.input.value.clone();
                            match idx {
                                0 => app.pending_remote.name = val,
                                1 => app.pending_remote.host = val,
                                2 => app.pending_remote.user = val,
                                3 => app.pending_remote.port = val.parse().unwrap_or(22),
                                4 => app.pending_remote.key_path = if val.is_empty() { None } else { Some(std::path::PathBuf::from(val)) },
                                _ => {} 
                            }
                            if idx < 4 {
                                app.mode = AppMode::AddRemote(idx + 1);
                                let next_val = match idx + 1 {
                                    1 => app.pending_remote.host.clone(),
                                    2 => app.pending_remote.user.clone(),
                                    3 => app.pending_remote.port.to_string(),
                                    4 => app.pending_remote.key_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
                                    _ => String::new()
                                };
                                app.input.set_value(next_val);
                            } else {
                                app.remote_bookmarks.push(app.pending_remote.clone()); let _ = crate::config::save_state(app);
                                app.mode = AppMode::Normal; app.input.clear();
                            }
                            return true;
                        }
                        _ => return app.input.handle_event(&evt)
                    }
                },
                AppMode::Header(idx) => {
                    let idx = *idx;
                    match key.code {
                        KeyCode::Esc => { app.mode = AppMode::Normal; return true; }
                        KeyCode::Left => { if idx > 0 { app.mode = AppMode::Header(idx - 1); } return true; }
                        KeyCode::Right => { if idx < 3 { app.mode = AppMode::Header(idx + 1); } return true; }
                        KeyCode::Enter => {
                            match idx {
                                0 => app.mode = AppMode::Settings,
                                1 => if let Some(fs) = app.current_file_state_mut() { navigate_back(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                                2 => if let Some(fs) = app.current_file_state_mut() { navigate_forward(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                                3 => { app.toggle_split(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); }
                                _ => {} 
                            }
                            if let AppMode::Header(_) = app.mode { app.mode = AppMode::Normal; }
                            return true;
                        }
                        _ => return true
                    }
                },
                AppMode::OpenWith(path) => match key.code {
                    KeyCode::Esc => { app.mode = AppMode::Normal; app.input.clear(); return true; }
                    KeyCode::Enter => {
                        let cmd = app.input.value.clone();
                        if !cmd.is_empty() { let _ = event_tx.try_send(AppEvent::SpawnDetached { cmd, args: vec![path.to_string_lossy().to_string()] }); }
                        app.mode = AppMode::Normal; app.input.clear(); return true;
                    }
                    _ => return app.input.handle_event(&evt)
                },
                AppMode::Highlight => {
                    if let KeyCode::Char(c) = key.code {
                        if let Some(digit) = c.to_digit(10) {
                            if digit <= 6 {
                                let color = if digit == 0 { None } else { Some(digit as u8) };
                                if let Some(fs) = app.current_file_state() {
                                    let mut paths = Vec::new();
                                    if !fs.multi_select.is_empty() { for &idx in &fs.multi_select { if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); } } } 
                                    else if let Some(idx) = fs.selected_index { if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); } }
                                    for p in paths { if let Some(col) = color { app.path_colors.insert(p, col); } else { app.path_colors.remove(&p); } }
                                    let _ = crate::config::save_state(app);
                                }
                                app.mode = AppMode::Normal; return true;
                            }
                        }
                    } else if key.code == KeyCode::Esc { app.mode = AppMode::Normal; return true; }
                    return false;
                },
                AppMode::Settings => match key.code {
                    KeyCode::Esc => { app.mode = AppMode::Normal; return true; }
                    KeyCode::Char('1') => { app.settings_section = SettingsSection::Columns; return true; }
                    KeyCode::Char('2') => { app.settings_section = SettingsSection::Tabs; return true; }
                    KeyCode::Char('3') => { app.settings_section = SettingsSection::General; return true; }
                    KeyCode::Char('4') => { app.settings_section = SettingsSection::Remotes; return true; }
                    KeyCode::Char('5') => { app.settings_section = SettingsSection::Shortcuts; return true; }
                    KeyCode::Up => { app.settings_scroll = app.settings_scroll.saturating_sub(1); return true; }
                    KeyCode::Down => { app.settings_scroll = app.settings_scroll.saturating_add(1); return true; }
                    KeyCode::Char('t') if app.settings_section == SettingsSection::General => { app.smart_date = !app.smart_date; return true; } 
                    KeyCode::Char('a') if app.settings_section == SettingsSection::General => { app.auto_save = !app.auto_save; return true; } 
                    _ => return false
                },
                AppMode::NewFile | AppMode::NewFolder | AppMode::Rename | AppMode::Delete => match key.code {
                    KeyCode::Esc => { app.mode = AppMode::Normal; app.input.clear(); return true; }
                    KeyCode::Enter => {
                        let input = app.input.value.clone();
                        if let Some(fs) = app.current_file_state() {
                            let path = fs.current_path.join(&input);
                            match app.mode {
                                AppMode::NewFile => { let _ = event_tx.try_send(AppEvent::CreateFile(path)); }
                                AppMode::NewFolder => { let _ = event_tx.try_send(AppEvent::CreateFolder(path)); }
                                AppMode::Rename => if let Some(idx) = fs.selected_index { if let Some(old) = fs.files.get(idx) { let _ = event_tx.try_send(AppEvent::Rename(old.clone(), old.parent().unwrap().join(&input))); } }
                                AppMode::Delete => {
                                    let ic = input.trim().to_lowercase();
                                    if ic == "y" || ic == "yes" || ic.is_empty() || !app.confirm_delete {
                                        let mut paths = Vec::new();
                                        if !fs.multi_select.is_empty() { for &idx in &fs.multi_select { if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); } } } 
                                        else if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx) { paths.push(path.clone()); } }
                                        for p in paths { let _ = event_tx.try_send(AppEvent::Delete(p)); }
                                    }
                                }
                                _ => {} 
                            }
                        }
                        app.mode = AppMode::Normal; app.input.clear(); return true;
                    }
                    _ => return app.input.handle_event(&evt)
                },
                _ => {
                    // Standard Navigation & Actions
                    if key.code == KeyCode::Esc {
                        app.mode = AppMode::Normal;
                        if let Some(fs) = app.current_file_state_mut() { fs.multi_select.clear(); fs.selection_anchor = None; if !fs.search_filter.is_empty() { fs.search_filter.clear(); fs.selected_index = Some(0); *fs.table_state.offset_mut() = 0; let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } 
                        return true;
                    }
                    match key.code {
                        KeyCode::Char('c') if has_control => { if let Some(fs) = app.current_file_state() { if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx) { app.clipboard = Some((path.clone(), crate::app::ClipboardOp::Copy)); } } } return true; }
                        KeyCode::Char('x') if has_control => { if let Some(fs) = app.current_file_state() { if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx) { app.clipboard = Some((path.clone(), crate::app::ClipboardOp::Cut)); } } } return true; }
                        KeyCode::Char('v') if has_control => { if let Some((src, op)) = app.clipboard.clone() { if let Some(fs) = app.current_file_state() { let dest = fs.current_path.join(src.file_name().unwrap()); match op {
                            crate::app::ClipboardOp::Copy => { let _ = event_tx.try_send(AppEvent::Copy(src, dest)); }
                            crate::app::ClipboardOp::Cut => { let _ = event_tx.try_send(AppEvent::Rename(src, dest)); app.clipboard = None; }
                        } } } return true; }
                        KeyCode::Char('a') if has_control => { if let Some(fs) = app.current_file_state_mut() { fs.multi_select = (0..fs.files.len()).collect(); } return true; }
                        KeyCode::Char('z') if has_control => {
                            if let Some(action) = app.undo_stack.pop() {
                                match action.clone() {
                                    crate::app::UndoAction::Rename(old, new) | crate::app::UndoAction::Move(old, new) => { let _ = std::fs::rename(&new, &old); app.redo_stack.push(action); }
                                    crate::app::UndoAction::Copy(_src, dest) => { let _ = if dest.is_dir() { std::fs::remove_dir_all(&dest) } else { std::fs::remove_file(&dest) }; app.redo_stack.push(action); }
                                    _ => {} 
                                }
                                for i in 0..app.panes.len() { let _ = event_tx.try_send(AppEvent::RefreshFiles(i)); }
                            } else if let Some(fs) = app.current_file_state_mut() { if !fs.search_filter.is_empty() { fs.search_filter.clear(); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } }
                            return true;
                        }
                        KeyCode::Char('y') if has_control => {
                            if let Some(action) = app.redo_stack.pop() {
                                match action.clone() {
                                    crate::app::UndoAction::Rename(old, new) | crate::app::UndoAction::Move(old, new) => { let _ = std::fs::rename(&old, &new); app.undo_stack.push(action); }
                                    crate::app::UndoAction::Copy(src, dest) => { let _ = crate::modules::files::copy_recursive(&src, &dest); app.undo_stack.push(action); }
                                    _ => {} 
                                }
                                for i in 0..app.panes.len() { let _ = event_tx.try_send(AppEvent::RefreshFiles(i)); }
                            }
                            return true;
                        }
                        KeyCode::Char('f') if has_control => { app.mode = AppMode::Search; return true; }
                        KeyCode::Char(' ') => { if let Some(fs) = app.current_file_state() { if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx).cloned() { if path.is_dir() { app.mode = AppMode::Properties; } else { let target_pane = if app.focused_pane_index == 0 { 1 } else { 0 }; let _ = event_tx.try_send(AppEvent::PreviewRequested(target_pane, path)); } } } } return true; } 
                        KeyCode::Up => { if let Some(fs) = app.current_file_state_mut() { if let Some(sel) = fs.selected_index { if sel > 0 { fs.selected_index = Some(sel - 1); fs.table_state.select(Some(sel - 1)); } } else { fs.selected_index = Some(0); fs.table_state.select(Some(0)); } } return true; }
                        KeyCode::Down => { if let Some(fs) = app.current_file_state_mut() { if let Some(sel) = fs.selected_index { if sel < fs.files.len().saturating_sub(1) { fs.selected_index = Some(sel + 1); fs.table_state.select(Some(sel + 1)); } } else { fs.selected_index = Some(0); fs.table_state.select(Some(0)); } } return true; }
                        KeyCode::Left => { if app.panes.len() > 1 && app.focused_pane_index > 0 { app.focused_pane_index -= 1; } else { app.sidebar_focus = true; } return true; }
                        KeyCode::Right => { if app.sidebar_focus { app.sidebar_focus = false; } else if app.panes.len() > 1 && app.focused_pane_index < app.panes.len() - 1 { app.focused_pane_index += 1; } return true; }
                        KeyCode::Enter => {
                            let mut navigate_to = None;
                            if let Some(fs) = app.current_file_state() {
                                if let Some(idx) = fs.selected_index {
                                    if let Some(path) = fs.files.get(idx) {
                                        if path.is_dir() { navigate_to = Some(path.clone()); }
                                        else { terma::utils::spawn_detached("xdg-open", vec![path.to_string_lossy().to_string()]); }
                                    }
                                }
                            }
                            if let Some(p) = navigate_to { if let Some(fs) = app.current_file_state_mut() { fs.current_path = p.clone(); fs.selected_index = Some(0); fs.multi_select.clear(); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, p); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } }
                            return true;
                        }
                        KeyCode::F(6) => {
                            if let Some(fs) = app.current_file_state() {
                                if let Some(p) = fs.selected_index.and_then(|idx| fs.files.get(idx)) {
                                    app.mode = AppMode::Rename;
                                    app.input.set_value(p.file_name().unwrap().to_string_lossy().to_string());
                                    app.rename_selected = true;
                                    return true;
                                }
                            }
                            return false;
                        }
                        KeyCode::Delete => {
                            if let Some(fs) = app.current_file_state() {
                                if fs.selected_index.is_some() {
                                    if app.confirm_delete { app.mode = AppMode::Delete; }
                                    else {
                                        let mut paths = Vec::new();
                                        if !fs.multi_select.is_empty() { for &idx in &fs.multi_select { if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); } } } 
                                        else if let Some(idx) = fs.selected_index { if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); } }
                                        for p in paths { let _ = event_tx.try_send(AppEvent::Delete(p)); }
                                    }
                                    return true;
                                }
                            }
                            return false;
                        }
                        KeyCode::Char('~') => {
                            if let Some(fs) = app.current_file_state_mut() {
                                if let Some(home) = dirs::home_dir() {
                                    fs.current_path = home.clone(); fs.selected_index = Some(0); fs.multi_select.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, home);
                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                    return true;
                                }
                            }
                            return false;
                        }
                        KeyCode::Char(c) if key.modifiers.is_empty() => { if let Some(fs) = app.current_file_state_mut() { fs.search_filter.push(c); fs.selected_index = Some(0); *fs.table_state.offset_mut() = 0; let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } return true; } 
                        KeyCode::Backspace => { if let Some(fs) = app.current_file_state_mut() { if !fs.search_filter.is_empty() { fs.search_filter.pop(); fs.selected_index = Some(0); *fs.table_state.offset_mut() = 0; let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } else if let Some(parent) = fs.current_path.parent() { let p = parent.to_path_buf(); fs.current_path = p.clone(); fs.selected_index = Some(0); fs.multi_select.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, p); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } return true; } 
                        _ => return false
                    }
                }
            }
        }
        Event::Mouse(me) => {
            let column = me.column;
            let row = me.row;
            let (w, h) = app.terminal_size;

            // 0. Modal Handling
            match app.mode.clone() {
                AppMode::Highlight => if let MouseEventKind::Down(_) = me.kind {
                    let area_w = 34; let area_h = 5; let area_x = (w.saturating_sub(area_w)) / 2; let area_y = (h.saturating_sub(area_h)) / 2;
                    if column >= area_x && column < area_x + area_w && row >= area_y && row < area_y + area_h {
                        let rel_x = column.saturating_sub(area_x + 3);
                        if row >= area_y + 2 && row <= area_y + 3 {
                            let colors = [1, 2, 3, 4, 5, 6, 0];
                            if let Some(&color_code) = colors.get((rel_x / 4) as usize) {
                                let color = if color_code == 0 { None } else { Some(color_code as u8) };
                                if let Some(fs) = app.current_file_state() {
                                    let mut paths = Vec::new();
                                    if !fs.multi_select.is_empty() { for &idx in &fs.multi_select { if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); } } } 
                                    else if let Some(idx) = fs.selected_index { if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); } }
                                    for p in paths { if let Some(col) = color { app.path_colors.insert(p, col); } else { app.path_colors.remove(&p); } }
                                    let _ = crate::config::save_state(app);
                                }
                                app.mode = AppMode::Normal;
                            }
                        }
                    } else { app.mode = AppMode::Normal; }
                    return true;
                },
                AppMode::ContextMenu { x, y, target, actions } => if let MouseEventKind::Down(_) = me.kind {
                    let (mw, mh) = (25, actions.len() as u16 + 2);
                    let (mut dx, mut dy) = (x, y);
                    if dx + mw > w { dx = w.saturating_sub(mw); }
                    if dy + mh > h { dy = h.saturating_sub(mh); }
                    if column >= dx && column < dx + mw && row >= dy && row < dy + mh {
                        if row > dy && row < dy + mh - 1 { if let Some(action) = actions.get((row - dy - 1) as usize) { handle_context_menu_action(action, &target, app, event_tx.clone()); } }
                    } else { app.mode = AppMode::Normal; }
                    return true;
                },
                AppMode::Settings | AppMode::ImportServers | AppMode::NewFile | AppMode::NewFolder | AppMode::Rename | AppMode::Delete | AppMode::Properties | AppMode::CommandPalette | AppMode::AddRemote(_) | AppMode::OpenWith(_) | AppMode::Editor => {
                    match me.kind {
                        MouseEventKind::Down(_) => {
                            if let AppMode::Editor = app.mode {
                                if let Some(preview) = &mut app.editor_state {
                                    if let Some(editor) = &mut preview.editor {
                                        let editor_area = ratatui::layout::Rect::new(0, 0, w, h.saturating_sub(1));
                                        if editor_area.contains(ratatui::layout::Position { x: column, y: row }) {
                                            editor.handle_mouse_event(me, editor_area);
                                        }
                                    }
                                }
                                return true;
                            }
                            let (aw, ah) = match app.mode { AppMode::Settings => ((w as f32 * 0.8) as u16, (h as f32 * 0.8) as u16), AppMode::Properties => ((w as f32 * 0.5) as u16, (h as f32 * 0.5) as u16), AppMode::CommandPalette | AppMode::AddRemote(_) | AppMode::OpenWith(_) => ((w as f32 * 0.6) as u16, (h as f32 * 0.2) as u16), _ => ((w as f32 * 0.4) as u16, (h as f32 * 0.1) as u16) };
                            let (ax, ay) = ((w - aw) / 2, (h - ah) / 2);
                            if column >= ax && column < ax + aw && row >= ay && row < ay + ah {
                                if let AppMode::Settings = app.mode {
                                    let inner_x = ax + 1; let inner_y = ay + 1;
                                    if column < inner_x + 15 {
                                        let rel_y = row.saturating_sub(inner_y);
                                        match rel_y {
                                            0 => app.settings_section = SettingsSection::Columns,
                                            1 => app.settings_section = SettingsSection::Tabs,
                                            2 => app.settings_section = SettingsSection::General,
                                            3 => app.settings_section = SettingsSection::Remotes,
                                            4 => app.settings_section = SettingsSection::Shortcuts,
                                            _ => {} 
                                        }
                                        app.settings_scroll = 0; // Reset scroll when switching sections
                                    } else if app.settings_section == SettingsSection::General {
                                        let rel_y = row.saturating_sub(inner_y + 1);
                                        match rel_y {
                                            0 => app.default_show_hidden = !app.default_show_hidden,
                                            1 => app.confirm_delete = !app.confirm_delete,
                                            2 => app.smart_date = !app.smart_date,
                                            3 => app.auto_save = !app.auto_save,
                                            4 => app.icon_mode = match app.icon_mode {
                                                IconMode::Nerd => IconMode::Unicode,
                                                IconMode::Unicode => IconMode::ASCII,
                                                IconMode::ASCII => IconMode::Nerd
                                            },
                                            _ => {} 
                                        }
                                    } else if app.settings_section == SettingsSection::Columns {
                                        if row >= inner_y && row < inner_y + 3 {
                                            let cx = column.saturating_sub(inner_x + 15);
                                            if cx < 12 { app.settings_target = SettingsTarget::SingleMode; } else if cx < 25 { app.settings_target = SettingsTarget::SplitMode; }
                                        } else if row >= inner_y + 4 {
                                            let ry = row.saturating_sub(inner_y + 4);
                                            match ry {
                                                0 => app.toggle_column(crate::app::FileColumn::Extension),
                                                1 => app.toggle_column(crate::app::FileColumn::Size),
                                                2 => app.toggle_column(crate::app::FileColumn::Modified),
                                                3 => app.toggle_column(crate::app::FileColumn::Created),
                                                4 => app.toggle_column(crate::app::FileColumn::Permissions),
                                                _ => {} 
                                            }
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                        }
                                    }
                                }
                            } else { app.mode = AppMode::Normal; app.input.clear(); }
                        }
                        MouseEventKind::ScrollUp => {
                            if let AppMode::Editor = app.mode {
                                if let Some(preview) = &mut app.editor_state { if let Some(editor) = &mut preview.editor { editor.handle_mouse_event(me, ratatui::layout::Rect::new(0, 0, w, h.saturating_sub(1))); } }
                            } else if let AppMode::Settings = app.mode { app.settings_scroll = app.settings_scroll.saturating_sub(2); }
                        }
                        MouseEventKind::ScrollDown => {
                            if let AppMode::Editor = app.mode {
                                if let Some(preview) = &mut app.editor_state { if let Some(editor) = &mut preview.editor { editor.handle_mouse_event(me, ratatui::layout::Rect::new(0, 0, w, h.saturating_sub(1))); } }
                            } else if let AppMode::Settings = app.mode { app.settings_scroll = app.settings_scroll.saturating_add(2); }
                        }
                        _ => {} 
                    }
                    return true;
                }
                _ => {} 
            }

            match me.kind {
                MouseEventKind::Down(button) => {
                    let sw = app.sidebar_width();
                    
                    // Header Icons
                    if row == 0 {
                        if let Some((_, action_id)) = app.header_icon_bounds.iter().find(|(r, _)| column >= r.x && column < r.x + r.width && row == r.y) {
                            match action_id.as_str() {
                                "back" => if let Some(fs) = app.current_file_state_mut() { navigate_back(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                                "forward" => if let Some(fs) = app.current_file_state_mut() { navigate_forward(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                                                                "split" => { app.toggle_split(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); }
                                                                                                "burger" => { app.mode = AppMode::Settings; app.settings_scroll = 0; }
                                                                                                _ => {} 
                                                                                            }
                                                                                            return true;                        }
                    }

                    // Tabs
                    if row == 0 {
                        if let Some((_, p_idx, t_idx)) = app.tab_bounds.iter().find(|(r, _, _)| r.contains(ratatui::layout::Position { x: column, y: row })).cloned() {
                            if button == MouseButton::Left { if let Some(p) = app.panes.get_mut(p_idx) { p.active_tab_index = t_idx; app.focused_pane_index = p_idx; let _ = event_tx.try_send(AppEvent::RefreshFiles(p_idx)); } } 
                            else if button == MouseButton::Right { if let Some(p) = app.panes.get_mut(p_idx) { if p.tabs.len() > 1 { p.tabs.remove(t_idx); if p.active_tab_index >= p.tabs.len() { p.active_tab_index = p.tabs.len() - 1; } let _ = event_tx.try_send(AppEvent::RefreshFiles(p_idx)); } } } 
                            return true;
                        }
                    }

                    // Breadcrumbs
                    for (p_idx, pane) in app.panes.iter_mut().enumerate() {
                        if let Some(fs) = pane.current_state_mut() {
                            if let Some(path) = fs.breadcrumb_bounds.iter().find(|(r, _)| r.contains(ratatui::layout::Position { x: column, y: row })).map(|(_, p)| p.clone()) {
                                if button == MouseButton::Middle {
                                    let mut nfs = fs.clone(); nfs.current_path = path.clone(); nfs.selected_index = Some(0); nfs.search_filter.clear(); *nfs.table_state.offset_mut() = 0; nfs.history = vec![path]; nfs.history_index = 0;
                                    pane.open_tab(nfs);
                                } else {
                                    fs.current_path = path.clone(); fs.selected_index = Some(0); fs.multi_select.clear(); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path);
                                }
                                let _ = event_tx.try_send(AppEvent::RefreshFiles(p_idx)); app.focused_pane_index = p_idx; app.sidebar_focus = false; return true;
                            }
                        }
                    }

                    // Pane focus & Sorting
                    if column >= sw {
                        let cw = w.saturating_sub(sw); let pc = app.panes.len();
                        let pw = if pc > 0 { cw / pc as u16 } else { cw };
                        let cp = (column.saturating_sub(sw) / pw) as usize;
                        if cp < pc {
                            if row == 1 || row == 2 {
                                if let Some(fs) = app.panes.get_mut(cp).and_then(|p| p.current_state_mut()) {
                                    for (r, col) in &fs.column_bounds {
                                        if column >= r.x && column < r.x + r.width + 1 {
                                            if fs.sort_column == *col { fs.sort_ascending = !fs.sort_ascending; } else { fs.sort_column = *col; fs.sort_ascending = true; }
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(cp)); return true;
                                        }
                                    }
                                }
                            }
                            app.focused_pane_index = cp; app.sidebar_focus = false; 
                        }
                    }

                    if column < sw {
                        app.sidebar_focus = true; app.drag_start_pos = Some((column, row));
                        if let Some(b) = app.sidebar_bounds.iter().find(|b| b.y == row).cloned() {
                            app.sidebar_index = b.index; if let SidebarTarget::Favorite(ref p) = b.target { app.drag_source = Some(p.clone()); }
                            if button == MouseButton::Right {
                                let t = match &b.target {
                                    SidebarTarget::Favorite(p) => Some(ContextMenuTarget::SidebarFavorite(p.clone())),
                                    SidebarTarget::Remote(i) => Some(ContextMenuTarget::SidebarRemote(*i)),
                                    SidebarTarget::Storage(i) => Some(ContextMenuTarget::SidebarStorage(*i)),
                                    _ => None
                                };
                                if let Some(target) = t { let actions = get_context_menu_actions(&target, app); app.mode = AppMode::ContextMenu { x: column, y: row, target, actions }; }
                            }
                        }
                        return true;
                    }
                    
                    if row >= 3 {
                        let idx = fs_mouse_index(row, app);
                        let mut sp = None; let mut is_dir = false;
                        let has_mods = me.modifiers.contains(KeyModifiers::SHIFT) || me.modifiers.contains(KeyModifiers::CONTROL);
                        
                        if let Some(fs) = app.current_file_state_mut() {
                            if idx < fs.files.len() {
                                if fs.files[idx].to_string_lossy() == "__DIVIDER__" { return true; } 
                                if button == MouseButton::Left {
                                    if me.modifiers.contains(KeyModifiers::CONTROL) { if fs.multi_select.contains(&idx) { fs.multi_select.remove(&idx); } else { fs.multi_select.insert(idx); } fs.selected_index = Some(idx); fs.table_state.select(Some(idx)); }
                                    else if me.modifiers.contains(KeyModifiers::SHIFT) { let anchor = fs.selection_anchor.unwrap_or(fs.selected_index.unwrap_or(0)); fs.multi_select.clear(); for i in std::cmp::min(anchor, idx)..=std::cmp::max(anchor, idx) { fs.multi_select.insert(i); } fs.selected_index = Some(idx); fs.table_state.select(Some(idx)); }
                                    else { fs.multi_select.clear(); fs.selection_anchor = Some(idx); fs.selected_index = Some(idx); fs.table_state.select(Some(idx)); }
                                }
                                else if !fs.multi_select.contains(&idx) { fs.multi_select.clear(); fs.selected_index = Some(idx); fs.table_state.select(Some(idx)); }
                                let p = fs.files[idx].clone(); is_dir = fs.metadata.get(&p).map(|m| m.is_dir).unwrap_or(false); sp = Some(p);
                            } else if button == MouseButton::Left && !has_mods { fs.selected_index = None; fs.table_state.select(None); fs.multi_select.clear(); fs.selection_anchor = None; }
                            else if button == MouseButton::Right { let target = ContextMenuTarget::EmptySpace; let actions = get_context_menu_actions(&target, app); app.mode = AppMode::ContextMenu { x: column, y: row, target, actions }; return true; } 
                        }
                        if let Some(path) = sp {
                            if button == MouseButton::Right { let target = if is_dir { ContextMenuTarget::Folder(idx) } else { ContextMenuTarget::File(idx) }; let actions = get_context_menu_actions(&target, app); app.mode = AppMode::ContextMenu { x: column, y: row, target, actions }; return true; }
                            if button == MouseButton::Middle {
                                if is_dir { if let Some(p) = app.panes.get_mut(app.focused_pane_index) { if let Some(fs) = p.current_state() { let mut nfs = fs.clone(); nfs.current_path = path.clone(); nfs.selected_index = Some(0); nfs.search_filter.clear(); *nfs.table_state.offset_mut() = 0; nfs.history = vec![path.clone()]; nfs.history_index = 0; p.open_tab(nfs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } } 
                                else { let _ = event_tx.try_send(AppEvent::PreviewRequested(if app.focused_pane_index == 0 { 1 } else { 0 }, path.clone())); } 
                                return true;
                            }
                            app.drag_source = Some(path.clone()); app.drag_start_pos = Some((column, row));
                            if button == MouseButton::Left && app.mouse_last_click.elapsed() < Duration::from_millis(500) && app.mouse_click_pos == (column, row) {
                                if path.is_dir() { if let Some(fs) = app.current_file_state_mut() { fs.current_path = path.clone(); fs.selected_index = Some(0); fs.multi_select.clear(); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } 
                                else { terma::utils::spawn_detached("xdg-open", vec![path.to_string_lossy().to_string()]); } 
                            } 
                            app.mouse_last_click = std::time::Instant::now(); app.mouse_click_pos = (column, row);
                        }
                    } else if row == h.saturating_sub(1) && column < 9 { app.running = false; return true; }
                }
                MouseEventKind::Up(_) => {
                    if app.is_resizing_sidebar { app.is_resizing_sidebar = false; let _ = crate::config::save_state(app); return true; }
                    if app.is_dragging {
                        if let Some((source, target)) = app.drag_source.take().zip(app.hovered_drop_target.take()) {
                            match target {
                                DropTarget::ImportServers | DropTarget::RemotesHeader => if source.extension().map(|e| e == "toml").unwrap_or(false) { let _ = app.import_servers(source); let _ = crate::config::save_state(app); } 
                                DropTarget::Favorites => if source.is_dir() && !app.starred.contains(&source) { app.starred.push(source); let _ = crate::config::save_state(app); }
                                DropTarget::Pane(t_idx) => if let Some(dest) = app.panes.get(t_idx).and_then(|p| p.current_state()).map(|fs| fs.current_path.join(source.file_name().unwrap())) { if me.modifiers.contains(KeyModifiers::SHIFT) { let _ = event_tx.try_send(AppEvent::Copy(source, dest)); } else { let _ = event_tx.try_send(AppEvent::Rename(source, dest)); } }
                                _ => {} 
                            }
                        }
                    } else if column < app.sidebar_width() {
                        if let Some(b) = app.sidebar_bounds.iter().find(|b| b.y == row) {
                            match &b.target {
                                SidebarTarget::Header(h) if h == "REMOTES" => { app.mode = AppMode::ImportServers; app.input.set_value("servers.toml".to_string()); }
                                SidebarTarget::Favorite(p) => { let p = p.clone(); if let Some(fs) = app.current_file_state_mut() { fs.current_path = p.clone(); fs.remote_session = None; fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, p); } let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                                SidebarTarget::Storage(idx) => if let Some(disk) = app.system_state.disks.get(*idx) { if !disk.is_mounted { let dev = disk.device.clone(); let tx = event_tx.clone(); let p_idx = app.focused_pane_index; tokio::spawn(async move { if let Ok(out) = std::process::Command::new("udisksctl").arg("mount").arg("-b").arg(&dev).output() { if String::from_utf8_lossy(&out.stdout).contains("Mounted") { tokio::time::sleep(Duration::from_millis(200)).await; let _ = tx.send(AppEvent::RefreshFiles(p_idx)).await; } } }); } else { let p = std::path::PathBuf::from(&disk.name); if let Some(fs) = app.current_file_state_mut() { fs.current_path = p.clone(); fs.remote_session = None; fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, p); } let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } 
                                SidebarTarget::Remote(idx) => execute_command(crate::app::CommandAction::ConnectToRemote(*idx), app, event_tx.clone()),
                                _ => {} 
                            }
                        }
                    }
                    app.is_dragging = false; app.drag_start_pos = None; app.drag_source = None; app.hovered_drop_target = None;
                }
                MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                    app.mouse_pos = (column, row);
                    if app.is_resizing_sidebar { app.sidebar_width_percent = (column as f32 / w as f32 * 100.0) as u16; app.sidebar_width_percent = app.sidebar_width_percent.clamp(5, 50); return true; }
                    if let Some((sx, sy)) = app.drag_start_pos { if ((column as i16 - sx as i16).pow(2) + (row as i16 - sy as i16).pow(2)) as f32 >= 1.0 { app.is_dragging = true; } }
                    if app.is_dragging {
                        let sw = app.sidebar_width();
                        if let Some((sx, _)) = app.drag_start_pos { if sx < sw { if let Some(src) = app.drag_source.clone() { if let Some(h) = app.sidebar_bounds.iter().find(|b| b.y == row) { if let SidebarTarget::Favorite(t) = &h.target { if &src != t { if let Some(si) = app.starred.iter().position(|p| p == &src) { if let Some(ei) = app.starred.iter().position(|p| p == t) { let item = app.starred.remove(si); app.starred.insert(ei, item); app.sidebar_index = h.index; } } } } } } } } 
                        if app.mode == AppMode::ImportServers { let (aw, ah) = ((w as f32 * 0.6) as u16, (h as f32 * 0.2) as u16); let (ax, ay) = ((w - aw) / 2, (h - ah) / 2); if column >= ax && column < ax + aw && row >= ay && row < ay + ah { app.hovered_drop_target = Some(DropTarget::ImportServers); } else { app.hovered_drop_target = None; } } 
                        else if column < sw { if let Some(b) = app.sidebar_bounds.iter().find(|b| b.y == row) { if let SidebarTarget::Header(h) = &b.target { if h == "REMOTES" { app.hovered_drop_target = Some(DropTarget::RemotesHeader); } else { app.hovered_drop_target = Some(DropTarget::Favorites); } } else { app.hovered_drop_target = Some(DropTarget::Favorites); } } else { app.hovered_drop_target = Some(DropTarget::Favorites); } } 
                        else { let cw = w.saturating_sub(sw); let pc = app.panes.len(); if pc > 1 { let hi = (column.saturating_sub(sw) / (cw / pc as u16)) as usize; if hi < pc && hi != app.focused_pane_index { app.hovered_drop_target = Some(DropTarget::Pane(hi)); } else { app.hovered_drop_target = None; } } else { app.hovered_drop_target = None; } } 
                    }
                    return true;
                }
                MouseEventKind::ScrollUp => {
                    if let AppMode::Settings = app.mode {
                        app.settings_scroll = app.settings_scroll.saturating_sub(2);
                    } else if let Some(fs) = app.current_file_state_mut() {
                        let new_offset = fs.table_state.offset().saturating_sub(3);
                        *fs.table_state.offset_mut() = new_offset;
                    }
                    return true;
                } 
                MouseEventKind::ScrollDown => {
                    if let AppMode::Settings = app.mode {
                        app.settings_scroll = app.settings_scroll.saturating_add(2);
                    } else if let Some(fs) = app.current_file_state_mut() {
                        let max_offset = fs.files.len().saturating_sub(fs.view_height.saturating_sub(4));
                        let new_offset = (fs.table_state.offset() + 3).min(max_offset);
                        *fs.table_state.offset_mut() = new_offset;
                    }
                    return true;
                } 
                _ => {} 
            }
        }
        Event::Paste(text) => {
            if let AppMode::Editor = app.mode {
                if let Some(preview) = &mut app.editor_state {
                    if let Some(editor) = &mut preview.editor {
                        for c in text.chars() {
                            editor.handle_event(&Event::Key(terma::input::event::KeyEvent {
                                code: KeyCode::Char(c),
                                modifiers: terma::input::event::KeyModifiers::empty(),
                                kind: terma::input::event::KeyEventKind::Press,
                            }), ratatui::layout::Rect::new(0, 0, app.terminal_size.0, app.terminal_size.1));
                        }
                        if app.auto_save {
                            let _ = event_tx.try_send(AppEvent::SaveFile(preview.path.clone(), editor.get_content()));
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
