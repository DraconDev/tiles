use notify::RecursiveMode;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use uuid::Uuid;

use terma::input::event::Event;
use terma::integration::ratatui::TermaBackend;

// Ratatui Imports
use ratatui::Terminal;

use crate::app::{
    App, AppEvent, AppMode, CommitInfo, CurrentView, FileCategory, FileMetadata, GitStatus,
    MonitorSubview, PreviewState, RemoteSession, UndoAction,
};
mod app;
mod config;
mod event;
mod event_helpers;
mod events;
mod icons;
mod license;
mod modules;
mod state;
mod ui;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    std::panic::set_hook(Box::new(|panic_info| {
        let msg = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };
        let location = panic_info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_string());
        crate::app::log_debug(&format!("PANIC at {}: {}", location, msg));
    }));

    run_tty().await
}

async fn run_tty() -> color_eyre::Result<()> {
    crate::app::log_debug("run_tty start");
    let backend = TermaBackend::new(std::io::stdout())?;
    let tile_queue = backend.tile_queue();
    let mut terminal = Terminal::new(backend)?;

    let (app, event_tx, mut event_rx) = setup_app(tile_queue);

    // Watcher Setup
    let tx_clone = event_tx.clone();
    let _debouncer = notify_debouncer_mini::new_debouncer(
        Duration::from_millis(500),
        move |res: notify_debouncer_mini::DebounceEventResult| {
            if let Ok(events) = res {
                for event in events {
                    let _ = tx_clone.blocking_send(AppEvent::FilesChangedOnDisk(event.path));
                }
            }
        },
    )?;
    let _watched_paths: std::collections::HashMap<usize, PathBuf> =
        std::collections::HashMap::new();

    // 1. Input Loop (Thread)
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
                let polled = unsafe {
                    terma::backend::tty::poll_input(std::os::fd::BorrowedFd::borrow_raw(fd), 20)
                };
                match polled {
                    Ok(true) => match stdin.read(&mut buffer) {
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
                    },
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

    // 2. System Stats Loop (Tokio)
    {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            let mut sys_mod = crate::modules::system::SystemModule::new();
            loop {
                let data = sys_mod.get_data();
                let _ = tx.send(AppEvent::SystemUpdated(data)).await;
                tokio::time::sleep(Duration::from_millis(1000)).await;
            }
        });
    }

    // 3. Tick Loop (Tokio)
    {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            loop {
                let _ = tx.send(AppEvent::Tick).await;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
    }

    // Initial State Setup
    {
        let mut app_guard = app.lock().unwrap();
        app_guard.running = true;
        if let Ok(size) = terminal.size() {
            app_guard.terminal_size = (size.width, size.height);
        }
        for i in 0..app_guard.panes.len() {
            let _ = event_tx.send(AppEvent::RefreshFiles(i)).await;
        }
    }

    crate::app::log_debug("Entering main loop");

    let mut panes_needing_refresh = std::collections::HashSet::new();

    loop {
        let mut needs_draw = false;

        while let Ok(event) = event_rx.try_recv() {
            match event {
                AppEvent::Tick => {
                    needs_draw = true;
                }
                AppEvent::Raw(raw) => {
                    let mut app_guard = app.lock().unwrap();
                    if handle_event(
                        raw,
                        &mut app_guard,
                        event_tx.clone(),
                        &mut panes_needing_refresh,
                    ) {
                        needs_draw = true;
                    }
                }
                AppEvent::SystemUpdated(data) => {
                    let mut app_guard = app.lock().unwrap();
                    crate::modules::system::SystemModule::update_app_state(&mut app_guard, data);
                    needs_draw = true;
                }
                AppEvent::ConnectToRemote(pane_idx, bookmark_idx) => {
                    let remote_opt = {
                        let app_guard = app.lock().unwrap();
                        app_guard.remote_bookmarks.get(bookmark_idx).cloned()
                    };
                    if let Some(remote) = remote_opt {
                        let tx = event_tx.clone();
                        let p_idx = pane_idx;
                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                            "Connecting to {} ({})...",
                            remote.name, remote.host
                        )));

                        tokio::spawn(async move {
                            match std::net::TcpStream::connect(format!(
                                "{}:{}",
                                remote.host, remote.port
                            )) {
                                Ok(tcp) => {
                                    let mut sess = ssh2::Session::new().unwrap();
                                    sess.set_tcp_stream(tcp);
                                    sess.set_blocking(true);
                                    if let Err(e) = sess.handshake() {
                                        let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                            "Handshake failed: {}",
                                            e
                                        )));
                                        return;
                                    }
                                    let mut auth_ok = false;
                                    if let Ok(mut agent) = sess.agent() {
                                        if agent.connect().is_ok() {
                                            if let Ok(_identities) = agent.list_identities() {
                                                for identity in agent.identities().unwrap() {
                                                    if agent
                                                        .userauth(&remote.user, &identity)
                                                        .is_ok()
                                                    {
                                                        auth_ok = true;
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if !auth_ok {
                                        if let Some(key_path) = &remote.key_path {
                                            if sess
                                                .userauth_pubkey_file(
                                                    &remote.user,
                                                    None,
                                                    key_path,
                                                    None,
                                                )
                                                .is_ok()
                                            {
                                                auth_ok = true;
                                            }
                                        }
                                    }
                                    if auth_ok {
                                        let session = RemoteSession {
                                            host: remote.host.clone(),
                                            user: remote.user.clone(),
                                            name: remote.name.clone(),
                                            session: Some(Arc::new(Mutex::new(sess))),
                                        };
                                        let _ = tx
                                            .send(AppEvent::RemoteConnected(p_idx, session))
                                            .await;
                                    } else {
                                        let _ = tx.try_send(AppEvent::StatusMsg(
                                            "Authentication failed".to_string(),
                                        ));
                                    }
                                }
                                Err(e) => {
                                    let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                        "Connection failed: {}",
                                        e
                                    )));
                                }
                            }
                        });
                    }
                }
                AppEvent::RemoteConnected(pane_idx, session) => {
                    let mut app_guard = app.lock().unwrap();
                    if let Some(pane) = app_guard.panes.get_mut(pane_idx) {
                        if let Some(fs) = pane.current_state_mut() {
                            fs.remote_session = Some(session);
                            fs.current_path = PathBuf::from("/");
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(pane_idx));
                        }
                    }
                    needs_draw = true;
                }
                AppEvent::RefreshFiles(pane_idx) => {
                    panes_needing_refresh.insert(pane_idx);
                }
                AppEvent::FilesChangedOnDisk(path) => {
                    let mut app_guard = app.lock().unwrap();
                    for (i, pane) in app_guard.panes.iter().enumerate() {
                        if let Some(fs) = pane.current_state() {
                            if path.starts_with(&fs.current_path) {
                                panes_needing_refresh.insert(i);
                            }
                        }
                    }
                }
                AppEvent::PreviewRequested(pane_idx, path) => {
                    let tx = event_tx.clone();
                    let app_clone = app.clone();
                    tokio::spawn(async move {
                        let (is_binary, is_too_large, size_mb) =
                            terma::utils::check_file_suitability(&path, 5 * 1024 * 1024);
                        let content = if is_binary {
                            format!("<Binary file: {} MB>", size_mb)
                        } else if is_too_large {
                            format!("<File too large: {} MB>", size_mb)
                        } else {
                            std::fs::read_to_string(&path)
                                .unwrap_or_else(|e| format!("Error reading file: {}", e))
                        };

                        let mut editor = terma::widgets::TextEditor::with_content(&content);
                        editor.read_only = true;

                        {
                            let mut app_guard = app_clone.lock().unwrap();
                            let preview = PreviewState {
                                path: path.clone(),
                                content,
                                scroll: 0,
                                editor: Some(editor),
                                last_saved: None,
                                image_data: None,
                                highlighted_lines: None,
                            };

                            if let Some(pane) = app_guard.panes.get_mut(pane_idx) {
                                pane.preview = Some(preview.clone());
                            }
                            if app_guard.current_view == CurrentView::Editor {
                                app_guard.editor_state = Some(preview);
                                app_guard.sidebar_focus = false;
                            }
                        }
                        let _ = tx.send(AppEvent::Tick).await;
                    });
                }
                AppEvent::SaveFile(path, content) => {
                    let _ = std::fs::write(&path, content);
                    let mut app_guard = app.lock().unwrap();
                    if let Some(ref mut preview) = app_guard.editor_state {
                        if preview.path == path {
                            preview.last_saved = Some(std::time::Instant::now());
                        }
                    }
                    for pane in &mut app_guard.panes {
                        if let Some(ref mut preview) = pane.preview {
                            if preview.path == path {
                                preview.last_saved = Some(std::time::Instant::now());
                            }
                        }
                    }
                    needs_draw = true;
                }
                AppEvent::CreateFile(path) => {
                    let _ = std::fs::File::create(&path);
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(
                        app.lock().unwrap().focused_pane_index,
                    ));
                }
                AppEvent::CreateFolder(path) => {
                    let _ = std::fs::create_dir_all(&path);
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(
                        app.lock().unwrap().focused_pane_index,
                    ));
                }
                AppEvent::Rename(old, new) => {
                    let _ = std::fs::rename(&old, &new);
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(
                        app.lock().unwrap().focused_pane_index,
                    ));
                }
                AppEvent::Delete(path) => {
                    if path.is_dir() {
                        let _ = std::fs::remove_dir_all(&path);
                    } else {
                        let _ = std::fs::remove_file(&path);
                    }
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(
                        app.lock().unwrap().focused_pane_index,
                    ));
                }
                AppEvent::Copy(src, dest) => {
                    let tx = event_tx.clone();
                    tokio::spawn(async move {
                        let _ = terma::utils::copy_recursive(&src, &dest);
                        let _ = tx.send(AppEvent::RefreshFiles(0)).await;
                    });
                }
                AppEvent::SpawnTerminal {
                    path,
                    new_tab,
                    remote: _,
                    command,
                } => {
                    let cmd_str = command.as_deref();
                    terma::utils::spawn_terminal_at(&path, new_tab, cmd_str);
                }
                AppEvent::SpawnDetached { cmd, args } => {
                    terma::utils::spawn_detached(&cmd, args);
                }
                AppEvent::KillProcess(pid) => {
                    let _ = std::process::Command::new("kill")
                        .arg("-9")
                        .arg(pid.to_string())
                        .spawn();
                }
                AppEvent::GitHistoryUpdated(
                    p_idx,
                    _t_idx,
                    history,
                    pending,
                    branch,
                    ahead,
                    behind,
                ) => {
                    let mut app_guard = app.lock().unwrap();
                    if let Some(pane) = app_guard.panes.get_mut(p_idx) {
                        if let Some(fs) = pane.current_state_mut() {
                            fs.git_history = history;
                            fs.git_pending = pending;
                            fs.git_branch = branch;
                            fs.git_ahead = ahead;
                            fs.git_behind = behind;
                        }
                    }
                    needs_draw = true;
                }
                AppEvent::TaskProgress(id, progress, status) => {
                    let mut app_guard = app.lock().unwrap();
                    if let Some(task) = app_guard.background_tasks.iter_mut().find(|t| t.id == id) {
                        task.progress = progress;
                        task.status = status;
                    } else {
                        app_guard.background_tasks.push(crate::app::BackgroundTask {
                            id,
                            name: "Task".to_string(),
                            status,
                            progress,
                        });
                    }
                    needs_draw = true;
                }
                AppEvent::TaskFinished(id) => {
                    let mut app_guard = app.lock().unwrap();
                    app_guard.background_tasks.retain(|t| t.id != id);
                    needs_draw = true;
                }
                AppEvent::GlobalSearchUpdated(pane_idx, files, _meta) => {
                    let mut app_guard = app.lock().unwrap();
                    if let Some(pane) = app_guard.panes.get_mut(pane_idx) {
                        if let Some(fs) = pane.current_state_mut() {
                            fs.files = files;
                        }
                    }
                    needs_draw = true;
                }
                AppEvent::SystemMonitor => {
                    let mut app_guard = app.lock().unwrap();
                    app_guard.save_current_view_prefs();
                    app_guard.current_view = CurrentView::Processes;
                    needs_draw = true;
                }
                AppEvent::GitHistory => {
                    let mut app_guard = app.lock().unwrap();
                    app_guard.save_current_view_prefs();
                    app_guard.current_view = CurrentView::Git;
                    needs_draw = true;
                }
                AppEvent::Editor => {
                    let mut app_guard = app.lock().unwrap();
                    app_guard.save_current_view_prefs();
                    app_guard.current_view = CurrentView::Editor;
                    app_guard.load_view_prefs(CurrentView::Editor);
                    needs_draw = true;
                }
                AppEvent::StatusMsg(msg) => {
                    let mut app_guard = app.lock().unwrap();
                    app_guard.last_action_msg = Some((msg, std::time::Instant::now()));
                    needs_draw = true;
                }
                _ => {}
            }
        }

        // Handle Refreshes
        for pane_idx in panes_needing_refresh.drain() {
            let (path, remote) = {
                let app_guard = app.lock().unwrap();
                if let Some(pane) = app_guard.panes.get(pane_idx) {
                    if let Some(fs) = pane.current_state() {
                        (fs.current_path.clone(), fs.remote_session.clone())
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            };

            let tx = event_tx.clone();
            let app_clone = app.clone();
            tokio::spawn(async move {
                let (files, metadata) = if let Some(_session) = &remote {
                    // Remote refresh logic (mocked for now)
                    (Vec::new(), std::collections::HashMap::new())
                } else {
                    crate::modules::files::read_dir_with_metadata(&path)
                };

                let git_data = if remote.is_none() {
                    crate::modules::files::fetch_git_data(&path)
                } else {
                    None
                };

                {
                    let mut app_guard = app_clone.lock().unwrap();
                    if let Some(pane) = app_guard.panes.get_mut(pane_idx) {
                        if let Some(fs) = pane.current_state_mut() {
                            // Filter hidden files if needed
                            let filtered_files: Vec<_> = files
                                .into_iter()
                                .filter(|p| {
                                    fs.show_hidden
                                        || !p
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .map(|s| s.starts_with('.'))
                                            .unwrap_or(false)
                                })
                                .collect();

                            // Sort: Folders First, then by Column
                            let mut filtered_files = filtered_files;
                            filtered_files.sort_by(|a, b| {
                                let meta_a = metadata.get(a);
                                let meta_b = metadata.get(b);
                                let is_dir_a = meta_a.map(|m| m.is_dir).unwrap_or(false);
                                let is_dir_b = meta_b.map(|m| m.is_dir).unwrap_or(false);

                                // 1. Folders First (Always on top)
                                if is_dir_a != is_dir_b {
                                    return if is_dir_a {
                                        std::cmp::Ordering::Less
                                    } else {
                                        std::cmp::Ordering::Greater
                                    };
                                }

                                // 2. Column Sort
                                let ord = match fs.sort_column {
                                    crate::app::FileColumn::Name => {
                                        let na = a
                                            .file_name()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("")
                                            .to_lowercase();
                                        let nb = b
                                            .file_name()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("")
                                            .to_lowercase();
                                        na.cmp(&nb)
                                    }
                                    crate::app::FileColumn::Size => {
                                        let sa = meta_a.map(|m| m.size).unwrap_or(0);
                                        let sb = meta_b.map(|m| m.size).unwrap_or(0);
                                        sa.cmp(&sb)
                                    }
                                    crate::app::FileColumn::Modified => {
                                        let da = meta_a
                                            .map(|m| m.modified)
                                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                                        let db = meta_b
                                            .map(|m| m.modified)
                                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                                        da.cmp(&db)
                                    }
                                    crate::app::FileColumn::Created => {
                                        let da = meta_a
                                            .map(|m| m.created)
                                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                                        let db = meta_b
                                            .map(|m| m.created)
                                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                                        da.cmp(&db)
                                    }
                                    crate::app::FileColumn::Permissions => {
                                        let pa = meta_a.map(|m| m.permissions).unwrap_or(0);
                                        let pb = meta_b.map(|m| m.permissions).unwrap_or(0);
                                        pa.cmp(&pb)
                                    }
                                    _ => {
                                        let na = a
                                            .file_name()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("")
                                            .to_lowercase();
                                        let nb = b
                                            .file_name()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("")
                                            .to_lowercase();
                                        na.cmp(&nb)
                                    }
                                };

                                if fs.sort_ascending {
                                    ord
                                } else {
                                    ord.reverse()
                                }
                            });

                            fs.files = filtered_files;
                            fs.local_count = fs.files.len();
                            fs.metadata = metadata;
                            // Sort and filter here if needed
                        }
                    }
                }
                let _ = tx.send(AppEvent::Tick).await;
                if let Some((history, pending, branch, ahead, behind)) = git_data {
                    let _ = tx
                        .send(AppEvent::GitHistoryUpdated(
                            pane_idx,
                            0,
                            history,
                            pending,
                            Some(branch),
                            ahead,
                            behind,
                        ))
                        .await;
                }
            });
        }

        if needs_draw {
            let mut app_guard = app.lock().unwrap();
            if !app_guard.running {
                break;
            }
            terminal.draw(|f| ui::draw(f, &mut app_guard))?;
        }

        tokio::time::sleep(Duration::from_millis(16)).await;
    }

    Ok(())
}

fn setup_app(
    tile_queue: Arc<Mutex<Vec<terma::compositor::engine::TilePlacement>>>,
) -> (
    Arc<Mutex<App>>,
    mpsc::Sender<AppEvent>,
    mpsc::Receiver<AppEvent>,
) {
    let (tx, rx) = mpsc::channel(1000);
    let mut app = App::new(tile_queue);

    if let Some(state) = crate::config::load_state() {
        app.panes = state.panes;
        app.focused_pane_index = state.focused_pane_index;
        app.starred = state.starred;
        app.remote_bookmarks = state.remote_bookmarks;
        app.current_view = state.current_view;
        app.path_colors = state.path_colors;
        app.external_tools = state.external_tools;
        if let Some(mode) = state.icon_mode {
            app.icon_mode = mode;
        }
        app.is_split_mode = state.is_split_mode;
        app.semantic_coloring = state.semantic_coloring;
        app.show_sidebar = state.show_sidebar;
        app.show_side_panel = state.show_side_panel;
        app.default_show_hidden = state.default_show_hidden;
    }

    let app_arc = Arc::new(Mutex::new(app));
    (app_arc, tx, rx)
}

fn handle_event(
    evt: Event,
    app: &mut App,
    event_tx: mpsc::Sender<AppEvent>,
    panes_needing_refresh: &mut std::collections::HashSet<usize>,
) -> bool {
    events::handle_event(evt, app, event_tx, panes_needing_refresh)
}
