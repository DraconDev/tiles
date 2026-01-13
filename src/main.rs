use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use std::os::unix::process::CommandExt;

// Terma Imports
use terma::integration::ratatui::TermaBackend;
use terma::input::event::{Event, KeyCode, MouseEventKind, KeyModifiers, MouseButton};
use terma::widgets::TextEditor;

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
                AppEvent::PreviewRequested(_target_pane_idx, path) => {
                    let app_clone = app.clone();
                    let tx = event_tx.clone();
                    tokio::spawn(async move {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            // Quick editor for text files
                            let editor = if content.len() < 50000 { Some(TextEditor::with_content(&content)) } else { None };
                            let preview_content = content.chars().take(5000).collect::<String>();
                            let mut app_guard = app_clone.lock().unwrap();
                            app_guard.mode = AppMode::Editor;
                            app_guard.editor_state = Some(crate::app::PreviewState { path, content: preview_content, scroll: 0, editor });
                        } else if path.is_dir() {
                             {
                                 let mut app_guard = app_clone.lock().unwrap();
                                 if let Some(fs) = app_guard.current_file_state_mut() {
                                     fs.current_path = path.clone(); fs.selected_index = Some(0); fs.multi_select.clear(); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path);
                                 }
                             }
                             let _ = tx.send(AppEvent::RefreshFiles(0)).await;
                        }
                    });
                    needs_draw = true;
                }
                AppEvent::SaveFile(path, content) => {
                    let tx = event_tx.clone();
                    tokio::spawn(async move {
                        if let Ok(_) = std::fs::write(&path, content) {
                            let _ = tx.send(AppEvent::StatusMsg(format!("Saved: {}", path.display()))).await;
                        } else {
                            let _ = tx.send(AppEvent::StatusMsg(format!("Error saving: {}", path.display()))).await;
                        }
                    });
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
                KeyCode::Char('s') | KeyCode::Char('S') if has_control => {
                    if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                        if let Some(preview) = &mut pane.preview {
                            if let Some(editor) = &mut preview.editor {
                                let _ = event_tx.try_send(AppEvent::SaveFile(preview.path.clone(), editor.get_content()));
                                editor.modified = false;
                                return true;
                            }
                        }
                    }
                    return true; 
                }
                KeyCode::Char('\\') if has_control => { app.toggle_split(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); return true; }
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
                KeyCode::Char('s') if has_control => {
                    if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                        if let Some(preview) = &mut pane.preview {
                            if let Some(editor) = &mut preview.editor {
                                let _ = event_tx.try_send(AppEvent::SaveFile(preview.path.clone(), editor.get_content()));
                                editor.modified = false;
                                return true;
                            }
                        }
                    }
                }
                _ => {}
            }

            // Route to Editor if focused pane is in preview mode
            let (w, h) = app.terminal_size;
            let sw = app.sidebar_width();
            let pane_count = app.panes.len();
            let content_w = w.saturating_sub(sw);
            let pane_w = if pane_count > 1 { content_w / 2 } else { content_w };
            let editor_area = ratatui::layout::Rect::new(0, 0, pane_w.saturating_sub(2), h.saturating_sub(4));

            if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                if let Some(preview) = &mut pane.preview {
                    if let Some(editor) = &mut preview.editor {
                        if !app.sidebar_focus && app.mode == AppMode::Normal {
                            if editor.handle_event(&evt, editor_area) {
                                return true;
                            }
                        }
                    }
                }
            }

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
                            match idx { 0 => app.pending_remote.name = val, 1 => app.pending_remote.host = val, 2 => app.pending_remote.user = val, 3 => app.pending_remote.port = val.parse().unwrap_or(22), 4 => app.pending_remote.key_path = if val.is_empty() { None } else { Some(std::path::PathBuf::from(val)) }, _ => {} }
                            if idx < 4 {
                                app.mode = AppMode::AddRemote(idx + 1);
                                let next_val = match idx + 1 { 1 => app.pending_remote.host.clone(), 2 => app.pending_remote.user.clone(), 3 => app.pending_remote.port.to_string(), 4 => app.pending_remote.key_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(), _ => String::new() };
                                app.input.set_value(next_val);
                            } else {
                                app.remote_bookmarks.push(app.pending_remote.clone()); let _ = crate::config::save_state(app);
                                app.mode = AppMode::Normal; app.input.clear();
                            }
                            return true;
                        }
                        _ => return app.input.handle_event(&evt)
                    }
                }
                AppMode::Header(idx) => {
                    let idx = *idx;
                    let total_icons = 4;
                    let total_tabs: usize = app.panes.iter().map(|p| p.tabs.len()).sum();
                    let total_items = total_icons + total_tabs;
                    match key.code {
                        KeyCode::Esc => { app.mode = AppMode::Normal; return true; }
                        KeyCode::Left => { app.mode = AppMode::Header(idx.saturating_sub(1)); return true; }
                        KeyCode::Right => { if idx < total_items.saturating_sub(1) { app.mode = AppMode::Header(idx + 1); } return true; }
                        KeyCode::Down => { app.mode = AppMode::Normal; return true; }
                        KeyCode::Enter => {
                            if idx < total_icons {
                                match idx {
                                    0 => app.mode = AppMode::Settings,
                                    1 => if let Some(fs) = app.current_file_state_mut() { navigate_back(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                                    2 => if let Some(fs) = app.current_file_state_mut() { navigate_forward(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                                    3 => { app.toggle_split(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); }
                                    _ => {} 
                                }
                                if let AppMode::Header(_) = app.mode { app.mode = AppMode::Normal; }
                            } else {
                                let mut current_global_tab = 4;
                                for (p_i, pane) in app.panes.iter_mut().enumerate() {
                                    for (t_i, _) in pane.tabs.iter().enumerate() {
                                        if current_global_tab == idx { pane.active_tab_index = t_i; app.focused_pane_index = p_i; let _ = event_tx.try_send(AppEvent::RefreshFiles(p_i)); app.mode = AppMode::Normal; return true; }
                                        current_global_tab += 1;
                                    }
                                }
                            }
                            return true;
                        }
                        _ => {} 
                    }
                    return true;
                }
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
                }
                AppMode::Settings => match key.code {
                    KeyCode::Esc => { app.mode = AppMode::Normal; return true; }
                    KeyCode::Char('1') => { app.settings_target = SettingsTarget::SingleMode; return true; }
                    KeyCode::Char('2') => { app.settings_target = SettingsTarget::SplitMode; return true; }
                    KeyCode::Left | KeyCode::BackTab => { app.settings_section = match app.settings_section { SettingsSection::Columns => SettingsSection::Shortcuts, SettingsSection::Tabs => SettingsSection::Columns, SettingsSection::General => SettingsSection::Tabs, SettingsSection::Remotes => SettingsSection::General, SettingsSection::Shortcuts => SettingsSection::Remotes }; return true; } 
                    KeyCode::Right | KeyCode::Tab => { app.settings_section = match app.settings_section { SettingsSection::Columns => SettingsSection::Tabs, SettingsSection::Tabs => SettingsSection::General, SettingsSection::General => SettingsSection::Remotes, SettingsSection::Remotes => SettingsSection::Shortcuts, SettingsSection::Shortcuts => SettingsSection::Columns }; return true; } 
                    KeyCode::Char('n') => { app.toggle_column(crate::app::FileColumn::Name); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); return true; } 
                    KeyCode::Char('e') => { app.toggle_column(crate::app::FileColumn::Extension); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); return true; } 
                    KeyCode::Char('s') => { app.toggle_column(crate::app::FileColumn::Size); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); return true; } 
                    KeyCode::Char('m') => { app.toggle_column(crate::app::FileColumn::Modified); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); return true; } 
                    KeyCode::Char('c') => { app.toggle_column(crate::app::FileColumn::Created); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); return true; } 
                    KeyCode::Char('p') => { app.toggle_column(crate::app::FileColumn::Permissions); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); return true; } 
                    KeyCode::Char('i') => { app.icon_mode = match app.icon_mode { IconMode::Nerd => IconMode::Unicode, IconMode::Unicode => IconMode::ASCII, IconMode::ASCII => IconMode::Nerd }; return true; }
                    KeyCode::Char('h') if app.settings_section == SettingsSection::General => { app.default_show_hidden = !app.default_show_hidden; return true; } 
                    KeyCode::Char('d') if app.settings_section == SettingsSection::General => { app.confirm_delete = !app.confirm_delete; return true; } 
                    KeyCode::Char('t') if app.settings_section == SettingsSection::General => { app.smart_date = !app.smart_date; return true; } 
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
                    if key.code == KeyCode::Esc {
                        app.mode = AppMode::Normal; for pane in &mut app.panes { pane.preview = None; }
                        if let Some(fs) = app.current_file_state_mut() { fs.multi_select.clear(); fs.selection_anchor = None; if !fs.search_filter.is_empty() { fs.search_filter.clear(); fs.selected_index = Some(0); *fs.table_state.offset_mut() = 0; let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } 
                        return true;
                    }
                    match key.code {
                        KeyCode::Char('c') if has_control => { if let Some(fs) = app.current_file_state() { if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx) { app.clipboard = Some((path.clone(), crate::app::ClipboardOp::Copy)); } } } return true; }
                        KeyCode::Char('x') if has_control => { if let Some(fs) = app.current_file_state() { if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx) { app.clipboard = Some((path.clone(), crate::app::ClipboardOp::Cut)); } } } return true; }
                        KeyCode::Char('v') if has_control => { if let Some((src, op)) = app.clipboard.clone() { if let Some(fs) = app.current_file_state() { let dest = fs.current_path.join(src.file_name().unwrap()); match op { crate::app::ClipboardOp::Copy => { let _ = event_tx.try_send(AppEvent::Copy(src, dest)); } crate::app::ClipboardOp::Cut => { let _ = event_tx.try_send(AppEvent::Rename(src, dest)); app.clipboard = None; } } } } return true; }
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
                        KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => { if let Some(pane) = app.panes.get_mut(app.focused_pane_index) { pane.preview = None; if let Some(fs) = pane.current_state_mut() { navigate_back(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } return true; }
                        KeyCode::Right if key.modifiers.contains(KeyModifiers::ALT) => { if let Some(pane) = app.panes.get_mut(app.focused_pane_index) { pane.preview = None; if let Some(fs) = pane.current_state_mut() { navigate_forward(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } return true; }
                        KeyCode::Down => { app.move_down(key.modifiers.contains(KeyModifiers::SHIFT)); return true; }
                        KeyCode::Up => {
                            if app.sidebar_focus && app.sidebar_index == 0 { app.mode = AppMode::Header(0); return true; } 
                            else if let Some(fs) = app.current_file_state() { if fs.selected_index == Some(0) || fs.files.is_empty() { app.mode = AppMode::Header(0); return true; } }
                            app.move_up(key.modifiers.contains(KeyModifiers::SHIFT)); return true; 
                        }
                        KeyCode::Left => { if key.modifiers.contains(KeyModifiers::SHIFT) { app.copy_to_other_pane(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); } else { app.move_left(); } return true; } 
                        KeyCode::Right => { if key.modifiers.contains(KeyModifiers::SHIFT) { app.copy_to_other_pane(); let _ = event_tx.try_send(AppEvent::RefreshFiles(0)); let _ = event_tx.try_send(AppEvent::RefreshFiles(1)); } else { app.move_right(); } return true; } 
                        KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => { app.mode = AppMode::Properties; return true; }
                        KeyCode::Enter => { if let Some(fs) = app.current_file_state_mut() { if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx).cloned() { if path.is_dir() { fs.current_path = path.clone(); fs.selected_index = Some(0); fs.multi_select.clear(); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } } } return true; } 
                        KeyCode::Char(' ') => { if let Some(fs) = app.current_file_state() { if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx).cloned() { if path.is_dir() { app.mode = AppMode::Properties; } else { let target_pane = if app.focused_pane_index == 0 { 1 } else { 0 }; let _ = event_tx.try_send(AppEvent::PreviewRequested(target_pane, path)); } } } } return true; } 
                        KeyCode::F(6) => {
                            let path_to_rename = if let Some(fs) = app.current_file_state() {
                                if let Some(idx) = fs.selected_index { fs.files.get(idx).cloned() } else { None }
                            } else { None };

                            if let Some(p) = path_to_rename {
                                app.mode = AppMode::Rename;
                                app.input.set_value(p.file_name().unwrap().to_string_lossy().to_string());
                                app.rename_selected = true;
                                return true;
                            }
                            return false;
                        }
                        KeyCode::Delete => {
                            let mut paths_to_delete = Vec::new();
                            let mut enter_delete_mode = false;
                            if let Some(fs) = app.current_file_state() {
                                if fs.selected_index.is_some() {
                                    if !app.confirm_delete {
                                        if !fs.multi_select.is_empty() {
                                            for &idx in &fs.multi_select { if let Some(p) = fs.files.get(idx) { paths_to_delete.push(p.clone()); } }
                                        } else if let Some(idx) = fs.selected_index {
                                            if let Some(p) = fs.files.get(idx) { paths_to_delete.push(p.clone()); }
                                        }
                                    } else {
                                        enter_delete_mode = true;
                                    }
                                }
                            }
                            if enter_delete_mode {
                                app.mode = AppMode::Delete;
                                return true;
                            }
                            if !paths_to_delete.is_empty() {
                                for p in paths_to_delete { let _ = event_tx.try_send(AppEvent::Delete(p)); }
                                return true;
                            }
                            return false;
                        }
                        KeyCode::Char('~') => {
                            if let Some(fs) = app.current_file_state_mut() {
                                if let Some(home) = dirs::home_dir() {
                                    fs.current_path = home.clone();
                                    fs.selected_index = Some(0);
                                    fs.multi_select.clear();
                                    *fs.table_state.offset_mut() = 0;
                                    push_history(fs, home);
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
                                let color = if color_code == 0 { None } else { Some(color_code) };
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
                AppMode::Settings | AppMode::ImportServers | AppMode::NewFile | AppMode::NewFolder | AppMode::Rename | AppMode::Delete | AppMode::Properties | AppMode::CommandPalette | AppMode::AddRemote(_) | AppMode::OpenWith(_) => {
                    match me.kind {
                        MouseEventKind::Down(_) => {
                            let (aw, ah) = match app.mode { AppMode::Settings => ((w as f32 * 0.8) as u16, (h as f32 * 0.8) as u16), AppMode::Properties => ((w as f32 * 0.5) as u16, (h as f32 * 0.5) as u16), AppMode::CommandPalette | AppMode::AddRemote(_) | AppMode::OpenWith(_) => ((w as f32 * 0.6) as u16, (h as f32 * 0.2) as u16), _ => ((w as f32 * 0.4) as u16, (h as f32 * 0.1) as u16) };
                            let (ax, ay) = ((w - aw) / 2, (h - ah) / 2);
                            if column >= ax && column < ax + aw && row >= ay && row < ay + ah {
                                if let AppMode::Settings = app.mode {
                                    let inner_x = ax + 1; let inner_y = ay + 1;
                                    if column < inner_x + 15 {
                                        let rel_y = row.saturating_sub(inner_y);
                                        match rel_y { 0 => app.settings_section = SettingsSection::Columns, 1 => app.settings_section = SettingsSection::Tabs, 2 => app.settings_section = SettingsSection::General, 3 => app.settings_section = SettingsSection::Remotes, 4 => app.settings_section = SettingsSection::Shortcuts, _ => {} }
                                        app.settings_scroll = 0; // Reset scroll when switching sections
                                    } else if app.settings_section == SettingsSection::General {
                                        let rel_y = row.saturating_sub(inner_y + 1);
                                        match rel_y { 0 => app.default_show_hidden = !app.default_show_hidden, 1 => app.confirm_delete = !app.confirm_delete, 2 => app.smart_date = !app.smart_date, 3 => app.icon_mode = match app.icon_mode { IconMode::Nerd => IconMode::Unicode, IconMode::Unicode => IconMode::ASCII, IconMode::ASCII => IconMode::Nerd }, _ => {} }
                                    } else if app.settings_section == SettingsSection::Columns {
                                        if row >= inner_y && row < inner_y + 3 {
                                            let cx = column.saturating_sub(inner_x + 15);
                                            if cx < 12 { app.settings_target = SettingsTarget::SingleMode; } else if cx < 25 { app.settings_target = SettingsTarget::SplitMode; }
                                        } else if row >= inner_y + 4 {
                                            let ry = row.saturating_sub(inner_y + 4);
                                            match ry { 0 => app.toggle_column(crate::app::FileColumn::Extension), 1 => app.toggle_column(crate::app::FileColumn::Size), 2 => app.toggle_column(crate::app::FileColumn::Modified), 3 => app.toggle_column(crate::app::FileColumn::Created), 4 => app.toggle_column(crate::app::FileColumn::Permissions), _ => {} }
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                        }
                                    }
                                }
                            } else { app.mode = AppMode::Normal; app.input.clear(); }
                        }
                        MouseEventKind::ScrollUp => {
                            if let AppMode::Settings = app.mode { app.settings_scroll = app.settings_scroll.saturating_sub(2); }
                        }
                        MouseEventKind::ScrollDown => {
                            if let AppMode::Settings = app.mode { app.settings_scroll = app.settings_scroll.saturating_add(2); }
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
                            // Delegation to Editor
                            let mut delegated = false;
                            if let Some(pane) = app.panes.get_mut(cp) {
                                if let Some(preview) = &mut pane.preview {
                                    if let Some(editor) = &mut preview.editor {
                                        // Calculate pane area (rough estimate matching UI logic)
                                        let pane_x = sw + (cp as u16 * pw);
                                        let editor_area = ratatui::layout::Rect::new(pane_x + 1, 1, pw.saturating_sub(2), h.saturating_sub(2));
                                        if editor_area.contains(ratatui::layout::Position { x: column, y: row }) {
                                            if editor.handle_mouse_event(me, editor_area) {
                                                delegated = true;
                                            }
                                        }
                                    }
                                }
                            }
                            if delegated { app.focused_pane_index = cp; return true; }

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
                                else { spawn_detached("xdg-open", vec![&path.to_string_lossy()]); } 
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
                                SidebarTarget::Favorite(p) => { let p = p.clone(); if let Some(fs) = app.current_file_state_mut() { fs.current_path = p.clone(); fs.remote_session = None; fs.selected_index = Some(0); fs.multi_select.clear(); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, p); } let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
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
        _ => {} 
    }
    false
}