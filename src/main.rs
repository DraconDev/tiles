use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

// Terma Imports
use terma::integration::ratatui::TermaBackend;
use terma::input::event::{Event, KeyCode, MouseEventKind, KeyModifiers, MouseButton};

// Ratatui Imports
use ratatui::Terminal;

use crate::app::{App, AppMode, CommandItem, AppEvent, SidebarTarget, ContextMenuTarget, SettingsSection, SettingsTarget, DropTarget};

mod app;
mod config;
mod modules;
mod ui;
mod license;

#[tokio::main] async fn main() -> Result<(), Box<dyn std::error::Error>> {
    crate::app::log_debug("main start");
    
    // Initialize TermaBackend
    let backend = TermaBackend::new(std::io::stdout())?;
    let tile_queue = backend.tile_queue();
    let mut terminal = Terminal::new(backend)?;

    // Setup App & Channels
    let app = Arc::new(Mutex::new(App::new(tile_queue)));    
    let (event_tx, mut event_rx) = mpsc::channel::<AppEvent>(1000); 

    // 1. TTY Input Loop
    {
        let tx = event_tx.clone();
        terma::input::InputReader::spawn(move |evt| {
             let _ = tx.blocking_send(AppEvent::Raw(evt));
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

    // Initial Refresh
    {
        let pane_count = {
            let app_guard = app.lock().unwrap();
            app_guard.panes.len()
        };
        for i in 0..pane_count {
            let _ = event_tx.send(AppEvent::RefreshFiles(i)).await;
        }
        
        // Initial Size
        let mut app_guard = app.lock().unwrap();
        if let Ok(size) = terminal.size() {
            app_guard.terminal_size = (size.width, size.height);
        }
    }

    loop {
        // Draw
        {
            let mut app_guard = app.lock().unwrap();
            if !app_guard.running { 
                let _ = crate::config::save_state(&app_guard);
                break; 
            }
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
                    let (path, show_hidden, filter, session, sort_column, sort_ascending) = {
                        let app_guard = app.lock().unwrap();
                        if let Some(pane) = app_guard.panes.get(pane_idx) {
                            if let Some(fs) = pane.current_state() {
                                (
                                    fs.current_path.clone(),
                                    fs.show_hidden,
                                    fs.search_filter.clone(),
                                    fs.remote_session.as_ref().map(|rs| rs.session.clone()),
                                    fs.sort_column,
                                    fs.sort_ascending
                                )
                            } else { continue; }
                        } else { continue; }
                    };
                    
                    let tx = event_tx.clone();
                    tokio::spawn(async move {
                        let mut temp_state = crate::app::FileState::new(
                            path,
                            None,
                            show_hidden,
                            vec![], 
                            sort_column,
                            sort_ascending,
                        );
                        temp_state.search_filter = filter;
                        
                        if let Some(s_mutex) = session {
                            if let Ok(s) = s_mutex.lock() { 
                                crate::modules::files::update_files(&mut temp_state, Some(&s)); 
                            }
                        } else {
                            crate::modules::files::update_local_files(&mut temp_state);
                        }
                        
                        let _ = tx.send(AppEvent::FilesUpdated(
                            pane_idx, 
                            temp_state.files, 
                            temp_state.metadata, 
                            temp_state.git_status, 
                            temp_state.git_branch,
                            temp_state.local_count
                        )).await;
                    });
                }
                AppEvent::FilesUpdated(pane_idx, files, meta, git, branch, local_count) => {
                    let mut app_guard = app.lock().unwrap();
                    if let Some(pane) = app_guard.panes.get_mut(pane_idx) {
                        if let Some(fs) = pane.current_state_mut() {
                            fs.files = files; 
                            fs.metadata = meta; 
                            fs.git_status = git; 
                            fs.git_branch = branch;
                            fs.local_count = local_count;
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
                AppEvent::RemoteConnected(pane_idx, session) => {
                    let mut app_guard = app.lock().unwrap();
                    if let Some(pane) = app_guard.panes.get_mut(pane_idx) {
                        if let Some(fs) = pane.current_state_mut() {
                            fs.current_path = std::path::PathBuf::from("/");
                            fs.remote_session = Some(session);
                            fs.selected_index = Some(0);
                            fs.search_filter.clear();
                            *fs.table_state.offset_mut() = 0;
                            fs.history = vec![fs.current_path.clone()];
                            fs.history_index = 0;
                        }
                    }
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(pane_idx));
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
        CommandItem { key: "import_servers".to_string(), desc: "Import Servers (servers.toml)".to_string(), action: crate::app::CommandAction::ImportServers },
    ];
    let mut filtered = commands;
    for bookmark_idx in 0..app.remote_bookmarks.len() {
        let bookmark = &app.remote_bookmarks[bookmark_idx];
        filtered.push(CommandItem { key: format!("connect_{}", bookmark_idx), desc: format!("Connect to: {}", bookmark.name), action: crate::app::CommandAction::ConnectToRemote(bookmark_idx) });
    }
    app.filtered_commands = filtered.into_iter().filter(|cmd| cmd.desc.to_lowercase().contains(&app.input.value.to_lowercase())).collect();
    app.command_index = app.command_index.min(app.filtered_commands.len().saturating_sub(1));
}

fn execute_command(action: crate::app::CommandAction, app: &mut App, _event_tx: mpsc::Sender<AppEvent>) {
    match action {
        crate::app::CommandAction::Quit => { app.running = false; },
        crate::app::CommandAction::ToggleZoom => app.toggle_zoom(),
        crate::app::CommandAction::SwitchView(view) => app.current_view = view,
        crate::app::CommandAction::AddRemote => { app.mode = AppMode::AddRemote; app.input.clear(); },
        crate::app::CommandAction::ConnectToRemote(idx) => {
            if let Some(bookmark) = app.remote_bookmarks.get(idx).cloned() {
                let tx = _event_tx.clone();
                let pane_idx = app.focused_pane_index;
                let bookmark_name = bookmark.name.clone();
                
                tokio::task::spawn_blocking(move || {
                    use std::net::TcpStream;
                    let addr = format!("{}:{}", bookmark.host, bookmark.port);
                    if let Ok(tcp) = TcpStream::connect_timeout(&addr.parse().unwrap_or("127.0.0.1:22".parse().unwrap()), Duration::from_secs(5)) {
                        if let Ok(mut sess) = ssh2::Session::new() {
                            sess.set_tcp_stream(tcp);
                            sess.handshake().ok();
                            
                            let auth_ok = if let Some(key_path) = &bookmark.key_path {
                                sess.userauth_pubkey_file(&bookmark.user, None, key_path, None).is_ok()
                            } else {
                                // Fallback to agent
                                sess.userauth_agent(&bookmark.user).is_ok()
                            };

                            if auth_ok {
                                let remote_sess = crate::app::RemoteSession {
                                    name: bookmark_name,
                                    host: bookmark.host,
                                    user: bookmark.user,
                                    session: Arc::new(Mutex::new(sess)),
                                };
                                let _ = tx.blocking_send(AppEvent::RemoteConnected(pane_idx, remote_sess));
                            }
                        }
                    }
                });
            }
        },
        crate::app::CommandAction::ImportServers => {
            let import_path = if let Some(fs) = app.current_file_state() {
                fs.current_path.join("servers.toml")
            } else {
                std::path::PathBuf::from("servers.toml")
            };
            let _ = app.import_servers(import_path);
            let _ = crate::config::save_state(app);
        },
        crate::app::CommandAction::CommandPalette => { app.mode = AppMode::CommandPalette; },
    }
}

fn spawn_terminal(path: &std::path::Path, new_tab: bool, remote: Option<&crate::app::RemoteSession>) {
    let mut terminals = vec!["kgx", "gnome-terminal", "konsole", "xdg-terminal-exec", "x-terminal-emulator", "alacritty", "kitty", "xterm"];
    let env_t;
    if let Ok(et) = std::env::var("TERMINAL") { env_t = et; terminals.insert(0, &env_t); }
    
    for t in terminals {
        if std::process::Command::new("which").arg(t).stdout(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false) {
            let mut args = Vec::new();
            if new_tab && (t == "gnome-terminal" || t == "kgx") { args.push("--tab"); }
            
            let cmd_str;
            if let Some(r) = remote {
                 let path_str = path.to_string_lossy();
                 let ssh_target = format!("{}@{}", r.user, r.host);
                 if t == "gnome-terminal" || t == "kgx" {
                     cmd_str = format!("{} {} -- ssh -t {} \"cd '{}'; exec \\$SHELL\" &", t, args.join(" "), ssh_target, path_str);
                 } else if t == "konsole" {
                     cmd_str = format!("{} {} -e ssh -t {} \"cd '{}'; exec \\$SHELL\" &", t, args.join(" "), ssh_target, path_str);
                 } else {
                     cmd_str = format!("{} -e ssh -t {} \"cd '{}'; exec \\$SHELL\" &", t, ssh_target, path_str);
                 }
            } else {
                if t == "gnome-terminal" || t == "kgx" { args.push("--working-directory"); } 
                else if t == "konsole" { args.push("--workdir"); }
                else if t == "alacritty" { args.push("--working-directory"); }
                
                let path_str = path.to_string_lossy();
                cmd_str = format!("{} {} \"{}\" &", t, args.join(" "), path_str);
            }
            
            let _ = std::process::Command::new("sh").arg("-c").arg(&cmd_str).spawn();
            break;
        }
    }
}

fn handle_event(evt: Event, app: &mut App, event_tx: mpsc::Sender<AppEvent>) {
    match evt {
        Event::Resize(w, h) => {
            app.terminal_size = (w, h);
        }
        Event::Key(key) => {
            crate::app::log_debug(&format!("KEY EVENT: code={:?} modifiers={:?}", key.code, key.modifiers));
            match app.mode {
                AppMode::CommandPalette => {
                    match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Enter => { if let Some(cmd) = app.filtered_commands.get(app.command_index).cloned() { execute_command(cmd.action, app, event_tx.clone()); } app.mode = AppMode::Normal; app.input.clear(); }
                        _ => {
                            if app.input.handle_event(&evt) {
                                update_commands(app);
                            }
                        }
                    }
                }
                AppMode::Settings => {
                    match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Char('1') => app.settings_target = SettingsTarget::SingleMode,
                        KeyCode::Char('2') => app.settings_target = SettingsTarget::SplitMode,
                        KeyCode::Left | KeyCode::BackTab => { app.settings_section = match app.settings_section { SettingsSection::Columns => SettingsSection::Remotes, SettingsSection::Tabs => SettingsSection::Columns, SettingsSection::General => SettingsSection::Tabs, SettingsSection::Remotes => SettingsSection::General }; } 
                        KeyCode::Right | KeyCode::Tab => { app.settings_section = match app.settings_section { SettingsSection::Columns => SettingsSection::Tabs, SettingsSection::Tabs => SettingsSection::General, SettingsSection::General => SettingsSection::Remotes, SettingsSection::Remotes => SettingsSection::Columns }; } 
                        KeyCode::Char('n') => { app.toggle_column(crate::app::FileColumn::Name); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } 
                        KeyCode::Char('s') => { app.toggle_column(crate::app::FileColumn::Size); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } 
                        KeyCode::Char('m') => { app.toggle_column(crate::app::FileColumn::Modified); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } 
                        KeyCode::Char('p') => { app.toggle_column(crate::app::FileColumn::Permissions); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } 
                        KeyCode::Char('h') if app.settings_section == SettingsSection::General => { app.default_show_hidden = !app.default_show_hidden; } 
                        KeyCode::Char('d') if app.settings_section == SettingsSection::General => { app.confirm_delete = !app.confirm_delete; } 
                        _ => {} 
                    }
                }
                                AppMode::ImportServers => {
                                    match key.code {
                                        KeyCode::Esc => app.mode = AppMode::Normal,
                                        KeyCode::Enter => {
                                            let filename = app.input.value.clone();
                                            let import_path = if let Some(fs) = app.current_file_state() { fs.current_path.join(filename) } else { std::path::PathBuf::from(filename) };
                                            let _ = app.import_servers(import_path);
                                            let _ = crate::config::save_state(app);
                                            app.mode = AppMode::Normal; app.input.clear();
                                        }
                                        _ => { app.input.handle_event(&evt); }
                                    }
                                }
                                AppMode::NewFile | AppMode::NewFolder | AppMode::Rename | AppMode::Delete => {
                                    // Rename special logic
                                    if app.mode == AppMode::Rename && app.rename_selected {
                                        match key.code {
                                            KeyCode::Char(c) => {
                                                app.rename_selected = false;
                                                let input_val = app.input.value.clone();
                                                let path = std::path::Path::new(&input_val);
                                                if let Some(stem) = path.file_stem() {
                                                    if let Some(ext) = path.extension() {
                                                        if !stem.to_string_lossy().is_empty() {
                                                            app.input.set_value(format!("{}.{}", c, ext.to_string_lossy()));
                                                        } else {
                                                            app.input.set_value(c.to_string());
                                                        }
                                                    } else {
                                                        app.input.set_value(c.to_string());
                                                    }
                                                } else {
                                                    app.input.set_value(c.to_string());
                                                }
                                                return;
                                            }
                                            KeyCode::Backspace => {
                                                 app.rename_selected = false;
                                                 let input_val = app.input.value.clone();
                                                 let path = std::path::Path::new(&input_val);
                                                 if let Some(ext) = path.extension() {
                                                     app.input.set_value(format!(".{}", ext.to_string_lossy()));
                                                 } else {
                                                     app.input.clear();
                                                 }
                                                 return;
                                            }
                                            KeyCode::Left | KeyCode::Right => {
                                                app.rename_selected = false;
                                                // Fall through to standard navigation
                                            }
                                            KeyCode::Esc => { app.mode = AppMode::Normal; app.input.clear(); return; }
                                            KeyCode::Enter => {} // Handled below
                                            _ => {}
                                        }
                                    }

                                    match key.code {
                                        KeyCode::Esc => { app.mode = AppMode::Normal; app.input.clear(); }
                                        KeyCode::Enter => {
                                            let input = app.input.value.clone();
                                            if let Some(fs) = app.current_file_state() {
                                                let path = fs.current_path.join(&input);
                                                match app.mode {
                                                    AppMode::NewFile => { let _ = event_tx.try_send(AppEvent::CreateFile(path)); }
                                                    AppMode::NewFolder => { let _ = event_tx.try_send(AppEvent::CreateFolder(path)); }
                                                    AppMode::Rename => {
                                                        if let Some(idx) = fs.selected_index {
                                                            if let Some(old_path) = fs.files.get(idx) {
                                                                let new_path = old_path.parent().unwrap_or(&std::path::PathBuf::from(".")).join(&input);
                                                                let _ = event_tx.try_send(AppEvent::Rename(old_path.clone(), new_path));
                                                            }
                                                        }
                                                    }
                                                    AppMode::Delete => {
                                                        if input.to_lowercase() == "y" || input.to_lowercase() == "yes" || !app.confirm_delete {
                                                            if let Some(idx) = fs.selected_index {
                                                                if let Some(path) = fs.files.get(idx) {
                                                                    let _ = event_tx.try_send(AppEvent::Delete(path.clone()));
                                                                }
                                                            }
                                                        }
                                                    }
                                                    _ => {} 
                                                }
                                            }
                                            app.mode = AppMode::Normal;
                                            app.input.clear();
                                        }
                                        _ => { app.input.handle_event(&evt); }
                                    }
                                }
                                _ => {
                                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                                        match key.code {
                                            KeyCode::Char('q') => { app.running = false; }
                                            KeyCode::Char('s') => { app.toggle_split(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); } 
                                            KeyCode::Char('h') => { let pane_idx = app.toggle_hidden(); let _ = event_tx.try_send(AppEvent::RefreshFiles(pane_idx)); } 
                                            KeyCode::Char('t') => {
                                                if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                                                    if let Some(fs) = pane.current_state() {
                                                        let mut new_fs = fs.clone();
                                                        new_fs.selected_index = Some(0); new_fs.search_filter.clear(); *new_fs.table_state.offset_mut() = 0; new_fs.history = vec![new_fs.current_path.clone()]; new_fs.history_index = 0;
                                                        pane.open_tab(new_fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                                    }
                                                }
                                            }
                                            KeyCode::Char('g') => { if let Some(pane) = app.panes.get(app.focused_pane_index) { if let Some(fs) = pane.current_state() { spawn_terminal(&fs.current_path, true, fs.remote_session.as_ref()); } } } 
                                            KeyCode::Char(' ') => { app.input.clear(); app.mode = AppMode::CommandPalette; update_commands(app); } 
                                            _ => {} 
                                        }
                                        return;
                                    }
                                    if key.code == KeyCode::Esc {
                                        app.mode = AppMode::Normal;
                                        if let Some(fs) = app.current_file_state_mut() { fs.multi_select.clear(); fs.selection_anchor = None; if !fs.search_filter.is_empty() { fs.search_filter.clear(); fs.selected_index = Some(0); *fs.table_state.offset_mut() = 0; let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } 
                                    }                    match key.code {
                        KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => {
                            if let Some(fs) = app.current_file_state_mut() { navigate_back(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } 
                        }
                        KeyCode::Right if key.modifiers.contains(KeyModifiers::ALT) => {
                            if let Some(fs) = app.current_file_state_mut() { navigate_forward(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } 
                        }
                        KeyCode::Up if key.modifiers.contains(KeyModifiers::ALT) => {
                             crate::app::log_debug("Alt+Up pressed");
                             if app.sidebar_focus {
                                  if app.sidebar_index < app.sidebar_bounds.len() {
                                      let bound = &app.sidebar_bounds[app.sidebar_index];
                                      crate::app::log_debug(&format!("Current bound: {:?}", bound));
                                      if let SidebarTarget::Favorite(path) = &bound.target {
                                          if let Some(idx) = app.starred.iter().position(|p| p == path) {
                                              if idx > 0 {
                                                  app.starred.swap(idx, idx - 1);
                                                  if app.sidebar_index > 0 { app.sidebar_index -= 1; }
                                                  let _ = crate::config::save_state(app);
                                              }
                                          }
                                      }
                                  }
                             }
                        }
                        KeyCode::Down if key.modifiers.contains(KeyModifiers::ALT) => {
                             crate::app::log_debug("Alt+Down pressed");
                             if app.sidebar_focus {
                                  if app.sidebar_index < app.sidebar_bounds.len() {
                                      let bound = &app.sidebar_bounds[app.sidebar_index];
                                      if let SidebarTarget::Favorite(path) = &bound.target {
                                          if let Some(idx) = app.starred.iter().position(|p| p == path) {
                                              if idx < app.starred.len() - 1 {
                                                  app.starred.swap(idx, idx + 1);
                                                  app.sidebar_index += 1;
                                                  let _ = crate::config::save_state(app);
                                              }
                                          }
                                      }
                                  }
                             }
                        }
                        KeyCode::Down => { app.move_down(key.modifiers.contains(KeyModifiers::SHIFT)); }
                        KeyCode::Up => { app.move_up(key.modifiers.contains(KeyModifiers::SHIFT)); }
                        KeyCode::Left => { if key.modifiers.contains(KeyModifiers::SHIFT) { app.copy_to_other_pane(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); } else if key.modifiers.contains(KeyModifiers::CONTROL) { app.move_to_other_pane(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); } else { app.move_left(); } } 
                        KeyCode::Right => { if key.modifiers.contains(KeyModifiers::SHIFT) { app.copy_to_other_pane(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); } else if key.modifiers.contains(KeyModifiers::CONTROL) { app.move_to_other_pane(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); } else { app.move_right(); } } 
                        KeyCode::Enter => { if let Some(fs) = app.current_file_state_mut() { if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx).cloned() { if path.is_dir() { fs.current_path = path.clone(); fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } } } } 
                        KeyCode::Char(' ') => { if let Some(fs) = app.current_file_state() { if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx).cloned() { if app.starred.contains(&path) { app.starred.retain(|x| x != &path); } else { app.starred.push(path.clone()); } } } } } 
                        KeyCode::Char(c) if key.modifiers.is_empty() && c != '.' && c != 'g' => { if let Some(fs) = app.current_file_state_mut() { fs.search_filter.push(c); fs.selected_index = Some(0); *fs.table_state.offset_mut() = 0; let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } 
                        KeyCode::Char(c) if key.modifiers.is_empty() && (c == '.' || c == 'g') => { if let Some(pane) = app.panes.get(app.focused_pane_index) { if let Some(fs) = pane.current_state() { spawn_terminal(&fs.current_path, c == 'g', fs.remote_session.as_ref()); } } }                        KeyCode::Backspace => { if let Some(fs) = app.current_file_state_mut() { if !fs.search_filter.is_empty() { fs.search_filter.pop(); fs.selected_index = Some(0); *fs.table_state.offset_mut() = 0; let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } else if let Some(parent) = fs.current_path.parent() { let p = parent.to_path_buf(); fs.current_path = p.clone(); fs.selected_index = Some(0); *fs.table_state.offset_mut() = 0; push_history(fs, p); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } } 
                        _ => {} 
                    }
                }
            }
        }
        Event::Mouse(me) => {
            let column = me.column;
            let row = me.row;
            match me.kind {
                MouseEventKind::Down(button) => {
                    crate::app::log_debug(&format!("MOUSE DOWN: button={:?} row={} col={}", button, row, column));
                    let (w, h) = app.terminal_size;
                    
                    // 0. Modal interaction
                    if app.mode == AppMode::Settings {
                        let area_w = (w as f32 * 0.8) as u16; let area_h = (h as f32 * 0.8) as u16; let area_x = (w - area_w) / 2; let area_y = (h - area_h) / 2;
                        if column >= area_x && column < area_x + area_w && row >= area_y && row < area_y + area_h {
                            let inner = ratatui::layout::Rect::new(area_x + 1, area_y + 1, area_w.saturating_sub(2), area_h.saturating_sub(2));
                            if column < inner.x + 15 {
                                let rel_y = row.saturating_sub(inner.y + 1);
                                match rel_y {
                                    0 => app.settings_section = SettingsSection::Columns,
                                    1 => app.settings_section = SettingsSection::Tabs,
                                    2 => app.settings_section = SettingsSection::General,
                                    3 => app.settings_section = SettingsSection::Remotes,
                                    _ => {} 
                                }
                            } else {
                                match app.settings_section {
                                    SettingsSection::Columns => {
                                        if row >= inner.y && row < inner.y + 3 {
                                            let content_x = column.saturating_sub(inner.x + 15);
                                            if content_x < 12 { app.settings_target = SettingsTarget::SingleMode; } else if content_x < 25 { app.settings_target = SettingsTarget::SplitMode; }
                                        } else if row >= inner.y + 4 {
                                            let rel_y = row.saturating_sub(inner.y + 5);
                                            match rel_y { 0 => app.toggle_column(crate::app::FileColumn::Size), 1 => app.toggle_column(crate::app::FileColumn::Modified), 2 => app.toggle_column(crate::app::FileColumn::Permissions), _ => {} } // TODO: Add Name toggle
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                        }
                                    }
                                    SettingsSection::General => {
                                        let rel_y = row.saturating_sub(inner.y + 2);
                                        match rel_y { 0 => app.default_show_hidden = !app.default_show_hidden, 1 => app.confirm_delete = !app.confirm_delete, _ => {} } // TODO: Add other general settings
                                    }
                                    _ => {} 
                                }
                            }
                            return;
                        } else { app.mode = AppMode::Normal; } // Fall through
                    }

                    if app.mode == AppMode::ImportServers {
                        let area_w = (w as f32 * 0.6) as u16; let area_h = (h as f32 * 0.2) as u16; let area_x = (w - area_w) / 2; let area_y = (h - area_h) / 2;
                        if column >= area_x && column < area_x + area_w && row >= area_y && row < area_y + area_h { return; } // Clicked inside modal
                        else {
                            let mut handled = false;
                            if row >= 3 { // Check if clicked on file list
                                let index = fs_mouse_index(row, app);
                                if let Some(fs) = app.current_file_state() { if index < fs.files.len() { let path = &fs.files[index]; if path.extension().map(|e| e == "toml").unwrap_or(false) { app.input.set_value(path.file_name().unwrap_or_default().to_string_lossy().to_string()); handled = true; } } } 
                            }
                            if !handled { app.mode = AppMode::Normal; } // Clicked outside modal and not on a toml file
                            if !handled && button == MouseButton::Left { return; } // Prevent default action if not handled
                        }
                    }

                    if matches!(app.mode, AppMode::NewFile | AppMode::NewFolder | AppMode::Rename | AppMode::Delete) {
                        let area_w = (w as f32 * 0.4) as u16; let area_h = (h as f32 * 0.1) as u16; let area_x = (w - area_w) / 2; let area_y = (h - area_h) / 2;
                        if column < area_x || column >= area_x + area_w || row < area_y || row >= area_y + area_h {
                            app.mode = AppMode::Normal; app.input.clear();
                            return;
                        }
                    }

                    if matches!(app.mode, AppMode::Properties) {
                        let area_w = (w as f32 * 0.5) as u16; let area_h = (h as f32 * 0.5) as u16; let area_x = (w - area_w) / 2; let area_y = (h - area_h) / 2;
                        if column < area_x || column >= area_x + area_w || row < area_y || row >= area_y + area_h {
                            app.mode = AppMode::Normal;
                            return;
                        }
                    }

                    if matches!(app.mode, AppMode::CommandPalette) {
                        let area_w = (w as f32 * 0.6) as u16; let area_h = (h as f32 * 0.2) as u16; let area_x = (w - area_w) / 2; let area_y = (h - area_h) / 2;
                        if column < area_x || column >= area_x + area_w || row < area_y || row >= area_y + area_h {
                            app.mode = AppMode::Normal; app.input.clear();
                            return;
                        }
                    }

                    if matches!(app.mode, AppMode::AddRemote) {
                        let area_w = (w as f32 * 0.6) as u16; let area_h = (h as f32 * 0.4) as u16; let area_x = (w - area_w) / 2; let area_y = (h - area_h) / 2;
                        if column < area_x || column >= area_x + area_w || row < area_y || row >= area_y + area_h {
                            app.mode = AppMode::Normal; app.input.clear();
                            return;
                        }
                    }

                    if let AppMode::ContextMenu { x, y, target } = app.mode.clone() {
                        let items_count = match target {
                            ContextMenuTarget::File(_) => 5, ContextMenuTarget::Folder(_) => 4, ContextMenuTarget::EmptySpace => 4,
                            ContextMenuTarget::SidebarFavorite(_) => 2, ContextMenuTarget::SidebarRemote(_) => 2,
                            ContextMenuTarget::SidebarStorage(idx) => { if app.system_state.disks.get(idx).map(|d| d.is_mounted).unwrap_or(false) { 2 } else { 1 } }
                        };
                        let menu_width = 22; let menu_height = items_count as u16 + 2;
                        let mut draw_x = x; let mut draw_y = y;
                        if draw_x + menu_width > w { draw_x = w.saturating_sub(menu_width); }
                        if draw_y + menu_height > h { draw_y = h.saturating_sub(menu_height); }

                        if column >= draw_x && column < draw_x + menu_width && row >= draw_y && row < draw_y + menu_height {
                            let menu_row = row.saturating_sub(draw_y + 1) as usize;
                            match target {
                                ContextMenuTarget::File(idx) => { if let Some(fs) = app.current_file_state_mut() { if let Some(path) = fs.files.get(idx).cloned() { match menu_row { 0 => { let _ = std::process::Command::new("xdg-open").arg(&path).spawn(); app.mode = AppMode::Normal; } 1 => { app.mode = AppMode::Rename; app.input.set_value(path.file_name().unwrap_or_default().to_string_lossy().to_string()); app.rename_selected = true; } 2 => app.mode = AppMode::Delete, 3 => app.mode = AppMode::Properties, _ => app.mode = AppMode::Normal, } } } } 
                                ContextMenuTarget::Folder(idx) => { if let Some(fs) = app.current_file_state_mut() { if let Some(path) = fs.files.get(idx).cloned() { match menu_row { 0 => { fs.current_path = path.clone(); fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); app.mode = AppMode::Normal; } 1 => { if app.starred.contains(&path) { app.starred.retain(|x| x != &path); } else { app.starred.push(path.clone()); } app.mode = AppMode::Normal; } 2 => { app.mode = AppMode::Rename; app.input.set_value(path.file_name().unwrap_or_default().to_string_lossy().to_string()); app.rename_selected = true; } 3 => app.mode = AppMode::Delete, _ => app.mode = AppMode::Normal, } } } } 
                                ContextMenuTarget::EmptySpace => { match menu_row { 0 => { app.mode = AppMode::NewFolder; app.input.clear(); }, 1 => { app.mode = AppMode::NewFile; app.input.clear(); }, 2 => { let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); app.mode = AppMode::Normal; }, 3 => { if let Some(fs) = app.current_file_state() { spawn_terminal(&fs.current_path, false, fs.remote_session.as_ref()); } app.mode = AppMode::Normal; } _ => app.mode = AppMode::Normal, } } 
                                ContextMenuTarget::SidebarFavorite(path) => { match menu_row { 0 => { if let Some(fs) = app.current_file_state_mut() { fs.current_path = path.clone(); fs.remote_session = None; fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path); } let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); app.mode = AppMode::Normal; } 1 => { app.starred.retain(|x| x != &path); app.mode = AppMode::Normal; } _ => app.mode = AppMode::Normal, } } 
                                ContextMenuTarget::SidebarRemote(idx) => { match menu_row { 0 => { execute_command(crate::app::CommandAction::ConnectToRemote(idx), app, event_tx.clone()); app.mode = AppMode::Normal; } 1 => { app.remote_bookmarks.remove(idx); app.mode = AppMode::Normal; } _ => app.mode = AppMode::Normal, } } 
                                ContextMenuTarget::SidebarStorage(idx) => {
                                    if let Some(disk) = app.system_state.disks.get(idx) {
                                        if disk.is_mounted { match menu_row { 0 => { let p = std::path::PathBuf::from(&disk.name); if let Some(fs) = app.current_file_state_mut() { fs.current_path = p.clone(); fs.remote_session = None; fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, p); } let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); app.mode = AppMode::Normal; } 1 => { let dev = disk.device.clone(); tokio::spawn(async move { let _ = std::process::Command::new("udisksctl").arg("unmount").arg("-b").arg(&dev).output(); }); app.mode = AppMode::Normal; } _ => app.mode = AppMode::Normal, } } 
                                        else { let dev = disk.device.clone(); let tx = event_tx.clone(); let p_idx = app.focused_pane_index; tokio::spawn(async move { if let Ok(out) = std::process::Command::new("udisksctl").arg("mount").arg("-b").arg(&dev).output() { if let Some(_) = String::from_utf8_lossy(&out.stdout).split(" at ").last() { tokio::time::sleep(Duration::from_millis(200)).await; let _ = tx.send(AppEvent::RefreshFiles(p_idx)).await; } } }); app.mode = AppMode::Normal; }
                                    }
                                }
                            }
                            return;
                        } else { app.mode = AppMode::Normal; } // Fall through
                    }

                    // 1. Header handling (Row 0)
                    if row == 0 {
                        let clicked_tab = app.tab_bounds.iter().find(|(rect, _, _)| rect.contains(ratatui::layout::Position { x: column, y: row })).cloned();
                        if let Some((_, p_idx, t_idx)) = clicked_tab {
                            if button == MouseButton::Left {
                                if let Some(pane) = app.panes.get_mut(p_idx) { pane.active_tab_index = t_idx; app.focused_pane_index = p_idx; app.sidebar_focus = false; let _ = event_tx.try_send(AppEvent::RefreshFiles(p_idx)); }
                            } else if button == MouseButton::Right {
                                if let Some(pane) = app.panes.get_mut(p_idx) {
                                    if pane.tabs.len() > 1 { pane.tabs.remove(t_idx); if pane.active_tab_index >= pane.tabs.len() { pane.active_tab_index = pane.tabs.len() - 1; } let _ = event_tx.try_send(AppEvent::RefreshFiles(p_idx)); }
                                }
                            }
                            return;
                        }
                        if column < 10 { app.mode = AppMode::Settings; return; }
                        if column >= w.saturating_sub(3) { app.toggle_split(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); return; }
                    }

                    // 2. Normal interaction
                    // Check Breadcrumbs
                    for (p_idx, pane) in app.panes.iter_mut().enumerate() {
                        if let Some(fs) = pane.current_state_mut() {
                            let clicked_crumb = fs.breadcrumb_bounds.iter().find(|(rect, _)| rect.contains(ratatui::layout::Position { x: column, y: row })).map(|(_, path)| path.clone());
                            if let Some(path) = clicked_crumb {
                                fs.current_path = path.clone(); fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path);
                                let _ = event_tx.try_send(AppEvent::RefreshFiles(p_idx)); app.focused_pane_index = p_idx; app.sidebar_focus = false; return;
                            }
                        }
                    }

                    let sidebar_width = app.sidebar_width();
                    
                    // Update pane focus
                    if column >= sidebar_width {
                        let content_area_width = w.saturating_sub(sidebar_width);
                        let pane_count = app.panes.len();
                        let pane_width = if pane_count > 0 { content_area_width / pane_count as u16 } else { content_area_width };
                        let clicked_pane = (column.saturating_sub(sidebar_width) / pane_width) as usize;
                        if clicked_pane < pane_count { app.focused_pane_index = clicked_pane; app.sidebar_focus = false; }
                    }

                    if column < sidebar_width {
                        app.sidebar_focus = true;
                        app.drag_start_pos = Some((column, row));
                        let clicked_sidebar_item = app.sidebar_bounds.iter().find(|b| b.y == row).cloned();
                        if let Some(bound) = clicked_sidebar_item {
                            app.sidebar_index = bound.index;
                            if button == MouseButton::Right {
                                let target = match &bound.target {
                                    SidebarTarget::Favorite(p) => Some(ContextMenuTarget::SidebarFavorite(p.clone())),
                                    SidebarTarget::Remote(idx) => Some(ContextMenuTarget::SidebarRemote(*idx)),
                                    SidebarTarget::Storage(idx) => Some(ContextMenuTarget::SidebarStorage(*idx)),
                                    _ => None
                                };
                                if let Some(t) = target { app.mode = AppMode::ContextMenu { x: column, y: row, target: t }; return; }
                            }
                            match &bound.target {
                                SidebarTarget::Header(h) if h == "REMOTES" => { app.mode = AppMode::ImportServers; app.input.set_value("servers.toml".to_string()); }
                                SidebarTarget::Favorite(p) => { let p2 = p.clone(); if let Some(fs) = app.current_file_state_mut() { fs.current_path = p2.clone(); fs.remote_session = None; fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, p2); } let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); app.sidebar_focus = false; }
                                SidebarTarget::Storage(idx) => {
                                    if let Some(disk) = app.system_state.disks.get(*idx) {
                                        if !disk.is_mounted {
                                            let dev = disk.device.clone(); let tx = event_tx.clone(); let pane_idx = app.focused_pane_index;
                                            tokio::spawn(async move { if let Ok(out) = std::process::Command::new("udisksctl").arg("mount").arg("-b").arg(&dev).output() { if let Some(_) = String::from_utf8_lossy(&out.stdout).split(" at ").last() { tokio::time::sleep(Duration::from_millis(200)).await; let _ = tx.send(AppEvent::RefreshFiles(pane_idx)).await; } } });
                                        } else {
                                            let p = std::path::PathBuf::from(&disk.name);
                                            if let Some(fs) = app.current_file_state_mut() { fs.current_path = p.clone(); fs.remote_session = None; fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, p); }
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); app.sidebar_focus = false;
                                        }
                                    }
                                }
                                SidebarTarget::Remote(idx) => { execute_command(crate::app::CommandAction::ConnectToRemote(*idx), app, event_tx.clone()); app.sidebar_focus = false; }
                                _ => {} 
                            }
                        }
                        return;
                    }
                    
                    if row >= 3 {
                        let index = fs_mouse_index(row, app);
                        let mut selected_path = None; let mut is_dir = false;
                        if let Some(fs) = app.current_file_state_mut() {
                            if index < fs.files.len() {
                                if fs.files[index].to_string_lossy() == "__DIVIDER__" { return; } // Ignore dividers
                                fs.selected_index = Some(index); fs.table_state.select(Some(index));
                                let p = fs.files[index].clone(); is_dir = fs.metadata.get(&p).map(|m| m.is_dir).unwrap_or(false); selected_path = Some(p);
                            } else if button == MouseButton::Right { app.mode = AppMode::ContextMenu { x: column, y: row, target: ContextMenuTarget::EmptySpace }; return; } // Right click on empty space
                        }
                        if let Some(path) = selected_path {
                            if button == MouseButton::Right { let target = if is_dir { ContextMenuTarget::Folder(index) } else { ContextMenuTarget::File(index) }; app.mode = AppMode::ContextMenu { x: column, y: row, target }; return; }
                            app.drag_source = Some(path.clone()); app.drag_start_pos = Some((column, row));
                            // Double click detection
                            if button == MouseButton::Left && app.mouse_last_click.elapsed() < Duration::from_millis(500) && app.mouse_click_pos == (column, row) {
                                if path.is_dir() { if let Some(fs) = app.current_file_state_mut() { fs.current_path = path.clone(); fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } 
                                else { let _ = std::process::Command::new("xdg-open").arg(&path).spawn(); } 
                            }
                            app.mouse_last_click = std::time::Instant::now(); app.mouse_click_pos = (column, row);
                        }
                    } else if row >= 1 && button == MouseButton::Right { // Right click above file list but below header
                        app.mode = AppMode::ContextMenu { x: column, y: row, target: ContextMenuTarget::EmptySpace };
                    }
                }
                                MouseEventKind::Up(_) => {
                                    if app.is_dragging {
                                        let mut reorder_done = false;
                                        if let Some((sx, sy)) = app.drag_start_pos {
                                            let (ex, ey) = (column, row);
                                            crate::app::log_debug(&format!("Mouse Up Drag: sx={} sy={} ex={} ey={}", sx, sy, ex, ey));
                                            if sx < app.sidebar_width() && ex < app.sidebar_width() {
                                                 if let Some(start_bound) = app.sidebar_bounds.iter().find(|b| b.y == sy).cloned() {
                                                     crate::app::log_debug(&format!("Start bound: {:?}", start_bound));
                                                     if let SidebarTarget::Favorite(start_path) = start_bound.target {
                                                         if let Some(end_bound) = app.sidebar_bounds.iter().find(|b| b.y == ey).cloned() {
                                                             crate::app::log_debug(&format!("End bound: {:?}", end_bound));
                                                             if let SidebarTarget::Favorite(end_path) = end_bound.target {
                                                                 if let Some(s_idx) = app.starred.iter().position(|p| p == &start_path) {
                                                                     if let Some(e_idx) = app.starred.iter().position(|p| p == &end_path) {
                                                                         crate::app::log_debug(&format!("Swapping {} with {}", s_idx, e_idx));
                                                                         if s_idx != e_idx {
                                                                             let item = app.starred.remove(s_idx);
                                                                             app.starred.insert(e_idx, item);
                                                                             let _ = crate::config::save_state(app);
                                                                             reorder_done = true;
                                                                         }
                                                                     }
                                                                 }
                                                             }
                                                         }
                                                     }
                                                 }
                                            }
                                        }

                                        if !reorder_done {
                                            if let Some(source) = &app.drag_source {
                                                if let Some(target) = &app.hovered_drop_target {
                                                    match target {
                                                        DropTarget::ImportServers | DropTarget::RemotesHeader => {
                                                            if source.extension().map(|e| e == "toml").unwrap_or(false) {
                                                                let _ = app.import_servers(source.clone());
                                                                let _ = crate::config::save_state(app);
                                                                app.mode = AppMode::Normal;
                                                            }
                                                        }
                                                        DropTarget::Favorites => {
                                                            if source.is_dir() { if !app.starred.contains(source) { app.starred.push(source.clone()); let _ = crate::config::save_state(app); } } // Add to favorites if it's a directory
                                                        }
                                                        _ => {} 
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    app.is_dragging = false; app.drag_start_pos = None; app.drag_source = None;
                                }
                                MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                                    app.mouse_pos = (column, row);
                                    // Check if drag has started
                                    if let Some((sx, sy)) = app.drag_start_pos { if ((column as i16 - sx as i16).pow(2) + (row as i16 - sy as i16).pow(2)) as f32 >= 1.0 { app.is_dragging = true; } } // Threshold for drag start
                                    // Update hovered drop target
                                    if app.is_dragging {
                                        if app.mode == AppMode::ImportServers {
                                            let (w, h) = app.terminal_size; let area_w = (w as f32 * 0.6) as u16; let area_h = (h as f32 * 0.2) as u16; let area_x = (w - area_w) / 2; let area_y = (h - area_h) / 2;
                                            if column >= area_x && column < area_x + area_w && row >= area_y && row < area_y + area_h { app.hovered_drop_target = Some(DropTarget::ImportServers); } else { app.hovered_drop_target = None; } // Inside import modal
                                        } else {
                                            let sidebar_width = app.sidebar_width();
                                            if column < sidebar_width {
                                                // Check if hovering over REMOTES header specifically
                                                if let Some(bound) = app.sidebar_bounds.iter().find(|b| b.y == row) {
                                                    if let SidebarTarget::Header(h) = &bound.target {
                                                        if h == "REMOTES" { app.hovered_drop_target = Some(DropTarget::RemotesHeader); } else { app.hovered_drop_target = Some(DropTarget::Favorites); } // Default to favorites if not REMOTES header
                                                    } else { app.hovered_drop_target = Some(DropTarget::Favorites); } // Hovering over a favorite item
                                                } else { app.hovered_drop_target = Some(DropTarget::Favorites); } // Fallback to favorites if no specific bound found
                                            } else { app.hovered_drop_target = None; } // Not in sidebar
                                        }
                                    }
                                }
                                MouseEventKind::ScrollUp => { if let Some(fs) = app.current_file_state_mut() { let new_offset = fs.table_state.offset().saturating_sub(3); *fs.table_state.offset_mut() = new_offset; } } 
                MouseEventKind::ScrollDown => { if let Some(fs) = app.current_file_state_mut() { let max_offset = fs.files.len().saturating_sub(fs.view_height.saturating_sub(4)); let new_offset = (fs.table_state.offset() + 3).min(max_offset); *fs.table_state.offset_mut() = new_offset; } } 
                _ => {} 
            }
        }
        _ => {} 
    }
}