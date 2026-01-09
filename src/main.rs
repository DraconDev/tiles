use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

// Terma Imports
use terma::integration::ratatui::TermaBackend;
use terma::input::event::{Event, KeyCode, MouseEventKind, KeyModifiers};

// Ratatui Imports
use ratatui::Terminal;

use crate::app::{App, AppMode, CommandItem, AppEvent, SidebarTarget, ContextMenuTarget, SettingsSection, SettingsTarget};

mod app;
mod config;
mod modules;
mod event;
mod ui;
mod license;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    crate::app::log_debug("main start");
    
    // Initialize TermaBackend (Raw Mode, etc.)
    let backend = TermaBackend::new(std::io::stdout())?;
    crate::app::log_debug("TermaBackend created");
    let tile_queue = backend.tile_queue();
    let mut terminal = Terminal::new(backend)?;

    // Setup App & Channels
    let app = Arc::new(Mutex::new(App::new(tile_queue)));    
    let (event_tx, mut event_rx) = mpsc::channel::<AppEvent>(1000); 

    // 1. TTY Input Loop (Parser -> Raw AppEvent)
    {
        let tx = event_tx.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            use std::os::fd::AsRawFd;
            let mut parser = terma::input::parser::Parser::new();
            let mut stdin = std::io::stdin();
            let fd = stdin.as_raw_fd();
            let mut buffer = [0; 1024];
            loop {
                let polled = unsafe { terma::backend::tty::poll_input(std::os::fd::BorrowedFd::borrow_raw(fd), 20) };
                match polled {
                    Ok(true) => {
                         match stdin.read(&mut buffer) {
                            Ok(0) => break,
                            Ok(n) => {
                                for i in 0..n {
                                    if let Some(evt) = parser.advance(buffer[i]) {
                                         if let Some(converted) = crate::event::convert_event(evt) {
                                             let _ = tx.blocking_send(AppEvent::Raw(converted));
                                         }
                                    }
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    Ok(false) => {
                        if let Some(evt) = parser.check_timeout() {
                             if let Some(converted) = crate::event::convert_event(evt) {
                                 let _ = tx.blocking_send(AppEvent::Raw(converted));
                             }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    // 2. System Stats Loop
    {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            let mut sys_mod = modules::system::SystemModule::new();
            loop {
                let data = sys_mod.get_data();
                let _ = tx.send(AppEvent::SystemUpdated(data)).await;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }

    // 3. Tick Loop
    {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            loop {
                let _ = tx.send(AppEvent::Tick).await;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });
    }

    // Main UI & Event Handling Loop
    crate::app::log_debug("Entering main loop");
    loop {
        // Draw
        {
            let mut app_guard = app.lock().unwrap();
            if !app_guard.running { 
                let _ = crate::config::save_state(&app_guard);
                break; 
            }
            app_guard.terminal_size = (terminal.size()?.width, terminal.size()?.height);
            terminal.draw(|f| {
                ui::draw(f, &mut app_guard);
            })?;
        }

        // Process Events
        while let Ok(event) = event_rx.try_recv() {
            match event {
                AppEvent::Raw(raw) => {
                    let mut app_guard = app.lock().unwrap();
                    handle_event(raw, &mut app_guard, event_tx.clone());
                }
                AppEvent::SystemUpdated(data) => {
                    let mut app_guard = app.lock().unwrap();
                    app_guard.system_state.cpu_usage = data.cpu_usage;
                    app_guard.system_state.mem_usage = data.mem_usage;
                    app_guard.system_state.total_mem = data.total_mem;
                    app_guard.system_state.disks = data.disks;
                }
                AppEvent::RefreshFiles(pane_idx) => {
                    let mut app_guard = app.lock().unwrap();
                    app_guard.update_files_for_active_tab(pane_idx);
                }
                AppEvent::FilesUpdated(pane_idx, files, meta, git, branch) => {
                    let mut app_guard = app.lock().unwrap();
                    if let Some(pane) = app_guard.panes.get_mut(pane_idx) {
                        if let Some(fs) = pane.current_state_mut() {
                            fs.files = files; fs.metadata = meta; fs.git_status = git; fs.git_branch = branch;
                        }
                    }
                }
                AppEvent::Delete(path) => {
                    let _ = std::fs::remove_file(&path).or_else(|_| std::fs::remove_dir_all(&path));
                    let mut app_guard = app.lock().unwrap();
                    let idx = app_guard.focused_pane_index;
                    app_guard.update_files_for_active_tab(idx);
                }
                AppEvent::Rename(old, new) => {
                    let _ = std::fs::rename(old, new);
                    let mut app_guard = app.lock().unwrap();
                    let idx = app_guard.focused_pane_index;
                    app_guard.update_files_for_active_tab(idx);
                }
                AppEvent::CreateFile(path) => {
                    let _ = std::fs::File::create(&path);
                    let mut app_guard = app.lock().unwrap();
                    let idx = app_guard.focused_pane_index;
                    app_guard.update_files_for_active_tab(idx);
                }
                AppEvent::CreateFolder(path) => {
                    let _ = std::fs::create_dir(&path);
                    let mut app_guard = app.lock().unwrap();
                    let idx = app_guard.focused_pane_index;
                    app_guard.update_files_for_active_tab(idx);
                }
                AppEvent::Tick => {}
            }
        }
        tokio::time::sleep(Duration::from_millis(16)).await;
    }

    Ok(())
}

fn push_history(fs: &mut crate::app::FileState, path: std::path::PathBuf) {
    if let Some(last) = fs.history.get(fs.history_index) {
        if last == &path { return; }
    }
    fs.history.truncate(fs.history_index + 1);
    fs.history.push(path);
    fs.history_index = fs.history.len() - 1;
}

fn navigate_back(fs: &mut crate::app::FileState) {
    if fs.history_index > 0 {
        fs.history_index -= 1;
        fs.current_path = fs.history[fs.history_index].clone();
        fs.selected_index = Some(0);
        fs.table_state.select(Some(0));
        *fs.table_state.offset_mut() = 0;
        fs.search_filter.clear();
    }
}

fn navigate_forward(fs: &mut crate::app::FileState) {
    if fs.history_index + 1 < fs.history.len() {
        fs.history_index += 1;
        fs.current_path = fs.history[fs.history_index].clone();
        fs.selected_index = Some(0);
        fs.table_state.select(Some(0));
        *fs.table_state.offset_mut() = 0;
        fs.search_filter.clear();
    }
}

fn fs_mouse_index(row: u16, app: &App) -> usize {
    let mouse_row_offset = row.saturating_sub(3) as usize;
    if let Some(fs) = app.current_file_state() { fs.table_state.offset() + mouse_row_offset }
    else { 0 }
}

fn update_commands(app: &mut App) {
    let commands = vec![
        CommandItem { key: "quit".to_string(), desc: "Quit".to_string(), action: crate::app::CommandAction::Quit },
        CommandItem { key: "remote".to_string(), desc: "Add Remote Host".to_string(), action: crate::app::CommandAction::AddRemote },
    ];
    let mut filtered = commands;
    for bookmark_idx in 0..app.remote_bookmarks.len() {
        let bookmark = &app.remote_bookmarks[bookmark_idx];
        filtered.push(CommandItem { key: format!("connect_{}", bookmark_idx), desc: format!("Connect to: {}", bookmark.name), action: crate::app::CommandAction::ConnectToRemote(bookmark_idx) });
    }
    app.filtered_commands = filtered.into_iter().filter(|cmd| cmd.desc.to_lowercase().contains(&app.input.to_lowercase())).collect();
    app.command_index = app.command_index.min(app.filtered_commands.len().saturating_sub(1));
}

fn execute_command(action: crate::app::CommandAction, app: &mut App, _event_tx: mpsc::Sender<AppEvent>) {
    match action {
        crate::app::CommandAction::Quit => { app.running = false; },
        crate::app::CommandAction::ToggleZoom => app.toggle_zoom(),
        crate::app::CommandAction::SwitchView(view) => app.current_view = view,
        crate::app::CommandAction::AddRemote => { app.mode = AppMode::AddRemote; app.input.clear(); },
        crate::app::CommandAction::ConnectToRemote(idx) => {
            if let Some(_bookmark) = app.remote_bookmarks.get(idx).cloned() {
                // Connection logic would go here
            }
        },
        crate::app::CommandAction::CommandPalette => { app.mode = AppMode::CommandPalette; },
    }
}

fn handle_event(evt: Event, app: &mut App, event_tx: mpsc::Sender<AppEvent>) {
    match evt {
        Event::Mouse(me) => {
            let column = me.column;
            let row = me.row;
            
            if let MouseEventKind::Down(_button) = me.kind {
                let (w, h) = app.terminal_size;
                
                // 0. Global Header (Row 0)
                if row == 0 {
                    if column < 10 { app.mode = AppMode::Settings; return; }
                    if column >= w.saturating_sub(3) {
                        app.toggle_split();
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                        return;
                    }
                }

                // 1. Settings Modal Handling
                if app.mode == AppMode::Settings {
                    let area_w = (w as f32 * 0.8) as u16;
                    let area_h = (h as f32 * 0.8) as u16;
                    let area_x = (w - area_w) / 2;
                    let area_y = (h - area_h) / 2;

                    if column >= area_x && column < area_x + area_w && row >= area_y && row < area_y + area_h {
                        let inner = ratatui::layout::Rect::new(area_x + 1, area_y + 1, area_w.saturating_sub(2), area_h.saturating_sub(2));
                        
                        if column < inner.x + 15 {
                            let rel_y = row.saturating_sub(inner.y);
                            match rel_y {
                                0 => app.settings_section = SettingsSection::Columns,
                                1 => app.settings_section = SettingsSection::Tabs,
                                2 => app.settings_section = SettingsSection::General,
                                _ => {}
                            }
                        } else {
                            match app.settings_section {
                                SettingsSection::Columns => {
                                    if row >= inner.y && row < inner.y + 3 {
                                        let content_x = column.saturating_sub(inner.x + 16);
                                        match content_x / 12 {
                                            0 => app.settings_target = SettingsTarget::AllPanes,
                                            1 => app.settings_target = SettingsTarget::Pane(0),
                                            2 => if app.panes.len() > 1 { app.settings_target = SettingsTarget::Pane(1); }
                                            _ => {}
                                        }
                                    } else if row >= inner.y + 4 {
                                        let rel_y = row.saturating_sub(inner.y + 4);
                                        match rel_y {
                                            0 => app.toggle_column(crate::app::FileColumn::Name),
                                            1 => app.toggle_column(crate::app::FileColumn::Size),
                                            2 => app.toggle_column(crate::app::FileColumn::Modified),
                                            3 => app.toggle_column(crate::app::FileColumn::Created),
                                            4 => app.toggle_column(crate::app::FileColumn::Permissions),
                                            5 => app.toggle_column(crate::app::FileColumn::Extension),
                                            _ => {}
                                        }
                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                    }
                                }
                                SettingsSection::General => {
                                    let rel_y = row.saturating_sub(inner.y + 1);
                                    match rel_y {
                                        0 => app.default_show_hidden = !app.default_show_hidden,
                                        1 => app.confirm_delete = !app.confirm_delete,
                                        _ => {}
                                    }
                                }
                                _ => {}
                            }
                        }
                        return;
                    } else { app.mode = AppMode::Normal; return; }
                }

                // 2. Context Menu Handling
                if let AppMode::ContextMenu { x, y, target } = app.mode.clone() {
                    let menu_width = 20;
                    let menu_height = match target {
                        ContextMenuTarget::File(_) => 6, ContextMenuTarget::Folder(_) => 5, ContextMenuTarget::EmptySpace => 5,
                        ContextMenuTarget::SidebarFavorite(_) => 3, ContextMenuTarget::SidebarRemote(_) => 3, ContextMenuTarget::SidebarStorage(_) => 2,
                    };
                    if column >= x && column < x + menu_width && row >= y && row < y + menu_height {
                        let menu_row = row.saturating_sub(y + 1) as usize;
                        match target {
                            ContextMenuTarget::File(idx) => {
                                if let Some(fs) = app.current_file_state_mut() {
                                    if let Some(path) = fs.files.get(idx).cloned() {
                                        match menu_row {
                                            0 => { let _ = std::process::Command::new("xdg-open").arg(&path).spawn(); app.mode = AppMode::Normal; }
                                            1 => { if app.starred.contains(&path) { app.starred.retain(|x| x != &path); } else { app.starred.push(path.clone()); } app.mode = AppMode::Normal; }
                                            2 => app.mode = AppMode::Rename, 3 => app.mode = AppMode::Delete, 4 => app.mode = AppMode::Properties, _ => app.mode = AppMode::Normal,
                                        }
                                    }
                                }
                            }
                            ContextMenuTarget::Folder(idx) => {
                                if let Some(fs) = app.current_file_state_mut() {
                                    if let Some(path) = fs.files.get(idx).cloned() {
                                        match menu_row {
                                            0 => { fs.current_path = path.clone(); fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); app.mode = AppMode::Normal; }
                                            1 => { if app.starred.contains(&path) { app.starred.retain(|x| x != &path); } else { app.starred.push(path.clone()); } app.mode = AppMode::Normal; }
                                            2 => app.mode = AppMode::Rename, 3 => app.mode = AppMode::Delete, _ => app.mode = AppMode::Normal,
                                        }
                                    }
                                }
                            }
                            ContextMenuTarget::EmptySpace => {
                                match menu_row {
                                    0 => { app.mode = AppMode::NewFolder; app.input.clear(); }, 1 => { app.mode = AppMode::NewFile; app.input.clear(); },
                                    2 => { let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); app.mode = AppMode::Normal; },
                                    3 => { if let Some(fs) = app.current_file_state() { let terminals = ["kgx", "alacritty", "kitty", "wezterm", "gnome-terminal", "konsole", "xterm", "xdg-terminal"]; for t in terminals { if std::process::Command::new("which").arg(t).stdout(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false) { if std::process::Command::new(t).current_dir(&fs.current_path).spawn().is_ok() { break; } } } } app.mode = AppMode::Normal; }
                                    _ => app.mode = AppMode::Normal,
                                }
                            }
                            ContextMenuTarget::SidebarFavorite(path) => {
                                let p_fav = path.clone();
                                match menu_row {
                                    0 => { app.starred.retain(|x| x != &p_fav); app.mode = AppMode::Normal; }
                                    1 => { if let Some(pane) = app.panes.get_mut(app.focused_pane_index) { let mut new_fs = pane.tabs[pane.active_tab_index].clone(); new_fs.current_path = p_fav.clone(); new_fs.selected_index = Some(0); new_fs.search_filter.clear(); *new_fs.table_state.offset_mut() = 0; new_fs.history = vec![p_fav]; new_fs.history_index = 0; pane.open_tab(new_fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } app.mode = AppMode::Normal; }
                                    _ => app.mode = AppMode::Normal,
                                }
                            }
                            ContextMenuTarget::SidebarRemote(idx) => {
                                match menu_row {
                                    0 => { execute_command(crate::app::CommandAction::ConnectToRemote(idx), app, event_tx.clone()); app.mode = AppMode::Normal; }
                                    1 => { app.remote_bookmarks.remove(idx); app.mode = AppMode::Normal; }
                                    _ => app.mode = AppMode::Normal,
                                }
                            }
                            _ => app.mode = AppMode::Normal,
                        }
                        return;
                    } else { app.mode = AppMode::Normal; return; }
                }

                if app.mode != AppMode::Normal { return; }

                // 3. Sidebar Focus
                let sidebar_width = app.sidebar_width();
                if column < sidebar_width {
                    app.sidebar_focus = true;
                    let clicked_sidebar_item = app.sidebar_bounds.iter().find(|b| b.y == row).cloned();
                    if let Some(bound) = clicked_sidebar_item {
                        app.sidebar_index = bound.index;
                        match &bound.target {
                            SidebarTarget::Favorite(p) => {
                                let p2 = p.clone();
                                if let Some(fs) = app.current_file_state_mut() { fs.current_path = p2.clone(); fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, p2); }
                                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); app.sidebar_focus = false;
                            }
                            SidebarTarget::Storage(idx) => {
                                if let Some(disk) = app.system_state.disks.get(*idx) {
                                    let p = std::path::PathBuf::from(&disk.name);
                                    if let Some(fs) = app.current_file_state_mut() { fs.current_path = p.clone(); fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, p); }
                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); app.sidebar_focus = false;
                                }
                            }
                            _ => {}
                        }
                    }
                    return;
                }

                // 4. Pane Selection
                let content_area_width = w.saturating_sub(sidebar_width);
                let pane_count = app.panes.len();
                let pane_width = if pane_count > 0 { content_area_width / pane_count as u16 } else { content_area_width };
                let clicked_pane = (column.saturating_sub(sidebar_width) / pane_width) as usize;
                if clicked_pane < pane_count { app.focused_pane_index = clicked_pane; app.sidebar_focus = false; }

                // 5. File Selection
                if row >= 3 {
                    let index = fs_mouse_index(row, app);
                    let mut selected_path = None;
                    if let Some(fs) = app.current_file_state_mut() {
                        if index < fs.files.len() {
                            fs.selected_index = Some(index); fs.table_state.select(Some(index));
                            selected_path = Some(fs.files[index].clone());
                        }
                    }
                    
                    if let Some(path) = selected_path {
                        app.drag_source = Some(path.clone()); app.drag_start_pos = Some((column, row));
                        if app.mouse_last_click.elapsed() < Duration::from_millis(500) && app.mouse_click_pos == (column, row) {
                            if path.is_dir() { 
                                if let Some(fs) = app.current_file_state_mut() { fs.current_path = path.clone(); fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                            }
                            else { let _ = std::process::Command::new("xdg-open").arg(&path).spawn(); }
                        }
                        app.mouse_last_click = std::time::Instant::now(); app.mouse_click_pos = (column, row);
                    }
                }
            }

            match me.kind {
                MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                    app.mouse_pos = (column, row);
                    if let Some((sx, sy)) = app.drag_start_pos {
                        if ((column as i16 - sx as i16).pow(2) + (row as i16 - sy as i16).pow(2)) as f32 >= 1.0 { app.is_dragging = true; }
                    }
                }
                MouseEventKind::ScrollUp => {
                    if let Some(fs) = app.current_file_state_mut() {
                        let new_offset = fs.table_state.offset().saturating_sub(3);
                        *fs.table_state.offset_mut() = new_offset;
                    }
                }
                MouseEventKind::ScrollDown => {
                    if let Some(fs) = app.current_file_state_mut() {
                        let max_offset = fs.files.len().saturating_sub(fs.view_height.saturating_sub(4));
                        let new_offset = (fs.table_state.offset() + 3).min(max_offset);
                        *fs.table_state.offset_mut() = new_offset;
                    }
                }
                _ => {}
            }
        }
        Event::Key(key) => {
            match app.mode {
                AppMode::CommandPalette => {
                    match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Char(c) => { app.input.push(c); update_commands(app); }
                        KeyCode::Backspace => { app.input.pop(); update_commands(app); }
                        KeyCode::Enter => { if let Some(cmd) = app.filtered_commands.get(app.command_index).cloned() { execute_command(cmd.action, app, event_tx.clone()); } app.mode = AppMode::Normal; app.input.clear(); }
                        _ => {}
                    }
                }
                AppMode::Settings => {
                    match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Char('0') => app.settings_target = SettingsTarget::AllPanes,
                        KeyCode::Char('1') => app.settings_target = SettingsTarget::Pane(0),
                        KeyCode::Char('2') => if app.panes.len() > 1 { app.settings_target = SettingsTarget::Pane(1); }
                        KeyCode::Left | KeyCode::BackTab => { app.settings_section = match app.settings_section { SettingsSection::Columns => SettingsSection::General, SettingsSection::Tabs => SettingsSection::Columns, SettingsSection::General => SettingsSection::Tabs }; }
                        KeyCode::Right | KeyCode::Tab => { app.settings_section = match app.settings_section { SettingsSection::Columns => SettingsSection::Tabs, SettingsSection::Tabs => SettingsSection::General, SettingsSection::General => SettingsSection::Columns }; }
                        KeyCode::Char('n') => { app.toggle_column(crate::app::FileColumn::Name); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                        KeyCode::Char('s') => { app.toggle_column(crate::app::FileColumn::Size); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                        KeyCode::Char('m') => { app.toggle_column(crate::app::FileColumn::Modified); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                        KeyCode::Char('c') => { app.toggle_column(crate::app::FileColumn::Created); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                        KeyCode::Char('p') => { app.toggle_column(crate::app::FileColumn::Permissions); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                        KeyCode::Char('e') => { app.toggle_column(crate::app::FileColumn::Extension); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                        KeyCode::Char('h') if app.settings_section == SettingsSection::General => { app.default_show_hidden = !app.default_show_hidden; }
                        KeyCode::Char('d') if app.settings_section == SettingsSection::General => { app.confirm_delete = !app.confirm_delete; }
                        _ => {}
                    }
                }
                _ => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        match key.code {
                            KeyCode::Char('q') => { app.running = false; }
                            KeyCode::Char('s') => { app.toggle_split(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); }
                            KeyCode::Char('h') => { let pane_idx = app.toggle_hidden(); let _ = event_tx.try_send(AppEvent::RefreshFiles(pane_idx)); }
                            KeyCode::Char('t') | KeyCode::Char('.') => {
                                if let Some(fs) = app.current_file_state() {
                                    let mut terminals = vec!["kgx".to_string(), "gnome-terminal".to_string(), "konsole".to_string(), "xdg-terminal-exec".to_string(), "x-terminal-emulator".to_string(), "alacritty".to_string(), "kitty".to_string(), "xterm".to_string()];
                                    if let Ok(et) = std::env::var("TERMINAL") { terminals.insert(0, et); }
                                    for t in terminals {
                                        if std::process::Command::new("which").arg(&t).stdout(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false) {
                                            let mut cmd = std::process::Command::new(&t); cmd.current_dir(&fs.current_path);
                                            if t == "gnome-terminal" || t == "kgx" { cmd.arg("--window"); } else if t == "konsole" { cmd.arg("--new-window"); }
                                            if cmd.spawn().is_ok() { break; }
                                        }
                                    }
                                }
                            }
                            KeyCode::Char(' ') => { app.input.clear(); app.mode = AppMode::CommandPalette; update_commands(app); }
                            _ => {}
                        }
                        return;
                    }
                    if key.code == KeyCode::Esc {
                        if let Some(fs) = app.current_file_state_mut() { fs.multi_select.clear(); fs.selection_anchor = None; if !fs.search_filter.is_empty() { fs.search_filter.clear(); fs.selected_index = Some(0); *fs.table_state.offset_mut() = 0; let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } }
                    }
                    match key.code {
                        KeyCode::Down => { app.move_down(key.modifiers.contains(KeyModifiers::SHIFT)); }
                        KeyCode::Up => { app.move_up(key.modifiers.contains(KeyModifiers::SHIFT)); }
                        KeyCode::Left => { if key.modifiers.contains(KeyModifiers::SHIFT) { app.copy_to_other_pane(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); } else if key.modifiers.contains(KeyModifiers::CONTROL) { app.move_to_other_pane(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); } else { app.move_left(); } }
                        KeyCode::Right => { if key.modifiers.contains(KeyModifiers::SHIFT) { app.copy_to_other_pane(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); } else if key.modifiers.contains(KeyModifiers::CONTROL) { app.move_to_other_pane(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); } else { app.move_right(); } }
                        KeyCode::Enter => { if let Some(fs) = app.current_file_state_mut() { if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx).cloned() { if path.is_dir() { fs.current_path = path.clone(); fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } } } }
                        KeyCode::Char(' ') => { if let Some(fs) = app.current_file_state() { if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx).cloned() { if app.starred.contains(&path) { app.starred.retain(|x| x != &path); } else { app.starred.push(path.clone()); } } } } }
                        KeyCode::Char(c) if key.modifiers.is_empty() => { if let Some(fs) = app.current_file_state_mut() { fs.search_filter.push(c); fs.selected_index = Some(0); *fs.table_state.offset_mut() = 0; let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } }
                        KeyCode::Backspace => { if let Some(fs) = app.current_file_state_mut() { if !fs.search_filter.is_empty() { fs.search_filter.pop(); fs.selected_index = Some(0); *fs.table_state.offset_mut() = 0; let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } else if let Some(parent) = fs.current_path.parent() { let p = parent.to_path_buf(); fs.current_path = p.clone(); fs.selected_index = Some(0); *fs.table_state.offset_mut() = 0; push_history(fs, p); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } }
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
}

fn fs_mouse_index(row: u16, app: &App) -> usize {
    let mouse_row_offset = row.saturating_sub(3) as usize;
    if let Some(fs) = app.current_file_state() { fs.table_state.offset() + mouse_row_offset }
    else { 0 }
}

fn update_commands(app: &mut App) {
    let commands = vec![
        CommandItem { key: "quit".to_string(), desc: "Quit".to_string(), action: crate::app::CommandAction::Quit },
        CommandItem { key: "remote".to_string(), desc: "Add Remote Host".to_string(), action: crate::app::CommandAction::AddRemote },
    ];
    let mut filtered = commands;
    for bookmark_idx in 0..app.remote_bookmarks.len() {
        let bookmark = &app.remote_bookmarks[bookmark_idx];
        filtered.push(CommandItem { key: format!("connect_{}", bookmark_idx), desc: format!("Connect to: {}", bookmark.name), action: crate::app::CommandAction::ConnectToRemote(bookmark_idx) });
    }
    app.filtered_commands = filtered.into_iter().filter(|cmd| cmd.desc.to_lowercase().contains(&app.input.to_lowercase())).collect();
    app.command_index = app.command_index.min(app.filtered_commands.len().saturating_sub(1));
}

fn execute_command(action: crate::app::CommandAction, app: &mut App, _event_tx: mpsc::Sender<AppEvent>) {
    match action {
        crate::app::CommandAction::Quit => { app.running = false; },
        crate::app::CommandAction::ToggleZoom => app.toggle_zoom(),
        crate::app::CommandAction::SwitchView(view) => app.current_view = view,
        crate::app::CommandAction::AddRemote => { app.mode = AppMode::AddRemote; app.input.clear(); },
        crate::app::CommandAction::ConnectToRemote(idx) => {
            if let Some(_bookmark) = app.remote_bookmarks.get(idx).cloned() {
                // Connection logic would go here
            }
        },
        crate::app::CommandAction::CommandPalette => { app.mode = AppMode::CommandPalette; },
    }
}
