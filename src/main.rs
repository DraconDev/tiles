use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

use terma::input::event::Event;
use terma::integration::ratatui::TermaBackend;

// Ratatui Imports
use ratatui::Terminal;

use crate::app::{App, AppEvent, CurrentView, PreviewState, RemoteSession};
mod app;
mod config;
mod event;
mod event_helpers;
mod events;
mod icons;
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
                            for byte in buffer.iter().take(n) {
                                if let Some(evt) = parser.advance(*byte) {
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
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        });
    }

    // Initial State Setup
    let pane_count = {
        let mut app_guard = app.lock().unwrap();
        app_guard.running = true;
        if let Ok(size) = terminal.size() {
            app_guard.terminal_size = (size.width, size.height);
        }
        app_guard.panes.len()
    };
    for i in 0..pane_count {
        let _ = event_tx.send(AppEvent::RefreshFiles(i)).await;
    }

    crate::app::log_debug("Entering main loop");

    let mut panes_needing_refresh = std::collections::HashSet::new();
    let mut last_self_save: std::collections::HashMap<PathBuf, String> = std::collections::HashMap::new();

    loop {
        let mut needs_draw = false;

        while let Ok(event) = event_rx.try_recv() {
            match event {
                AppEvent::Tick => {
                    needs_draw = true;
                }
                AppEvent::Raw(raw) => {
                    let (view_before, mode_before) = {
                        let app_guard = app.lock().unwrap();
                        (app_guard.current_view.clone(), app_guard.mode.clone())
                    };

                    let mut app_guard = app.lock().unwrap();
                    if handle_event(
                        raw,
                        &mut app_guard,
                        event_tx.clone(),
                        &mut panes_needing_refresh,
                    ) {
                        needs_draw = true;
                    }

                    if app_guard.current_view != view_before || app_guard.mode != mode_before {
                        let _ = terminal.clear();
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
                    // Check if this was a self-save
                    if let Some(saved_content) = last_self_save.get(&path) {
                        if let Ok(current_content) = std::fs::read_to_string(&path) {
                            if &current_content == saved_content {
                                last_self_save.remove(&path);
                                continue; // Skip refreshing/reloading for our own saves
                            }
                        }
                        last_self_save.remove(&path);
                    }

                    let app_guard = app.lock().unwrap();
                    let mut needs_reload = Vec::new();

                    for (i, pane) in app_guard.panes.iter().enumerate() {
                        if let Some(fs) = pane.current_state() {
                            if path.starts_with(&fs.current_path) {
                                panes_needing_refresh.insert(i);
                            }
                        }
                        if let Some(preview) = &pane.preview {
                            if preview.path == path {
                                if let Some(editor) = &preview.editor {
                                    if !editor.modified {
                                        needs_reload.push((i, path.clone()));
                                    }
                                }
                            }
                        }
                    }

                    if let Some(preview) = &app_guard.editor_state {
                        if preview.path == path {
                            if let Some(editor) = &preview.editor {
                                if !editor.modified {
                                    needs_reload.push((app_guard.focused_pane_index, path.clone()));
                                }
                            }
                        }
                    }

                    drop(app_guard);
                    for (p_idx, p_path) in needs_reload {
                        let _ = event_tx.try_send(AppEvent::PreviewRequested(p_idx, p_path));
                    }
                    needs_draw = true;
                }
                AppEvent::PreviewRequested(pane_idx, path) => {
                    let tx = event_tx.clone();
                    let app_clone = app.clone();
                    let (current_dir, preview_limit_mb) = {
                        let app_guard = app.lock().unwrap();
                        (
                            app_guard
                                .current_file_state()
                                .map(|fs| fs.current_path.clone())
                                .unwrap_or_else(|| PathBuf::from(".")),
                            app_guard.preview_max_mb.max(1),
                        )
                    };

                    tokio::spawn(async move {
                        let path_str = path.to_string_lossy();
                        let content = if let Some(hash) = path_str.strip_prefix("git://") {
                            let output = std::process::Command::new("git")
                                .args(["show", "--patch", "--stat", "--color=never", hash])
                                .current_dir(&current_dir)
                                .output();
                            match output {
                                Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
                                Err(e) => format!("Error fetching commit data: {}", e),
                            }
                        } else if let Some(file_path) = path_str.strip_prefix("git-diff://") {
                            let output = std::process::Command::new("git")
                                .args(["diff", file_path])
                                .current_dir(&current_dir)
                                .output();
                            match output {
                                Ok(out) => {
                                    let content = String::from_utf8_lossy(&out.stdout).to_string();
                                    if content.is_empty() {
                                        "(No changes or file only in index)".to_string()
                                    } else {
                                        content
                                    }
                                }
                                Err(e) => format!("Error fetching diff data: {}", e),
                            }
                        } else if path.is_dir() {
                            format!("\n\n   << PROJECT VIEW: {} >>\n\n   Select a file from the sidebar to begin editing.", 
                                path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".to_string()))
                        } else {
                            let (is_binary, is_too_large, size_mb) =
                                terma::utils::check_file_suitability(
                                    &path,
                                    preview_limit_mb as u64 * 1024 * 1024,
                                );
                            if is_binary {
                                format!("<Binary file: {} MB>", size_mb)
                            } else if is_too_large {
                                format!("<File too large: {} MB>", size_mb)
                            } else {
                                std::fs::read_to_string(&path)
                                    .unwrap_or_else(|e| format!("Error reading file: {}", e))
                            }
                        };

                        let mut editor = terma::widgets::TextEditor::with_content(&content);
                        if path_str.starts_with("git://") || path_str.starts_with("git-diff://") {
                            editor.language = "diff".to_string();
                            editor.read_only = true;
                        } else if path.is_dir() {
                            editor.read_only = true;
                        } else {
                            editor.language = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_string();
                        }

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
                            if app_guard.current_view == CurrentView::Editor
                                || app_guard.current_view == CurrentView::Commit
                            {
                                app_guard.editor_state = Some(preview);
                                app_guard.sidebar_focus = false;
                            }
                        }
                        let _ = tx.send(AppEvent::Tick).await;
                    });
                }
                AppEvent::SaveFile(path, content) => {
                    match std::fs::write(&path, &content) {
                        Ok(_) => {
                            last_self_save.insert(path.clone(), content);
                            let mut app_guard = app.lock().unwrap();
                            if let Some(ref mut preview) = app_guard.editor_state {
                                if preview.path == path {
                                    preview.last_saved = Some(std::time::Instant::now());
                                    if let Some(ref mut editor) = preview.editor {
                                        editor.modified = false;
                                    }
                                }
                            }
                            for pane in &mut app_guard.panes {
                                if let Some(ref mut preview) = pane.preview {
                                    if preview.path == path {
                                        preview.last_saved = Some(std::time::Instant::now());
                                        if let Some(ref mut editor) = preview.editor {
                                            editor.modified = false;
                                        }
                                    }
                                }
                            }

                            // Trigger refresh for panes showing this file's parent
                            if let Some(parent) = path.parent() {
                                for (i, pane) in app_guard.panes.iter().enumerate() {
                                    if let Some(fs) = pane.current_state() {
                                        if fs.current_path == parent {
                                            panes_needing_refresh.insert(i);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            let mut app_guard = app.lock().unwrap();
                            let msg = format!("Failed to save file: {}", e);
                            crate::app::log_debug(&msg);
                            app_guard.last_action_msg = Some((msg, std::time::Instant::now()));
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
                    match std::fs::rename(&old, &new) {
                        Ok(_) => {
                            let mut app_guard = app.lock().unwrap();
                            // Undo should move the path back to its original location.
                            app_guard
                                .undo_stack
                                .push(crate::app::UndoAction::Move(new.clone(), old.clone()));
                            app_guard.redo_stack.clear();
                            let _ = event_tx
                                .try_send(AppEvent::RefreshFiles(app_guard.focused_pane_index));
                        }
                        Err(e) => {
                            let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                "Rename failed: {}",
                                e
                            )));
                        }
                    }
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
                    let app_clone = app.clone();
                    tokio::spawn(async move {
                        let copied = terma::utils::copy_recursive(&src, &dest).is_ok();
                        if copied {
                            let mut app_guard = app_clone.lock().unwrap();
                            app_guard
                                .undo_stack
                                .push(crate::app::UndoAction::Copy(src.clone(), dest.clone()));
                            app_guard.redo_stack.clear();
                        }
                        let mut panes_to_refresh = std::collections::HashSet::new();
                        if let Some(parent) = dest.parent() {
                            let app_guard = app_clone.lock().unwrap();
                            for (i, pane) in app_guard.panes.iter().enumerate() {
                                if let Some(fs) = pane.current_state() {
                                    if fs.current_path == parent {
                                        panes_to_refresh.insert(i);
                                    }
                                }
                            }
                        }
                        if panes_to_refresh.is_empty() {
                            let _ = tx.send(AppEvent::RefreshFiles(0)).await;
                        } else {
                            for pane_idx in panes_to_refresh {
                                let _ = tx.send(AppEvent::RefreshFiles(pane_idx)).await;
                            }
                        }
                    });
                }
                AppEvent::Symlink(src, dest) => {
                    let result = {
                        #[cfg(unix)]
                        {
                            std::os::unix::fs::symlink(&src, &dest)
                        }
                        #[cfg(windows)]
                        {
                            if src.is_dir() {
                                std::os::windows::fs::symlink_dir(&src, &dest)
                            } else {
                                std::os::windows::fs::symlink_file(&src, &dest)
                            }
                        }
                    };

                    match result {
                        Ok(_) => {
                            if let Some(parent) = dest.parent() {
                                let app_guard = app.lock().unwrap();
                                for (i, pane) in app_guard.panes.iter().enumerate() {
                                    if let Some(fs) = pane.current_state() {
                                        if fs.current_path == parent {
                                            panes_needing_refresh.insert(i);
                                        }
                                    }
                                }
                            }
                            let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                "Linked {} -> {}",
                                dest.display(),
                                src.display()
                            )));
                        }
                        Err(e) => {
                            let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                "Symlink failed: {}",
                                e
                            )));
                        }
                    }
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
                    summary,
                    remotes,
                    stashes,
                ) => {
                    let mut app_guard = app.lock().unwrap();
                    if let Some(pane) = app_guard.panes.get_mut(p_idx) {
                        if let Some(fs) = pane.current_state_mut() {
                            fs.git_history = history;
                            fs.git_pending = pending;
                            fs.git_branch = branch;
                            fs.git_ahead = ahead;
                            fs.git_behind = behind;
                            fs.git_summary = summary;
                            fs.git_remotes = remotes;
                            fs.git_stashes = stashes;
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
                AppEvent::AddToFavorites(path) => {
                    let mut app_guard = app.lock().unwrap();
                    // Only add if path exists and not already in favorites
                    if path.exists() && !app_guard.starred.contains(&path) {
                        app_guard.starred.push(path.clone());
                        // Wrap save_state to prevent crash if serialization fails
                        if let Err(e) = crate::config::save_state(&app_guard) {
                            crate::app::log_debug(&format!("Failed to save state: {}", e));
                        }
                        let display_name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.display().to_string());
                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                            "Added to favorites: {}",
                            display_name
                        )));
                    }
                    needs_draw = true;
                }
                _ => {}
            }
        }

        // Handle Refreshes
        for pane_idx in panes_needing_refresh.drain() {
            let (path, remote, current_filter) = {
                let app_guard = app.lock().unwrap();
                if let Some(pane) = app_guard.panes.get(pane_idx) {
                    if let Some(fs) = pane.current_state() {
                        (fs.current_path.clone(), fs.remote_session.clone(), fs.search_filter.clone())
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
                let (files, mut metadata) = if let Some(_session) = &remote {
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

                let trimmed_filter = current_filter.trim();
                let (g_files, g_meta) = if trimmed_filter.len() > 3 && remote.is_none() {
                    let search_root = dirs::home_dir().unwrap_or_else(|| path.clone());
                    crate::modules::files::global_search(&search_root, trimmed_filter)
                } else {
                    (Vec::new(), std::collections::HashMap::new())
                };

                {
                    let mut app_guard = app_clone.lock().unwrap();
                    if let Some(pane) = app_guard.panes.get_mut(pane_idx) {
                        if let Some(fs) = pane.current_state_mut() {
                            // RACE CONDITION CHECK:
                            // Only apply if the filter hasn't changed since we started
                            if fs.search_filter != current_filter {
                                return;
                            }

                            // Filter hidden files if needed
                            let filtered_files: Vec<_> = files
                                .into_iter()
                                .filter(|p| {
                                    // 1. Hidden filter
                                    let is_hidden = p
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .map(|s| s.starts_with('.'))
                                        .unwrap_or(false);
                                    
                                    if !fs.show_hidden && is_hidden {
                                        return false;
                                    }

                                    // 2. Search filter
                                    if !fs.search_filter.is_empty() {
                                        let name = p
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or("")
                                            .to_lowercase();
                                        if !name.contains(&fs.search_filter.to_lowercase()) {
                                            return false;
                                        }
                                    }

                                    true
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
                                };

                                if fs.sort_ascending {
                                    ord
                                } else {
                                    ord.reverse()
                                }
                            });

                            fs.local_count = filtered_files.len();

                            if !g_files.is_empty() {
                                filtered_files.push(PathBuf::from("__DIVIDER__"));
                                for gf in g_files {
                                    if !filtered_files.contains(&gf) {
                                        filtered_files.push(gf);
                                    }
                                }
                                metadata.extend(g_meta);
                            }

                            fs.files = filtered_files;
                            fs.metadata = metadata;

                            // Apply pending selection (e.g., after navigate_up)
                            if let Some(pending_path) = fs.pending_select_path.take() {
                                if let Some(idx) = fs.files.iter().position(|p| p == &pending_path)
                                {
                                    fs.selection.selected = Some(idx);
                                    fs.table_state.select(Some(idx));
                                }
                            }
                        }
                    }
                }
                let _ = tx.send(AppEvent::Tick).await;
                if let Some((history, pending, branch, ahead, behind, summary, remotes, stashes)) = git_data {
                    let _ = tx
                        .send(AppEvent::GitHistoryUpdated(
                            pane_idx,
                            0,
                            history,
                            pending,
                            Some(branch),
                            ahead,
                            behind,
                            Some(summary),
                            remotes,
                            stashes,
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

        tokio::time::sleep(Duration::from_millis(33)).await;
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
        for pane in &mut app.panes {
            for tab in &mut pane.tabs {
                tab.local_count = tab.files.len();
            }
        }
        app.focused_pane_index = state.focused_pane_index;

        // Ensure CWD is active on start, keeping history
        if let Ok(cwd) = std::env::current_dir() {
            if let Some(pane) = app.panes.get_mut(0) {
                if let Some(fs) = pane.current_state_mut() {
                    if fs.current_path != cwd {
                        fs.current_path = cwd.clone();
                        crate::event_helpers::push_history(fs, cwd);
                    }
                }
            }
        }

        // Merge favorites (Defaults + Loaded)
        let mut loaded_starred = state.starred;
        for def in app.starred {
            if !loaded_starred.contains(&def) {
                loaded_starred.push(def);
            }
        }
        app.starred = loaded_starred;

        app.remote_bookmarks = state.remote_bookmarks;
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
        app.preview_max_mb = state.preview_max_mb.max(1);
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
