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
            let _has_alt = key.modifiers.contains(KeyModifiers::ALT);

            if let AppMode::Engage = app.mode {
                if key.code == KeyCode::Char('q') && has_control {
                    app.running = false;
                    return true;
                }
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
                        
                        let (w, h) = app.terminal_size;
                        let area = ratatui::layout::Rect::new(0, 0, w, h);
                        let block = ratatui::widgets::Block::default().borders(ratatui::widgets::Borders::ALL);
                        let editor_area = block.inner(area);
                        
                        if editor.handle_event(&evt, editor_area) {
                            if app.auto_save && editor.modified {
                                let _ = event_tx.try_send(AppEvent::SaveFile(preview.path.clone(), editor.get_content()));
                                editor.modified = false;
                            }
                            return true;
                        }
                        return true; 
                    } else {
                        if key.code == KeyCode::Esc { app.mode = AppMode::Normal; app.editor_state = None; }
                        return true;
                    }
                }
            }

            match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') if has_control => { app.running = false; return true; }
                KeyCode::Char('b') | KeyCode::Char('B') if has_control => { app.show_sidebar = !app.show_sidebar; return true; }
                KeyCode::Char('p') | KeyCode::Char('P') if has_control => { app.toggle_split(); return true; }
                KeyCode::Char('m') | KeyCode::Char('M') if has_control => { app.current_view = if app.current_view == CurrentView::Processes { CurrentView::Files } else { CurrentView::Processes }; return true; }
                KeyCode::Char('g') | KeyCode::Char('G') if has_control => { app.mode = AppMode::Settings; app.settings_scroll = 0; return true; }
                KeyCode::Char(' ') if has_control => { app.input.clear(); app.mode = AppMode::CommandPalette; return true; }
                _ => {}
            }

            match &app.mode {
                AppMode::CommandPalette => match key.code {
                    KeyCode::Esc => { app.mode = AppMode::Normal; return true; }
                    KeyCode::Enter => { 
                        if let Some(cmd) = app.filtered_commands.get(app.command_index).cloned() { execute_command(cmd.action, app, event_tx.clone()); } 
                        app.mode = AppMode::Normal; app.input.clear(); return true;
                    }
                    _ => return app.input.handle_event(&evt)
                },
                AppMode::Settings => match key.code {
                    KeyCode::Esc => { app.mode = AppMode::Normal; return true; }
                    KeyCode::Char('1') => { app.settings_section = SettingsSection::Columns; return true; }
                    KeyCode::Char('2') => { app.settings_section = SettingsSection::Tabs; return true; }
                    KeyCode::Char('3') => { app.settings_section = SettingsSection::General; return true; }
                    KeyCode::Up => { app.settings_scroll = app.settings_scroll.saturating_sub(1); return true; }
                    KeyCode::Down => { app.settings_scroll = app.settings_scroll.saturating_add(1); return true; }
                    _ => return false
                },
                _ => {
                    if key.code == KeyCode::Esc {
                        app.mode = AppMode::Normal;
                        if app.current_view == CurrentView::Processes { app.current_view = CurrentView::Files; }
                        return true;
                    }

                    if app.current_view == CurrentView::Processes {
                        match key.code {
                            KeyCode::Up => { app.move_process_up(); return true; }
                            KeyCode::Down => { app.move_process_down(); return true; }
                            KeyCode::Char('1') => { app.monitor_subview = MonitorSubview::Overview; return true; }
                            KeyCode::Char('2') => { app.monitor_subview = MonitorSubview::Applications; return true; }
                            KeyCode::Char('3') => { app.monitor_subview = MonitorSubview::Processes; return true; }
                            KeyCode::Char(c) if !has_control => { app.process_search_filter.push(c); app.process_selected_idx = Some(0); app.apply_process_sort(); return true; }
                            KeyCode::Backspace => { app.process_search_filter.pop(); app.process_selected_idx = Some(0); app.apply_process_sort(); return true; }
                            _ => {}
                        }
                    }

                    match key.code {
                        KeyCode::Up => { if let Some(fs) = app.current_file_state_mut() { if let Some(sel) = fs.selected_index { if sel > 0 { fs.selected_index = Some(sel - 1); fs.table_state.select(Some(sel - 1)); } } } return true; }
                        KeyCode::Down => { if let Some(fs) = app.current_file_state_mut() { if let Some(sel) = fs.selected_index { if sel < fs.files.len().saturating_sub(1) { fs.selected_index = Some(sel + 1); fs.table_state.select(Some(sel + 1)); } } } return true; }
                        KeyCode::Left => { if app.focused_pane_index > 0 { app.focused_pane_index -= 1; } else { app.sidebar_focus = true; } return true; }
                        KeyCode::Right => { if app.sidebar_focus { app.sidebar_focus = false; } else if app.focused_pane_index < app.panes.len() - 1 { app.focused_pane_index += 1; } return true; }
                        KeyCode::Enter => {
                            if let Some(fs) = app.current_file_state() {
                                if let Some(idx) = fs.selected_index {
                                    if let Some(path) = fs.files.get(idx) {
                                        if path.is_dir() { 
                                            let p = path.clone();
                                            if let Some(fs_mut) = app.current_file_state_mut() { fs_mut.current_path = p; fs_mut.selected_index = Some(0); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                                        } else { spawn_detached("xdg-open", vec![&path.to_string_lossy()]); }
                                    }
                                }
                            }
                            return true;
                        }
                        _ => return false
                    }
                }
            }
        }
        Event::Mouse(me) => {
            let column = me.column;
            let row = me.row;
            let (w, h) = app.terminal_size;

            match me.kind {
                MouseEventKind::Down(button) => {
                    let sw = app.sidebar_width();
                    
                    if app.current_view == CurrentView::Processes {
                        for (rect, view) in &app.monitor_subview_bounds {
                            if rect.contains(ratatui::layout::Position { x: column, y: row }) {
                                app.monitor_subview = *view;
                                app.process_search_filter.clear();
                                return true;
                            }
                        }
                        
                        match app.monitor_subview {
                            MonitorSubview::Processes | MonitorSubview::Applications => {
                                for (rect, col) in &app.process_column_bounds {
                                    if column >= rect.x && column < rect.x + rect.width && row == rect.y {
                                        app.sort_processes(*col);
                                        return true;
                                    }
                                }
                                if row >= 6 {
                                    let table_row = (row as usize).saturating_sub(6) + app.process_table_state.offset();
                                    let proc_count = if app.monitor_subview == MonitorSubview::Processes {
                                        app.system_state.processes.len()
                                    } else {
                                        let current_user = std::env::var("USER").unwrap_or_else(|_| "dracon".to_string());
                                        app.system_state.processes.iter().filter(|p| p.user == current_user && !p.name.starts_with('[')).count()
                                    };
                                    if table_row < proc_count { app.process_selected_idx = Some(table_row); app.process_table_state.select(app.process_selected_idx); }
                                    return true;
                                }
                            }
                            _ => {}
                        }
                        return true;
                    }

                    if row == 0 {
                        if let Some((_, action_id)) = app.header_icon_bounds.iter().find(|(r, _)| column >= r.x && column < r.x + r.width && row == r.y) {
                            match action_id.as_str() {
                                "monitor" => { app.current_view = if app.current_view == CurrentView::Processes { CurrentView::Files } else { CurrentView::Processes }; }
                                "burger" => { app.mode = AppMode::Settings; }
                                _ => {} 
                            }
                            return true;
                        }
                    }

                    if column < sw {
                        app.sidebar_focus = true;
                        if let Some(b) = app.sidebar_bounds.iter().find(|b| b.y == row) {
                            app.sidebar_index = b.index;
                            if let SidebarTarget::Favorite(p) = &b.target {
                                let p = p.clone();
                                if let Some(fs) = app.current_file_state_mut() { fs.current_path = p; fs.selected_index = Some(0); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                            }
                        }
                        return true;
                    }
                    
                    if row >= 3 {
                        if let Some(fs) = app.current_file_state_mut() {
                            let idx = (row as usize).saturating_sub(4) + fs.table_state.offset();
                            if idx < fs.files.len() { fs.selected_index = Some(idx); fs.table_state.select(Some(idx)); }
                        }
                    }
                }
                MouseEventKind::ScrollUp => {
                    if app.current_view == CurrentView::Processes {
                        if let Some(sel) = app.process_selected_idx { app.process_selected_idx = Some(sel.saturating_sub(3)); app.process_table_state.select(app.process_selected_idx); }
                    } else if let Some(fs) = app.current_file_state_mut() {
                        let new_offset = fs.table_state.offset().saturating_sub(3);
                        *fs.table_state.offset_mut() = new_offset;
                    }
                    return true;
                }
                MouseEventKind::ScrollDown => {
                    if app.current_view == CurrentView::Processes {
                        let max_idx = app.system_state.processes.len().saturating_sub(1);
                        if let Some(sel) = app.process_selected_idx { app.process_selected_idx = Some((sel + 3).min(max_idx)); app.process_table_state.select(app.process_selected_idx); }
                    } else if let Some(fs) = app.current_file_state_mut() {
                        *fs.table_state.offset_mut() = fs.table_state.offset().saturating_add(3);
                    }
                    return true;
                }
                _ => {}
            }
        }
        _ => {} 
    }
    false
}