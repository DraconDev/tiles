use std::{io, time::{Duration, Instant}};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseEventKind, MouseButton},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    Terminal,
};

mod app;
mod ui;
mod modules;
mod event;
mod config;
mod license;

use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use crate::app::{App, AppMode, CurrentView, CommandItem, AppEvent};
use crate::modules::docker::DockerModule;
use bollard::models::ContainerSummary;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let (event_tx, mut event_rx) = mpsc::channel(100);
    let (docker_tx, docker_rx) = mpsc::channel(10);
    let docker_module = DockerModule::new().ok().map(Arc::new);

    // Background Docker Worker
    if let Some(docker) = docker_module.clone() {
        tokio::spawn(async move {
            loop {
                if let Ok(containers) = docker.get_containers().await {
                    let _ = docker_tx.send(containers).await;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }

    // Tick Provider
    let tick_tx = event_tx.clone();
    tokio::spawn(async move {
        loop {
            let _ = tick_tx.send(AppEvent::Tick).await;
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    });

    let res = run_app(&mut terminal, &mut app, event_rx, event_tx, docker_rx, docker_module).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    if let Err(err) = res { println!("{err:?}"); }
    Ok(())
}

fn push_history(fs: &mut crate::app::FileState, path: std::path::PathBuf) {
    if fs.current_path == path { return; }
    fs.history.truncate(fs.history_index + 1);
    fs.history.push(path);
    fs.history_index = fs.history.len() - 1;
}

fn navigate_back(fs: &mut crate::app::FileState) {
    if fs.history_index > 0 {
        fs.history_index -= 1;
        fs.current_path = fs.history[fs.history_index].clone();
        fs.selected_index = Some(0);
        fs.search_filter.clear();
    }
}

fn navigate_forward(fs: &mut crate::app::FileState) {
    if fs.history_index + 1 < fs.history.len() {
        fs.history_index += 1;
        fs.current_path = fs.history[fs.history_index].clone();
        fs.selected_index = Some(0);
        fs.search_filter.clear();
    }
}

fn update_docker_filter(app: &mut App) {
    if let Some(fs) = app.current_file_state() {
        if let Some(idx) = fs.selected_index {
            if let Some(path) = fs.files.get(idx) {
                if path.is_dir() {
                    if path.join("Dockerfile").exists() || path.join("docker-compose.yml").exists() || path.join("docker-compose.yaml").exists() {
                        app.docker_state.filter = Some(path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string());
                        return;
                    }
                }
            }
        }
    }
    app.docker_state.filter = None;
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>, 
    app: &mut App, 
    mut event_rx: mpsc::Receiver<AppEvent>,
    event_tx: mpsc::Sender<AppEvent>,
    mut docker_rx: mpsc::Receiver<Vec<ContainerSummary>>,
    docker_module: Option<Arc<DockerModule>>,
) -> io::Result<()> {
    
    // Initial fetch
    let _ = event_tx.send(AppEvent::RefreshFiles(app.tab_index)).await;

    while app.running {
        terminal.draw(|f| ui::draw(f, app))?;

        tokio::select! {
            Some(containers) = docker_rx.recv() => { app.docker_state.containers = containers; }
            Some(evt) = event_rx.recv() => {
                match evt {
                    AppEvent::Tick => { app.system_module.update(&mut app.system_state); }
                    AppEvent::RefreshFiles(idx) => {
                        // Spawning background IO task
                        if let Some(fs) = app.file_tabs.get(idx) {
                            let path = fs.current_path.clone();
                            let show_hidden = fs.show_hidden;
                            let filter = fs.search_filter.clone();
                            let session = fs.remote_session.as_ref().map(|rs| rs.session.clone());
                            let tx = event_tx.clone();
                            
                            tokio::spawn(async move {
                                let mut temp_state = crate::app::FileState {
                                    current_path: path, remote_session: None, selected_index: None,
                                    table_state: ratatui::widgets::TableState::default(), files: Vec::new(),
                                    metadata: std::collections::HashMap::new(), show_hidden, git_status: std::collections::HashMap::new(),
                                    clipboard: None, search_filter: filter, starred: std::collections::HashSet::new(),
                                    columns: Vec::new(), history: Vec::new(), history_index: 0,
                                };
                                
                                if let Some(s_mutex) = session {
                                    if let Ok(s) = s_mutex.lock() {
                                        crate::modules::files::update_files(&mut temp_state, Some(&s));
                                    }
                                } else {
                                    crate::modules::files::update_files(&mut temp_state, None);
                                }
                                
                                let _ = tx.send(AppEvent::FilesUpdated(idx, temp_state.files, temp_state.metadata, temp_state.git_status)).await;
                            });
                        }
                    }
                    AppEvent::FilesUpdated(idx, files, meta, git) => {
                        if let Some(fs) = app.file_tabs.get_mut(idx) {
                            fs.files = files;
                            fs.metadata = meta;
                            fs.git_status = git;
                        }
                    }
                }
            }
            res = tokio::task::spawn_blocking(|| crossterm::event::poll(Duration::from_millis(10))) => {
                if let Ok(Ok(true)) = res {
                    if let Ok(Event::Mouse(mouse)) = crossterm::event::read() {
                        let (cols, rows) = terminal.size().map(|s| (s.width, s.height)).unwrap_or((0, 0));
                        match mouse.kind {
                            MouseEventKind::Down(btn) => {
                                if let AppMode::ContextMenu(x, y) = app.mode {
                                    if mouse.column >= x && mouse.column < x + 15 {
                                        match mouse.row.saturating_sub(y) as usize {
                                            0 => { if let Some(name) = app.current_file_state().and_then(|fs| fs.selected_index.and_then(|idx| fs.files.get(idx)).map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())) { app.mode = AppMode::Rename; app.input = name; } }
                                            1 => { if let Some(fs) = app.current_file_state_mut() { if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx).cloned() { if !fs.starred.insert(path.clone()) { fs.starred.remove(&path); } } } } app.mode = AppMode::Normal; }
                                            2 => { app.mode = AppMode::Delete; }
                                            _ => { app.mode = AppMode::Normal; }
                                        }
                                    } else { app.mode = AppMode::Normal; }
                                    continue;
                                }
                                if btn == MouseButton::Right { app.mode = AppMode::ContextMenu(mouse.column, mouse.row); continue; }
                                
                                if btn == MouseButton::Middle {
                                    if app.current_view == CurrentView::Files {
                                        let index = fs_mouse_index(mouse.row, app);
                                        if let Some(fs) = app.current_file_state() {
                                            if let Some(path) = fs.files.get(index).cloned() {
                                                if path.is_dir() {
                                                    let new_fs = crate::app::FileState {
                                                        current_path: path.clone(), remote_session: fs.remote_session.clone(), 
                                                        selected_index: Some(0), table_state: ratatui::widgets::TableState::default(),
                                                        files: Vec::new(), metadata: std::collections::HashMap::new(), 
                                                        show_hidden: fs.show_hidden, git_status: std::collections::HashMap::new(),
                                                        clipboard: None, search_filter: String::new(), starred: fs.starred.clone(),
                                                        columns: fs.columns.clone(), history: vec![path], history_index: 0,
                                                    };
                                                    app.file_tabs.push(new_fs);
                                                    let _ = event_tx.send(AppEvent::RefreshFiles(app.file_tabs.len() - 1)).await;
                                                }
                                            }
                                        }
                                    }
                                    continue;
                                }

                                if format!("{:?}", btn).contains("Back") || format!("{:?}", btn) == "Other(4)" {
                                    if let Some(fs) = app.current_file_state_mut() { navigate_back(fs); let _ = event_tx.send(AppEvent::RefreshFiles(app.tab_index)).await; }
                                    continue;
                                }
                                if format!("{:?}", btn).contains("Forward") || format!("{:?}", btn) == "Other(5)" {
                                    if let Some(fs) = app.current_file_state_mut() { navigate_forward(fs); let _ = event_tx.send(AppEvent::RefreshFiles(app.tab_index)).await; }
                                    continue;
                                }

                                if btn == MouseButton::Left {
                                    let mut is_double_click = false;
                                    if let Some((last_time, last_row, last_col)) = app.last_click {
                                        if last_time.elapsed() < Duration::from_millis(500) && last_row == mouse.row && last_col == mouse.column { is_double_click = true; }
                                    }
                                    app.last_click = Some((Instant::now(), mouse.row, mouse.column));

                                    if mouse.row == 0 {
                                        if mouse.column < 11 { app.current_view = CurrentView::Files; }
                                        else if mouse.column < 22 { app.current_view = CurrentView::System; }
                                        else if mouse.column < 33 { app.current_view = CurrentView::Docker; }
                                    } else if mouse.row == rows.saturating_sub(1) {
                                        if mouse.column < 13 { app.mode = AppMode::CommandPalette; app.input.clear(); update_commands(app); }
                                    } else {
                                        let sidebar_width = (cols as f32 * 0.2) as u16;
                                        if mouse.column < sidebar_width {
                                            app.sidebar_focus = true;
                                            let sidebar_row = mouse.row.saturating_sub(2) as usize;
                                            match sidebar_row {
                                                1..=4 => {
                                                    app.sidebar_index = sidebar_row - 1;
                                                    if is_double_click {
                                                        let path = match app.sidebar_index { 0 => dirs::home_dir(), 1 => dirs::download_dir(), 2 => dirs::document_dir(), 3 => dirs::picture_dir(), _ => None };
                                                        if let Some(p) = path {
                                                            if let Some(fs) = app.current_file_state_mut() {
                                                                fs.current_path = p.clone(); fs.selected_index = Some(0); fs.search_filter.clear();
                                                                *fs.table_state.offset_mut() = 0;
                                                                push_history(fs, p);
                                                            }
                                                            let _ = event_tx.send(AppEvent::RefreshFiles(app.tab_index)).await;
                                                            app.sidebar_focus = false;
                                                        }
                                                    }
                                                },
                                                r if r >= 7 => {
                                                    let bookmark_idx = r - 7;
                                                    if bookmark_idx < app.remote_bookmarks.len() {
                                                        app.sidebar_index = r - 1;
                                                        if is_double_click {
                                                            execute_command(crate::app::CommandAction::ConnectToRemote(bookmark_idx), app, &docker_module, event_tx.clone()).await;
                                                        }
                                                    }
                                                },
                                                _ => {}
                                            }
                                        } else {
                                            app.sidebar_focus = false;
                                            if app.current_view == CurrentView::Files {
                                                let index = fs_mouse_index(mouse.row, app);
                                                if let Some(fs) = app.current_file_state_mut() {
                                                    if index < fs.files.len() {
                                                        fs.selected_index = Some(index); fs.table_state.select(Some(index));
                                                        if is_double_click {
                                                            if let Some(path) = fs.files.get(index).cloned() {
                                                                if path.is_dir() {
                                                                    fs.current_path = path.clone(); fs.selected_index = Some(0); fs.search_filter.clear();
                                                                    *fs.table_state.offset_mut() = 0;
                                                                    push_history(fs, path);
                                                                }
                                                            }
                                                            let _ = event_tx.send(AppEvent::RefreshFiles(app.tab_index)).await;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            MouseEventKind::ScrollUp => {
                                if app.current_view == CurrentView::Files {
                                    if let Some(fs) = app.current_file_state_mut() {
                                        fs.selected_index = None;
                                        let new_offset = fs.table_state.offset().saturating_sub(3);
                                        *fs.table_state.offset_mut() = new_offset;
                                    }
                                } else { app.move_up(); update_docker_filter(app); }
                            }
                            MouseEventKind::ScrollDown => {
                                if app.current_view == CurrentView::Files {
                                    if let Some(fs) = app.current_file_state_mut() {
                                        fs.selected_index = None;
                                        let max_files = fs.files.len();
                                        let new_offset = (fs.table_state.offset() + 3).min(max_files.saturating_sub(1));
                                        *fs.table_state.offset_mut() = new_offset;
                                    }
                                } else { app.move_down(); update_docker_filter(app); }
                            }
                            _ => {}
                        }
                    } else if let Ok(Event::Key(key)) = crossterm::event::read() {
                        handle_key_event(key, app, &docker_module, event_tx.clone()).await;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_key_event(key: KeyCode, app: &mut App, docker_module: &Option<Arc<DockerModule>>, event_tx: mpsc::Sender<AppEvent>) {
    // Note: This needs to be moved to a function that handles modifiers
}

// Rewriting handle_key_event correctly inside run_app or as helper
// For now, let's keep it in run_app to avoid context passing hell.

fn fs_mouse_index(row: u16, app: &App) -> usize {
    let mouse_row_offset = row.saturating_sub(7) as usize;
    if let Some(fs) = app.current_file_state() { fs.table_state.offset() + mouse_row_offset }
    else { 0 }
}

fn update_commands(app: &mut App) {
    let mut commands = vec![
        CommandItem { label: "Quit".to_string(), action: crate::app::CommandAction::Quit },
        CommandItem { label: "Toggle Zoom".to_string(), action: crate::app::CommandAction::ToggleZoom },
        CommandItem { label: "View: Files".to_string(), action: crate::app::CommandAction::SwitchView(CurrentView::Files) },
        CommandItem { label: "View: Docker".to_string(), action: crate::app::CommandAction::SwitchView(CurrentView::Docker) },
        CommandItem { label: "View: System".to_string(), action: crate::app::CommandAction::SwitchView(CurrentView::System) },
        CommandItem { label: "Add Remote Host".to_string(), action: crate::app::CommandAction::AddRemote },
    ];
    for bookmark_idx in 0..app.remote_bookmarks.len() {
        let bookmark = &app.remote_bookmarks[bookmark_idx];
        commands.push(CommandItem { label: format!("Connect to: {}", bookmark.name), action: crate::app::CommandAction::ConnectToRemote(bookmark_idx) });
    }
    for container in &app.docker_state.containers {
         let name = container.names.as_ref().map(|n| n.first().map(|s| s.as_str()).unwrap_or("")).unwrap_or("").trim_start_matches('/');
         if !name.is_empty() {
             commands.push(CommandItem { label: format!("Start Container: {}", name), action: crate::app::CommandAction::StartContainer(name.to_string()) });
             commands.push(CommandItem { label: format!("Stop Container: {}", name), action: crate::app::CommandAction::StopContainer(name.to_string()) });
         }
    }
    app.filtered_commands = commands.into_iter().filter(|cmd| cmd.label.to_lowercase().contains(&app.input.to_lowercase())).collect();
    app.command_index = app.command_index.min(app.filtered_commands.len().saturating_sub(1));
}

async fn execute_command(action: crate::app::CommandAction, app: &mut App, docker_module: &Option<Arc<DockerModule>>, event_tx: mpsc::Sender<AppEvent>) {
    match action {
        crate::app::CommandAction::Quit => { app.running = false; },
        crate::app::CommandAction::ToggleZoom => app.toggle_zoom(),
        crate::app::CommandAction::SwitchView(view) => app.current_view = view,
        crate::app::CommandAction::StartContainer(name) => { if let Some(docker) = docker_module { let docker = docker.clone(); tokio::spawn(async move { let _ = docker.start_container(&name).await; }); } },
        crate::app::CommandAction::StopContainer(name) => { if let Some(docker) = docker_module { let docker = docker.clone(); tokio::spawn(async move { let _ = docker.stop_container(&name).await; }); } },
        crate::app::CommandAction::AddRemote => { app.mode = AppMode::AddRemote; app.input.clear(); },
        crate::app::CommandAction::ConnectToRemote(idx) => {
            if let Some(bookmark) = app.remote_bookmarks.get(idx).cloned() {
                let host = bookmark.host.clone();
                let port = bookmark.port;
                let user = bookmark.user.clone();
                let name = bookmark.name.clone();
                let key = format!("{}:{}", host, port);
                
                if !app.active_sessions.contains_key(&key) {
                    let addr = format!("{}:{}", host, port);
                    if let Ok(tcp) = std::net::TcpStream::connect(&addr) {
                        if let Ok(mut sess) = ssh2::Session::new() {
                            sess.set_tcp_stream(tcp);
                            if sess.handshake().is_ok() {
                                if sess.userauth_agent(&user).is_ok() {
                                    app.active_sessions.insert(key.clone(), Arc::new(Mutex::new(sess)));
                                }
                            }
                        }
                    }
                }

                if let Some(session) = app.active_sessions.get(&key).cloned() {
                    if let Some(fs) = app.current_file_state_mut() {
                        fs.remote_session = Some(crate::app::RemoteSession {
                            name,
                            host,
                            user,
                            session,
                        });
                        fs.current_path = std::path::PathBuf::from("/");
                        let _ = event_tx.send(AppEvent::RefreshFiles(app.tab_index)).await;
                    }
                }
            }
        }
    }
}

fn toggle_column(file_state: &mut crate::app::FileState, col: crate::app::FileColumn) {
    if file_state.columns.contains(&col) { file_state.columns.retain(|c| *c != col); }
    else { file_state.columns.push(col); }
}
