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
    let (event_tx, event_rx) = mpsc::channel(100);
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
    
    let _ = event_tx.send(AppEvent::RefreshFiles(app.tab_index)).await;

    while app.running {
        terminal.draw(|f| ui::draw(f, app))?;

        tokio::select! {
            Some(containers) = docker_rx.recv() => { app.docker_state.containers = containers; }
            Some(evt) = event_rx.recv() => {
                match evt {
                    AppEvent::Tick => { app.system_module.update(&mut app.system_state); }
                    AppEvent::RefreshFiles(idx) => {
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
                                    if let Ok(s) = s_mutex.lock() { crate::modules::files::update_files(&mut temp_state, Some(&s)); }
                                } else { crate::modules::files::update_files(&mut temp_state, None); }
                                let _ = tx.send(AppEvent::FilesUpdated(idx, temp_state.files, temp_state.metadata, temp_state.git_status)).await;
                            });
                        }
                    }
                    AppEvent::FilesUpdated(idx, files, meta, git) => {
                        if let Some(fs) = app.file_tabs.get_mut(idx) {
                            fs.files = files; fs.metadata = meta; fs.git_status = git;
                        }
                    }
                }
            }
            res = tokio::task::spawn_blocking(|| crossterm::event::poll(Duration::from_millis(10))) => {
                if let Ok(Ok(true)) = res {
                    if let Ok(evt) = crossterm::event::read() {
                        handle_event(evt, app, &docker_module, event_tx.clone()).await;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_event(evt: Event, app: &mut App, docker_module: &Option<Arc<DockerModule>>, event_tx: mpsc::Sender<AppEvent>) {
    let (_cols, _rows) = (80, 24); // Placeholder or passed from draw
    match evt {
        Event::Mouse(mouse) => {
            match mouse.kind {
                MouseEventKind::Down(btn) => {
                    if let AppMode::ContextMenu(_x, _y) = app.mode {
                        app.mode = AppMode::Normal; // Simple dismiss for now
                        return;
                    }
                    if btn == MouseButton::Right { app.mode = AppMode::ContextMenu(mouse.column, mouse.row); return; }
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
                        return;
                    }
                    if btn == MouseButton::Left {
                        if mouse.row == 0 {
                            if mouse.column < 11 { app.current_view = CurrentView::Files; }
                            else if mouse.column < 22 { app.current_view = CurrentView::System; }
                            else if mouse.column < 33 { app.current_view = CurrentView::Docker; }
                        } else {
                            // Focus logic handled in move_left/right or click detection
                        }
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
                        let new_offset = fs.table_state.offset().saturating_add(3);
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
                        KeyCode::Enter => {
                            if let Some(cmd) = app.filtered_commands.get(app.command_index).cloned() {
                                execute_command(cmd.action, app, docker_module, event_tx.clone()).await;
                            }
                            app.mode = AppMode::Normal; app.input.clear();
                        }
                        _ => {}
                    }
                }
                _ => {
                    match key.code {
                        KeyCode::Char('q') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => app.running = false,
                        KeyCode::Char('.') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => { app.mode = AppMode::CommandPalette; update_commands(app); }
                        KeyCode::Down => { app.move_down(); }
                        KeyCode::Up => { app.move_up(); }
                        KeyCode::Left => { app.move_left(); }
                        KeyCode::Right => { app.move_right(); }
                        KeyCode::Enter => {
                            if let Some(fs) = app.current_file_state_mut() {
                                if let Some(idx) = fs.selected_index {
                                    if let Some(path) = fs.files.get(idx).cloned() {
                                        if path.is_dir() {
                                            fs.current_path = path.clone(); fs.selected_index = Some(0);
                                            push_history(fs, path);
                                            let _ = event_tx.send(AppEvent::RefreshFiles(app.tab_index)).await;
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Backspace => {
                            if let Some(fs) = app.current_file_state_mut() {
                                if let Some(p) = fs.current_path.parent() {
                                    let path = p.to_path_buf(); fs.current_path = path.clone(); fs.selected_index = Some(0);
                                    push_history(fs, path);
                                    let _ = event_tx.send(AppEvent::RefreshFiles(app.tab_index)).await;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
}

fn fs_mouse_index(row: u16, app: &App) -> usize {
    let mouse_row_offset = row.saturating_sub(7) as usize;
    if let Some(fs) = app.current_file_state() { fs.table_state.offset() + mouse_row_offset }
    else { 0 }
}

fn update_commands(app: &mut App) {
    let mut commands = vec![
        CommandItem { label: "Quit".to_string(), action: crate::app::CommandAction::Quit },
        CommandItem { label: "View: Files".to_string(), action: crate::app::CommandAction::SwitchView(CurrentView::Files) },
        CommandItem { label: "Add Remote Host".to_string(), action: crate::app::CommandAction::AddRemote },
    ];
    app.filtered_commands = commands.into_iter().filter(|cmd| cmd.label.to_lowercase().contains(&app.input.to_lowercase())).collect();
    app.command_index = app.command_index.min(app.filtered_commands.len().saturating_sub(1));
}

async fn execute_command(action: crate::app::CommandAction, app: &mut App, docker_module: &Option<Arc<DockerModule>>, event_tx: mpsc::Sender<AppEvent>) {
    match action {
        crate::app::CommandAction::Quit => { app.running = false; },
        crate::app::CommandAction::SwitchView(view) => app.current_view = view,
        crate::app::CommandAction::AddRemote => { app.mode = AppMode::AddRemote; app.input.clear(); },
        crate::app::CommandAction::ConnectToRemote(idx) => {
            if let Some(bookmark) = app.remote_bookmarks.get(idx).cloned() {
                let addr = format!("{}:{}", bookmark.host, bookmark.port);
                if let Ok(tcp) = std::net::TcpStream::connect(&addr) {
                    if let Ok(mut sess) = ssh2::Session::new() {
                        sess.set_tcp_stream(tcp);
                        if sess.handshake().is_ok() && sess.userauth_agent(&bookmark.user).is_ok() {
                            let session = Arc::new(Mutex::new(sess));
                            app.active_sessions.insert(addr.clone(), session.clone());
                            if let Some(fs) = app.current_file_state_mut() {
                                fs.remote_session = Some(crate::app::RemoteSession { name: bookmark.name, host: bookmark.host, user: bookmark.user, session });
                                fs.current_path = std::path::PathBuf::from("/");
                                let _ = event_tx.send(AppEvent::RefreshFiles(app.tab_index)).await;
                            }
                        }
                    }
                }
            }
        },
        _ => {}
    }
}

fn toggle_column(file_state: &mut crate::app::FileState, col: crate::app::FileColumn) {
    if file_state.columns.contains(&col) { file_state.columns.retain(|c| *c != col); }
    else { file_state.columns.push(col); }
}