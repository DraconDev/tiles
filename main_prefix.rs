use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use std::os::unix::process::CommandExt;

// Terma Imports
use terma::integration::ratatui::TermaBackend;
use terma::input::event::{Event, KeyCode, MouseEventKind, KeyModifiers, MouseButton};

// Ratatui Imports
use ratatui::Terminal;

use crate::app::{App, AppMode, CommandItem, AppEvent, SidebarTarget, ContextMenuTarget, ContextMenuAction, FileCategory, SettingsSection, SettingsTarget, DropTarget};

mod app;
mod config;
mod modules;
mod ui;
mod license;
mod icons;

use crate::icons::IconMode;

fn get_context_menu_actions(target: &ContextMenuTarget, app: &App) -> Vec<ContextMenuAction> {
    match target {
        ContextMenuTarget::File(idx) => {
            // Check if multiple items are selected
            let mut actions = Vec::new();
            if let Some(fs) = app.current_file_state() {
                if !fs.multi_select.is_empty() && (fs.multi_select.contains(idx) || fs.multi_select.len() > 1) {
                     // Multi-selection Context Menu
                     actions.push(ContextMenuAction::Cut);
                     actions.push(ContextMenuAction::Copy);
                     actions.push(ContextMenuAction::Delete);
                     actions.push(ContextMenuAction::Compress);
                     return actions;
                }

                if let Some(path) = fs.files.get(*idx) {
                    let _is_starred = app.starred.contains(path);
                    
                    actions.push(ContextMenuAction::Open);
                    actions.push(ContextMenuAction::OpenWith);
                    actions.push(ContextMenuAction::Edit);
                    
                    // Categorized actions
                    let cat = crate::modules::files::get_file_category(path);
                    match cat {
                        FileCategory::Archive => actions.push(ContextMenuAction::ExtractHere),
                        FileCategory::Script => {
                            actions.push(ContextMenuAction::Run);
                            actions.push(ContextMenuAction::RunTerminal);
                        }
                        FileCategory::Audio | FileCategory::Video => {
                            actions.push(ContextMenuAction::Run); // "Play"
                        }
                        FileCategory::Image => {
                            actions.push(ContextMenuAction::SetWallpaper);
                        }
                        _ => {}
                    }
                    
                    actions.extend(vec![
                        ContextMenuAction::Cut,
                        ContextMenuAction::Copy,
                        ContextMenuAction::CopyPath,
                        ContextMenuAction::CopyName,
                        ContextMenuAction::Rename,
                        ContextMenuAction::Duplicate,
                        ContextMenuAction::Compress,
                        ContextMenuAction::Delete,
                    ]);

                        actions.push(ContextMenuAction::TerminalWindow);
                        actions.push(ContextMenuAction::SetColor(None));
                        actions.push(ContextMenuAction::Properties);                }
            }
            actions
        }
        ContextMenuTarget::Folder(idx) => {
             let mut actions = Vec::new();
             // Check Multi-select first
             if let Some(fs) = app.current_file_state() {
                if !fs.multi_select.is_empty() && (fs.multi_select.contains(idx) || fs.multi_select.len() > 1) {
                     actions.push(ContextMenuAction::Cut);
                     actions.push(ContextMenuAction::Copy);
                     actions.push(ContextMenuAction::Delete);
                     actions.push(ContextMenuAction::Compress);
                     return actions;
                }
                
                if let Some(path) = fs.files.get(*idx) {
                    actions.push(ContextMenuAction::Open);
                    actions.push(ContextMenuAction::OpenNewTab);
                    actions.push(ContextMenuAction::NewFolder);
                    actions.push(ContextMenuAction::NewFile);
            actions.push(ContextMenuAction::TerminalWindow);
                    
                    if !app.starred.contains(path) {
                        actions.push(ContextMenuAction::AddToFavorites);
                    } else {
                        actions.push(ContextMenuAction::RemoveFromFavorites);
                    }

                    actions.extend(vec![
                        ContextMenuAction::Cut,
                        ContextMenuAction::Copy,
                        ContextMenuAction::CopyPath,
                        ContextMenuAction::CopyName,
                        ContextMenuAction::Paste,
                        ContextMenuAction::Rename,
                        ContextMenuAction::Duplicate,
                        ContextMenuAction::Compress,
                        ContextMenuAction::Delete,
                    ]);
                    
                    if path.join(".git").exists() {
                        actions.push(ContextMenuAction::GitStatus);
                    } else {
                        actions.push(ContextMenuAction::GitInit);
                    }
                    
                    actions.push(ContextMenuAction::SetColor(None));
                    actions.push(ContextMenuAction::Properties);
                }
             }
             actions
        }
        ContextMenuTarget::EmptySpace => {
            let mut actions = vec![
                ContextMenuAction::NewFolder,
                ContextMenuAction::NewFile,
            ];
            
            if app.clipboard.is_some() {
                actions.push(ContextMenuAction::Paste);
            }
            
            actions.push(ContextMenuAction::SelectAll);
            actions.push(ContextMenuAction::Refresh);
            
            actions.push(ContextMenuAction::TerminalWindow);
            
            if let Some(fs) = app.current_file_state() {
                if fs.current_path.join(".git").exists() {
                    actions.push(ContextMenuAction::GitStatus);
                }
            }
            actions.push(ContextMenuAction::Properties); // Folder properties
            actions
        }
        ContextMenuTarget::SidebarFavorite(_) => vec![
            ContextMenuAction::Open,
            ContextMenuAction::OpenNewTab,
            ContextMenuAction::TerminalWindow,
            ContextMenuAction::RemoveFromFavorites,
        ],
        ContextMenuTarget::SidebarRemote(_) => vec![
            ContextMenuAction::ConnectRemote,
            ContextMenuAction::TerminalWindow,
            ContextMenuAction::DeleteRemote,
        ],
        ContextMenuTarget::SidebarStorage(idx) => {
            let mut actions = vec![];
            if let Some(disk) = app.system_state.disks.get(*idx) {
                if disk.is_mounted {
                    actions.push(ContextMenuAction::Open);
                    actions.push(ContextMenuAction::TerminalWindow);
                    actions.push(ContextMenuAction::Unmount);
                } else {
                    actions.push(ContextMenuAction::Mount);
                }
            }
            actions
        }
    }
}

#[tokio::main] async fn main() -> Result<(), Box<dyn std::error::Error>> {
    crate::app::log_debug("main start");
    
    // Initialize TermaBackend
    let backend = TermaBackend::new(std::io::stdout())?;
    let tile_queue = backend.tile_queue();
    let mut terminal = Terminal::new(backend)?;

    // Setup App & Channels
    let app = Arc::new(Mutex::new(App::new(tile_queue)));
    
    // Check for resize request from saved state
    {
        let app_guard = app.lock().unwrap();
        if let Some((w, h)) = app_guard.initial_window_size {
             use std::io::Write;
             // OSC 8; rows; cols t
             print!("\x1b[8;{};{}t", h, w);
             // Enable modifyOtherKeys level 2
             print!("\x1b[>4;2m");
             let _ = std::io::stdout().flush();
        } else {
             use std::io::Write;
             // Enable modifyOtherKeys level 2
             print!("\x1b[>4;2m");
             let _ = std::io::stdout().flush();
        }
    }

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
                tokio::time::sleep(Duration::from_millis(100)).await;
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
        // 1. Process All Pending Events
        let mut needs_draw = false;
        while let Ok(event) = event_rx.try_recv() {
            match event {
                AppEvent::Raw(raw) => {
                    let mut app_guard = app.lock().unwrap();
                    match raw {
                        Event::FocusGained | Event::FocusLost => { /* Ignore focus events for drawing */ }
                        _ => {
                            if handle_event(raw, &mut app_guard, event_tx.clone()) {
                                needs_draw = true;
                            }
                        }
                    }
                }
                AppEvent::SystemUpdated(data) => {
                    let mut app_guard = app.lock().unwrap();
                    app_guard.system_state.cpu_usage = data.cpu_usage;
                    app_guard.system_state.mem_usage = data.mem_usage;
                    app_guard.system_state.total_mem = data.total_mem;
                    app_guard.system_state.disks = data.disks;
                    app_guard.system_state.processes = data.processes;
                    needs_draw = true;
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
                        }
                        else {
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
                            needs_draw = true;
                        }
                    }
                }
                AppEvent::Delete(path) => {
                    let res = if path.is_dir() {
                        std::fs::remove_dir_all(&path)
                    } else {
                        std::fs::remove_file(&path)
                    };
                    
                    if let Err(e) = res {
                        crate::app::log_debug(&format!("DELETE FAILED for {:?}: {}", path, e));
                    } else {
                        crate::app::log_debug(&format!("DELETED {:?}", path));
                    }

                    let mut app_guard = app.lock().unwrap();
                    for i in 0..app_guard.panes.len() {
                        app_guard.update_files_for_active_tab(i);
                    }
                    needs_draw = true;
                }
                AppEvent::Rename(old, new) => {
                    let _ = std::fs::rename(&old, &new);
                    let mut app_guard = app.lock().unwrap();
                    app_guard.undo_stack.push(crate::app::UndoAction::Rename(old.clone(), new.clone()));
                    app_guard.redo_stack.clear();
                    for i in 0..app_guard.panes.len() {
                        app_guard.update_files_for_active_tab(i);
                    }
                    needs_draw = true;
                }
                AppEvent::Copy(src, dest) => {
                    let _ = crate::modules::files::copy_recursive(&src, &dest);
                    let mut app_guard = app.lock().unwrap();
                    app_guard.undo_stack.push(crate::app::UndoAction::Copy(src.clone(), dest.clone()));
                    app_guard.redo_stack.clear();
                    for i in 0..app_guard.panes.len() {
                        app_guard.update_files_for_active_tab(i);
                    }
                    needs_draw = true;
                }
                AppEvent::CreateFile(path) => {
                    let _ = std::fs::File::create(&path);
                    let mut app_guard = app.lock().unwrap();
                    for i in 0..app_guard.panes.len() {
                        app_guard.update_files_for_active_tab(i);
                    }
                    needs_draw = true;
                }
                AppEvent::CreateFolder(path) => {
                    let _ = std::fs::create_dir(&path);
                    let mut app_guard = app.lock().unwrap();
                    for i in 0..app_guard.panes.len() {
                        app_guard.update_files_for_active_tab(i);
                    }
                    needs_draw = true;
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
                    needs_draw = true;
                }
                AppEvent::PreviewRequested(target_pane_idx, path) => {
                    let app_clone = app.clone();
                    let tx = event_tx.clone();
                    tokio::spawn(async move {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let preview_content = content.chars().take(2000).collect::<String>();
                            let mut app_guard = app_clone.lock().unwrap();
                            if app_guard.panes.len() < 2 { app_guard.toggle_split(); }
                            if let Some(pane) = app_guard.panes.get_mut(target_pane_idx) {
                                pane.preview = Some(crate::app::PreviewState { path, content: preview_content, scroll: 0 });
                            }
                        } else if path.is_dir() {
                             {
                                 let mut app_guard = app_clone.lock().unwrap();
                                 if app_guard.panes.len() < 2 { app_guard.toggle_split(); }
                                 if let Some(pane) = app_guard.panes.get_mut(target_pane_idx) {
                                     if let Some(fs) = pane.current_state_mut() {
                                         fs.current_path = path.clone(); fs.selected_index = Some(0); fs.multi_select.clear(); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path);
                                     }
                                 }
                             }
                             let _ = tx.send(AppEvent::RefreshFiles(target_pane_idx)).await;
                        }
                    });
                    needs_draw = true;
                }
                AppEvent::SpawnTerminal { path, new_tab, remote, command } => {
                    let preferred = {
                        let app_guard = app.lock().unwrap();
                        app_guard.preferred_terminal.clone()
                    };
                    tokio::spawn(async move {
                        spawn_terminal(&path, new_tab, remote.as_ref(), preferred.as_deref(), command.as_deref());
                    });
                }
                AppEvent::SpawnDetached { cmd, args } => {
                    tokio::spawn(async move {
                        spawn_detached(&cmd, args.iter().map(|s| s.as_str()).collect());
                    });
                }
                AppEvent::StatusMsg(msg) => {
                    let mut app_guard = app.lock().unwrap();
                    app_guard.last_action_msg = Some((msg, std::time::Instant::now()));
                    needs_draw = true;
                }
                AppEvent::Tick => { needs_draw = true; } 
            }
        }

        // 2. Draw if needed
        if needs_draw {
            let mut app_guard = app.lock().unwrap();
            if !app_guard.running { 
                let _ = crate::config::save_state(&app_guard);
                // Disable modifyOtherKeys
                use std::io::Write;
                print!("\x1b[>4;0m");
                let _ = std::io::stdout().flush();
                break; 
            }
            terminal.draw(|f| {
                ui::draw(f, &mut app_guard);
            })?;
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
        fs.multi_select.clear();
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
        fs.multi_select.clear();
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
        crate::app::CommandAction::ToggleZoom => {},
        crate::app::CommandAction::SwitchView(view) => app.current_view = view,
        crate::app::CommandAction::AddRemote => { app.mode = AppMode::AddRemote(0); app.input.clear(); },
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

fn spawn_detached(cmd: &str, args: Vec<&str>) {
    let mut command = std::process::Command::new(cmd);
    command.args(args);
    unsafe {
        let _ = command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .pre_exec(|| { libc::setsid(); Ok(()) })
            .spawn();
    }
}

fn handle_context_menu_action(action: &ContextMenuAction, target: &ContextMenuTarget, app: &mut App, event_tx: mpsc::Sender<AppEvent>) {
    use std::path::Path;
    
    // Helper to get selected paths including multi-select
    let get_targets = |app: &mut App, target_idx: Option<usize>| -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();
        if let Some(fs) = app.current_file_state() {
             if let Some(idx) = target_idx {
                 if fs.multi_select.contains(&idx) {
                     for &i in &fs.multi_select {
                         if let Some(p) = fs.files.get(i) { paths.push(p.clone()); }
                     }
                 } else {
                     if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); }
                 }
             }
        }
        paths
    };

    // We do NOT set app.mode = AppMode::Normal here at the end unconditionally.

    // Each action that finishes the interaction should set it to Normal.

    // Actions that start a new mode (NewFolder, Rename, etc.) will leave it in that mode.

    

    // Default to closing menu unless specified otherwise

    let mut close_menu = true;



    match action {

        ContextMenuAction::SortBy(_) => { /* Removed/No-op if triggered */ }

        ContextMenuAction::AddToFavorites => {

            let path = match target {

                ContextMenuTarget::Folder(idx) | ContextMenuTarget::File(idx) => app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned()),

                _ => None,

            };

            if let Some(path) = path {

                if !app.starred.contains(&path) {

                    app.starred.push(path);

                    let _ = crate::config::save_state(app);

                }

            }

        }

                ContextMenuAction::RemoveFromFavorites => {

                    let path = match target {

                        ContextMenuTarget::Folder(idx) | ContextMenuTarget::File(idx) => app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned()),

                        ContextMenuTarget::SidebarFavorite(p) => Some(p.clone()),

                        _ => None,

                    };

                    if let Some(path) = path {

                        app.starred.retain(|x| x != &path);

                        let _ = crate::config::save_state(app);

                    }

                }

                                ContextMenuAction::SetColor(color) => {

                                    if let Some(c) = color {

                                        let paths = match target {

                                            ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) => get_targets(app, Some(*idx)),

                                            _ => vec![],

                                        };

                                        for p in paths {

                                            app.path_colors.insert(p, *c);

                                        }

                                        let _ = crate::config::save_state(app);

                                    } else {

                        // Open Highlight modal

                        app.mode = AppMode::Highlight;

                        close_menu = false;

                    }

                }

                                ContextMenuAction::Open => {

                                    match target {

                                        ContextMenuTarget::File(idx) => {

                                            if let Some(fs) = app.current_file_state() {

                                                if let Some(path) = fs.files.get(*idx) {

                                                    let _ = event_tx.try_send(AppEvent::SpawnDetached {

                                                        cmd: "xdg-open".to_string(),

                                                        args: vec![path.to_string_lossy().to_string()],

                                                    });

                                                }

                                            }

                                        }

                ContextMenuTarget::Folder(idx) => {

                    if let Some(fs) = app.current_file_state_mut() {

                        if let Some(path) = fs.files.get(*idx).cloned() {

                            fs.current_path = path.clone();

                            fs.selected_index = Some(0);

                            fs.search_filter.clear();

                            *fs.table_state.offset_mut() = 0;

                            push_history(fs, path.clone());

                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));

                        }

                    }

                }

                ContextMenuTarget::SidebarFavorite(path) => {

                    if let Some(fs) = app.current_file_state_mut() {

                        fs.current_path = path.clone();

                        fs.remote_session = None;

                        fs.selected_index = Some(0);

                        fs.search_filter.clear();

                        *fs.table_state.offset_mut() = 0;

                        push_history(fs, path.clone());

                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));

                    }

                }

                ContextMenuTarget::SidebarStorage(idx) => {

                    if let Some(disk) = app.system_state.disks.get(*idx) {

                        if disk.is_mounted {

                            let p = std::path::PathBuf::from(&disk.name);

                            if let Some(fs) = app.current_file_state_mut() {

                                fs.current_path = p.clone();

                                fs.remote_session = None;

                                fs.selected_index = Some(0);

                                fs.search_filter.clear();

                                *fs.table_state.offset_mut() = 0;

                                push_history(fs, p);

                                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));

                            }

                        }

                    }

                }

                _ => {}

            }

        }

                ContextMenuAction::OpenNewTab => {

                    let path = match target {

                        ContextMenuTarget::Folder(idx) => app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned()),

                        ContextMenuTarget::SidebarFavorite(p) => Some(p.clone()),

                        _ => None,

                    };

        

                    if let Some(path) = path {

                        if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {

                            if let Some(fs) = pane.current_state() {

                                let mut new_fs = fs.clone();

                                new_fs.current_path = path.clone();

                                new_fs.selected_index = Some(0);

                                new_fs.search_filter.clear();

                                *new_fs.table_state.offset_mut() = 0;

                                new_fs.history = vec![path];

                                new_fs.history_index = 0;

                                pane.open_tab(new_fs);

                                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));

                            }

                        }

                    }

                }

        

                ContextMenuAction::OpenWith => {

                    let path = match target {

                        ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) => app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned()),

                        _ => None,

                    };

                    if let Some(p) = path {

                        app.mode = AppMode::OpenWith(p);

                        app.input.clear();

                        close_menu = false;

                    }

                }

        

                                                        ContextMenuAction::Edit => {

                                                    if let ContextMenuTarget::File(idx) = target {

                                                        if let Some(fs) = app.current_file_state() {

                                                            if let Some(path) = fs.files.get(*idx) {

                                                                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());

                                                                let cmd = format!("{} \"{}\"", editor, path.to_string_lossy());

                                                                let _ = event_tx.try_send(AppEvent::SpawnTerminal {

                                                                    path: path.parent().unwrap_or(Path::new(".")).to_path_buf(),

                                                                    new_tab: true, // Use working logic

                                                                    remote: fs.remote_session.clone(),

                                                                    command: Some(cmd),

                                                                });

                                                            }

                                                        }

                                                    }

                                                }

                        ContextMenuAction::Run => {

                             if let ContextMenuTarget::File(idx) = target {

                                if let Some(fs) = app.current_file_state() {

                                    if let Some(path) = fs.files.get(*idx) {

                                        let cat = crate::modules::files::get_file_category(path);

                                        match cat {

                                            FileCategory::Audio | FileCategory::Video => {

                                                let _ = event_tx.try_send(AppEvent::SpawnDetached {

                                                    cmd: "xdg-open".to_string(),

                                                    args: vec![path.to_string_lossy().to_string()],

                                                });

                                            }

                                            _ => {

                                                let cmd = format!("./\"{}\"", path.file_name().unwrap_or_default().to_string_lossy());

                                                let _ = event_tx.try_send(AppEvent::SpawnTerminal {

                                                    path: path.parent().unwrap_or(Path::new(".")).to_path_buf(),

                                                    new_tab: false,

                                                    remote: fs.remote_session.clone(),

                                                    command: Some(cmd),

                                                });

                                            }

                                        }

                                    }

                                }

                            }

                        }

                                        ContextMenuAction::RunTerminal => {

                                     if let ContextMenuTarget::File(idx) = target {

                                        if let Some(fs) = app.current_file_state() {

                                            if let Some(path) = fs.files.get(*idx) {

                                                let _ = event_tx.try_send(AppEvent::SpawnTerminal {

                                                    path: path.clone(),

                                                    new_tab: false,

                                                    remote: fs.remote_session.clone(),

                                                    command: None,

                                                });

                                            }

                                        }

                                    }

                                }

        ContextMenuAction::ExtractHere => {

             if let ContextMenuTarget::File(idx) = target {

                if let Some(fs) = app.current_file_state() {

                    if let Some(path) = fs.files.get(*idx) {

                        let parent = path.parent().unwrap_or(Path::new("."));

                        // Try atool, then unzip/tar as fallbacks

                        let _ = std::process::Command::new("atool")

                            .arg("-x")

                            .arg(path)

                            .current_dir(parent)

                            .spawn()

                            .or_else(|_| {

                                // Fallback to common tools if atool missing

                                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();

                                match ext.as_str() {

                                    "zip" => std::process::Command::new("unzip").arg(path).current_dir(parent).spawn(),

                                    "tar" | "gz" | "xz" | "bz2" => std::process::Command::new("tar").arg("-xf").arg(path).current_dir(parent).spawn(),

                                    _ => Err(std::io::Error::new(std::io::ErrorKind::NotFound, "No extraction tool found")),

                                }

                            });

                    }

                }

            }

        }

        ContextMenuAction::NewFolder => {

            let parent_dir = match target {

                ContextMenuTarget::Folder(idx) => app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned()),

                ContextMenuTarget::EmptySpace => app.current_file_state().map(|fs| fs.current_path.clone()),

                _ => None,

            };

            if let Some(_parent) = parent_dir {

                app.mode = AppMode::NewFolder;

                app.input.clear();

                close_menu = false; // Stay in NewFolder mode

            }

        }

        ContextMenuAction::NewFile => {

            let parent_dir = match target {

                ContextMenuTarget::Folder(idx) => app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned()),

                ContextMenuTarget::EmptySpace => app.current_file_state().map(|fs| fs.current_path.clone()),

                _ => None,

            };

            if let Some(_parent) = parent_dir {

                app.mode = AppMode::NewFile;

                app.input.clear();

                close_menu = false; // Stay in NewFile mode

            }

        }

        ContextMenuAction::Cut => {

            let paths = match target {

                ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) => get_targets(app, Some(*idx)),

                _ => vec![],

            };

            if let Some(p) = paths.first() {

                app.clipboard = Some((p.clone(), crate::app::ClipboardOp::Cut));

            }

        }

                ContextMenuAction::Copy => {

                     let paths = match target {

                        ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) => get_targets(app, Some(*idx)),

                        _ => vec![],

                    };

                    if let Some(p) = paths.first() {

                        app.clipboard = Some((p.clone(), crate::app::ClipboardOp::Copy));

                    }

                }

        

                ContextMenuAction::CopyPath => {

                    let paths = match target {

                        ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) => get_targets(app, Some(*idx)),

                        _ => vec![],

                    };

                    if let Some(p) = paths.first() {

                        let path_str = p.to_string_lossy();

                        let encoded = terma::visuals::osc::simple_base64_encode(path_str.as_bytes());

                        print!("\x1b]52;c;{}\x07", encoded);

                        use std::io::Write;

                        let _ = std::io::stdout().flush();

                        app.last_action_msg = Some(("Path copied to clipboard".to_string(), std::time::Instant::now()));

                    }

                }

        

                ContextMenuAction::CopyName => {

                    let paths = match target {

                        ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) => get_targets(app, Some(*idx)),

                        _ => vec![],

                    };

                    if let Some(p) = paths.first() {

                        if let Some(name) = p.file_name() {

                            let name_str = name.to_string_lossy();

                            let encoded = terma::visuals::osc::simple_base64_encode(name_str.as_bytes());

                            print!("\x1b]52;c;{}\x07", encoded);

                            use std::io::Write;

                            let _ = std::io::stdout().flush();

                            app.last_action_msg = Some(("Name copied to clipboard".to_string(), std::time::Instant::now()));

                        }

                    }

                }

        

                ContextMenuAction::Paste => {

            if let Some((src, op)) = app.clipboard.clone() {

                let dest_dir = match target {

                    ContextMenuTarget::Folder(idx) => app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned()),

                    ContextMenuTarget::EmptySpace => app.current_file_state().map(|fs| fs.current_path.clone()),

                    _ => None,

                };

                if let Some(dest_dir) = dest_dir {

                    let dest = dest_dir.join(src.file_name().unwrap());

                    match op {

                        crate::app::ClipboardOp::Copy => { let _ = event_tx.try_send(AppEvent::Copy(src, dest)); }

                        crate::app::ClipboardOp::Cut => { let _ = event_tx.try_send(AppEvent::Rename(src, dest)); app.clipboard = None; }

                    }

                }

            }

        }

        ContextMenuAction::Rename => {

            let path = match target {

                ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) => {

                    app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned())

                }

                _ => None,

            };

            if let Some(p) = path {

                app.mode = AppMode::Rename;

                app.input.set_value(p.file_name().unwrap().to_string_lossy().to_string());

                app.rename_selected = true;

                close_menu = false;

            }

        }

        ContextMenuAction::Duplicate => {

            let path = match target {

                ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) => {

                    app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned())

                }

                _ => None,

            };

            if let Some(src) = path {

                let mut dest = src.clone();

                let stem = src.file_stem().unwrap().to_string_lossy();

                let ext = src.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();

                dest.set_file_name(format!("{} (copy){}", stem, ext));

                // Ensure unique name if already exists

                let mut i = 1;

                while dest.exists() {

                    dest.set_file_name(format!("{} (copy {}){}", stem, i, ext));

                    i += 1;

                }

                let _ = event_tx.try_send(AppEvent::Copy(src, dest));

            }

        }

        ContextMenuAction::Compress => {
            let path = match target {
                ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) => app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned()),
                _ => None,
            };
            if let Some(src) = path {
                let mut dest = src.clone();
                dest.set_extension("zip");
                let tx = event_tx.clone();
                let pane_idx = app.focused_pane_index;
                tokio::spawn(async move {
                    let success = std::process::Command::new("zip")
                        .arg("-r")
                        .arg(&dest)
                        .arg(&src)
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    
                    if !success {
                        dest.set_extension("tar.gz");
                        let _ = std::process::Command::new("tar")
                            .arg("-czf")
                            .arg(&dest)
                            .arg(&src)
                            .status();
                    }
                    let _ = tx.send(AppEvent::RefreshFiles(pane_idx)).await;
                });
            }
        }

        ContextMenuAction::Delete => {

            let path = match target {

                ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) => {

                    app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned())

                }

                ContextMenuTarget::SidebarFavorite(p) => Some(p.clone()),

                _ => None,

            };

            if let Some(p) = path {

                                if let ContextMenuTarget::SidebarFavorite(_) = target {

                                    app.starred.retain(|x| x != &p);

                                } else {

                                    app.mode = AppMode::Delete;

                                    close_menu = false;

                                }

                            }

                        }

                        ContextMenuAction::Properties => {

            app.mode = AppMode::Properties;

            close_menu = false;

        }

                ContextMenuAction::TerminalWindow => {
                    if let Some(fs) = app.current_file_state() {
                        let path = match target {
                            ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) => {
                                fs.files.get(*idx).and_then(|p| p.parent()).unwrap_or(&fs.current_path).to_path_buf()
                            }
                            _ => fs.current_path.clone()
                        };
                        let _ = event_tx.try_send(AppEvent::SpawnTerminal { path, new_tab: true, remote: fs.remote_session.clone(), command: None });
                    }
                }

                            ContextMenuAction::Refresh => {

                                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));

                            }

                            ContextMenuAction::SelectAll => {

                                if let Some(fs) = app.current_file_state_mut() {

                                    fs.multi_select = (0..fs.files.len()).collect();

                                }

                            }

                            ContextMenuAction::ToggleHidden => {

                                let _ = app.toggle_hidden();

                                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));

                            }

                            ContextMenuAction::ConnectRemote => {

                                if let ContextMenuTarget::SidebarRemote(idx) = target {

                                    execute_command(crate::app::CommandAction::ConnectToRemote(*idx), app, event_tx.clone());

                                }

                            }

                            ContextMenuAction::DeleteRemote => {

                                if let ContextMenuTarget::SidebarRemote(idx) = target {

                                    app.remote_bookmarks.remove(*idx);

                                }

                            }

                            ContextMenuAction::Mount => {

                                if let ContextMenuTarget::SidebarStorage(idx) = target {

                                    if let Some(disk) = app.system_state.disks.get(*idx) {

                                        let dev = disk.device.clone();

                                        let tx = event_tx.clone();

                                        let p_idx = app.focused_pane_index;

                                        tokio::spawn(async move {

                                            if let Ok(out) = std::process::Command::new("udisksctl").arg("mount").arg("-b").arg(&dev).output() {

                                                if String::from_utf8_lossy(&out.stdout).contains("Mounted") {

                                                    tokio::time::sleep(Duration::from_millis(200)).await;

                                                    let _ = tx.send(AppEvent::RefreshFiles(p_idx)).await;

                                                }

                                            }

                                        });

                                    }

                                }

                            }

                            ContextMenuAction::Unmount => {

                                if let ContextMenuTarget::SidebarStorage(idx) = target {

                                    if let Some(disk) = app.system_state.disks.get(*idx) {

                                        let dev = disk.device.clone();

                                        tokio::spawn(async move {

                                            let _ = std::process::Command::new("udisksctl").arg("unmount").arg("-b").arg(&dev).output();

                                        });

                                    }

                                }

                            }

                            ContextMenuAction::SetWallpaper => {

                                let path = match target {

                                    ContextMenuTarget::File(idx) => app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned()),

                                    _ => None,

                                };

                                if let Some(p) = path {

                                    let _ = std::process::Command::new("swww").arg("img").arg(&p).spawn()

                                        .or_else(|_| std::process::Command::new("feh").arg("--bg-fill").arg(&p).spawn())

                                        .or_else(|_| std::process::Command::new("gsettings").arg("set").arg("org.gnome.desktop.background").arg("picture-uri").arg(format!("file://{}", p.to_string_lossy())).spawn());

                                }

                            }

                            ContextMenuAction::GitInit => {

                                let path = match target {

                                    ContextMenuTarget::Folder(idx) => app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned()),

                                    ContextMenuTarget::EmptySpace => app.current_file_state().map(|fs| fs.current_path.clone()),

                                    _ => None,

                                };

                                if let Some(p) = path {

                                    let _ = std::process::Command::new("git").arg("init").current_dir(p).spawn();

                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));

                                }

                            }

                                            ContextMenuAction::GitStatus => {

                                                let path = match target {

                                                    ContextMenuTarget::Folder(idx) => app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned()),

                                                    ContextMenuTarget::EmptySpace => app.current_file_state().map(|fs| fs.current_path.clone()),

                                                    _ => None,

                                                };

                                                if let Some(p) = path {

                                                    let remote = app.current_file_state().and_then(|fs| fs.remote_session.as_ref());

                                                    let cmd = "git status; read -p 'Press enter to close... '".to_string();

                                                    let _ = event_tx.try_send(AppEvent::SpawnTerminal {

                                                        path: p,

                                                        new_tab: false,

                                                        remote: remote.cloned(),

                                                        command: Some(cmd),

                                                    });

                                                }

                                            }

                        }

    

    if close_menu {

        app.mode = AppMode::Normal;

    }

}

fn spawn_terminal(path: &std::path::Path, new_tab: bool, remote: Option<&crate::app::RemoteSession>, preferred_terminal: Option<&str>, command_to_run: Option<&str>) {
    let mut terminals: Vec<String> = vec![
        "kgx".into(), "gnome-terminal".into(), "konsole".into(), "tilix".into(), "terminator".into(),
        "xfce4-terminal".into(), "mate-terminal".into(), "lxterminal".into(), "wezterm".into(), "foot".into(),
        "xdg-terminal-exec".into(), "x-terminal-emulator".into(), "alacritty".into(), "kitty".into(), "xterm".into()
    ];

    
    let mut resolved_terminals = Vec::new();
    if let Some(pt) = preferred_terminal {
        resolved_terminals.push(pt.to_string());
    }
    
    if preferred_terminal.is_none() {
        if let Ok(et) = std::env::var("TERMINAL") { 
            terminals.insert(0, et); 
        }
        resolved_terminals.extend(terminals);
    }

    for t in resolved_terminals {
        let exists = if Some(t.as_str()) == preferred_terminal {
            true 
        } else {
             std::process::Command::new("which").arg(&t).stdout(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
        };

        if exists {
            let path_str = path.to_string_lossy();

            if let Some(r) = remote {
                 let ssh_target = format!("{}@{}", r.user, r.host);
                 let remote_cmd = if let Some(c) = command_to_run {
                     format!("cd '{}'; {}; exec $SHELL", path_str, c)
                 } else {
                     format!("cd '{}'; exec $SHELL", path_str)
                 };
                 
                 let mut command = std::process::Command::new(&t);
                 match t.as_str() {
                     "gnome-terminal" | "xfce4-terminal" | "mate-terminal" => {
                         if new_tab { command.arg("--tab"); }
                         command.args(["--", "ssh", "-t", &ssh_target, &remote_cmd]);
                     }
                     "kgx" => {
                         if new_tab { command.arg("--tab"); }
                         command.args(["--", "ssh", "-t", &ssh_target, &remote_cmd]);
                     }
                     "konsole" => {
                         if new_tab { command.arg("--new-tab"); } else { command.arg("--new-window"); }
                         command.args(["-e", "ssh", "-t", &ssh_target, &remote_cmd]);
                     }
                     "tilix" => {
                         if new_tab { command.args(["--action", "session-add-as-terminal"]); } else { command.arg("--action").arg("session-add-as-window"); }
                         command.args(["-e", "ssh", "-t", &ssh_target, &remote_cmd]);
                     }
                     "terminator" => {
                         if new_tab { command.arg("--new-tab"); }
                         command.args(["-e", "ssh", "-t", &ssh_target, &remote_cmd]);
                     }
                     "lxterminal" => {
                         if new_tab { command.arg("--tabs"); }
                         command.args(["-e", "ssh", "-t", &ssh_target, &remote_cmd]);
                     }
                     "wezterm" => {
                         if new_tab { command.args(["start", "--new-tab"]); } else { command.arg("start"); }
                         command.args(["ssh", &ssh_target, "-t", &remote_cmd]);
                     }
                     _ => {
                         command.args(["-e", "ssh", "-t", &ssh_target, &remote_cmd]);
                     }
                 }
                 
                 unsafe {
                     let _ = command
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .pre_exec(|| { libc::setsid(); Ok(()) })
                        .spawn();
                 }
            } else {
                let mut command = std::process::Command::new(&t);
                command.current_dir(path);
                
                // Helper to build local command string
                let local_cmd = if let Some(c) = command_to_run {
                    format!("{}; exec $SHELL", c)
                } else {
                    "exec $SHELL".to_string()
                };

                match t.as_str() {
                    "gnome-terminal" | "xfce4-terminal" | "mate-terminal" => {
                        if new_tab { command.arg("--tab"); }
                        command.arg("--working-directory").arg(&*path_str);
                        if command_to_run.is_some() {
                            command.arg("--").arg("sh").arg("-c").arg(&local_cmd);
                        }
                    }
                    "kgx" => {
                        if new_tab { command.arg("--tab"); }
                        command.arg("--working-directory").arg(&*path_str);
                        if command_to_run.is_some() {
                            command.arg("--").arg("sh").arg("-c").arg(&local_cmd);
                        }
                    }
                    "tilix" => {
                        if new_tab { command.args(["--action", "session-add-as-terminal"]); } else { command.arg("--action").arg("session-add-as-window"); }
                        command.arg("--working-directory").arg(&*path_str);
                        if command_to_run.is_some() {
                            command.arg("-e").arg("sh").arg("-c").arg(&local_cmd);
                        }
                    }
                    "terminator" => {
                        if new_tab { command.arg("--new-tab"); }
                        command.arg("--working-directory").arg(&*path_str);
                        if command_to_run.is_some() {
                            command.arg("-e").arg("sh").arg("-c").arg(&local_cmd);
                        }
                    }
                    "lxterminal" => {
                        if new_tab { command.arg("--tabs"); }
                        command.arg("--working-directory").arg(&*path_str);
                        if command_to_run.is_some() {
                            command.arg("-e").arg("sh").arg("-c").arg(&local_cmd);
                        }
                    }
                    "konsole" => {
                        if new_tab { command.arg("--new-tab"); } else { command.arg("--new-window"); }
                        command.arg("--workdir").arg(&*path_str);
                        if command_to_run.is_some() {
                            command.arg("-e").arg("sh").arg("-c").arg(&local_cmd);
                        }
                    }
                    "wezterm" => {
                        if new_tab { command.args(["start", "--new-tab", "--cwd"]); } else { command.arg("start").arg("--cwd"); }
                        command.arg(&*path_str);
                        if let Some(c) = command_to_run {
                            command.args(["sh", "-c", c]);
                        }
                    }
                    "alacritty" => {
                        command.arg("--working-directory").arg(&*path_str);
                        if command_to_run.is_some() {
                            command.arg("-e").arg("sh").arg("-c").arg(&local_cmd);
                        }
                    }
                    "kitty" => {
                        command.arg("--directory").arg(&*path_str);
                        if new_tab {
                            // Kitty needs remote control for tabs, but standard launch is window
                            // We'll just launch normally for now as most people don't have socket enabled
                        }
                        if command_to_run.is_some() {
                            command.arg("sh").arg("-c").arg(&local_cmd);
                        }
                    }
                    _ => {
                        // Generic fallback (xterm, etc)
                        if command_to_run.is_some() {
                            command.arg("-e").arg("sh").arg("-c").arg(&local_cmd);
                        }
                    }
                }

                unsafe {
                    let _ = command
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .pre_exec(|| { libc::setsid(); Ok(()) })
                        .spawn();
                }
            }
            break;
        }
    }
}

