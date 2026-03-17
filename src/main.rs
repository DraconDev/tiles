use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

use dracon_terminal_engine::contracts::InputEvent as Event;
use dracon_terminal_engine::input::parser::Parser as TuiParser;
use dracon_terminal_engine::input::mapping::to_ui_event;
use dracon_terminal_engine::integration::ratatui::TermaBackend as EngineBackend;

// Ratatui Imports
use ratatui::Terminal;

use crate::app::{App, AppEvent, CurrentView, PreviewState};
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

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    let result = run_tty(shutdown_clone).await;
    shutdown.store(true, Ordering::Release);
    result
}

async fn run_tty(shutdown: Arc<AtomicBool>) -> color_eyre::Result<()> {
    crate::app::log_debug("run_tty start");
    let backend = EngineBackend::new(std::io::stdout())?;
    let tile_queue = backend.tile_queue();
    let mut terminal = Terminal::new(backend)?;

    let (app, event_tx, mut event_rx) = setup_app(tile_queue);

    // Watcher Setup
    let tx_clone = event_tx.clone();
    let mut debouncer = notify_debouncer_mini::new_debouncer(
        Duration::from_millis(200),
        move |res: notify_debouncer_mini::DebounceEventResult| {
            match res {
                Ok(events) => {
                    for event in events {
                        crate::app::log_debug(&format!("File watch event: {:?}", event));
                        // Send the path from the event
                        let _ = tx_clone.blocking_send(AppEvent::FilesChangedOnDisk(event.path));
                    }
                }
                Err(e) => {
                    crate::app::log_debug(&format!("File watch error: {:?}", e));
                }
            }
        },
    )?;
    let mut watched_paths: std::collections::HashSet<PathBuf> =
        std::collections::HashSet::new();

    // Helper to sync watched paths with current pane paths
    let mut last_synced_paths: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    let mut sync_watches = |app: &App, debouncer: &mut notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>| {
        let mut current_paths = std::collections::HashSet::new();
        
        // Collect all paths from panes
        for pane in &app.panes {
            for tab in &pane.tabs {
                current_paths.insert(tab.current_path.clone());
            }
        }
        
        // Also watch expanded folders in sidebar for editor view
        for path in &app.expanded_folders {
            current_paths.insert(path.clone());
        }

        // Fast bail-out: skip if nothing changed since last sync
        if current_paths == last_synced_paths {
            return;
        }
        last_synced_paths = current_paths.clone();

        // Add paths that aren't being watched yet
        for path in &current_paths {
            if !watched_paths.contains(path) {
                crate::app::log_debug(&format!("Starting file watch for: {:?}", path));
                if let Ok(()) = debouncer.watcher().watch(path, notify::RecursiveMode::NonRecursive) {
                    watched_paths.insert(path.clone());
                    crate::app::log_debug(&format!("Now watching: {:?}", path));
                } else {
                    crate::app::log_debug(&format!("Failed to watch: {:?}", path));
                }
            }
        }

        // Remove paths that are no longer current
        let to_remove: Vec<_> = watched_paths
            .iter()
            .filter(|p| !current_paths.contains(*p))
            .cloned()
            .collect();
        for path in to_remove {
            // Note: notify_debouncer_mini doesn't support unwatch
            // The watch will just be inactive when path is no longer accessed
            watched_paths.remove(&path);
        }
    };

    // 1. Input Loop (Thread)
    {
        let tx = event_tx.clone();
        let shutdown_input = shutdown.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            use std::os::fd::AsRawFd;
            let mut parser = TuiParser::new();
            let mut stdin = std::io::stdin();
            let fd = stdin.as_raw_fd();
            let mut buffer = [0; 1024];
            while !shutdown_input.load(Ordering::Relaxed) {
                let polled = unsafe {
                    dracon_terminal_engine::backend::tty::poll_input(std::os::fd::BorrowedFd::borrow_raw(fd), 20)
                };
                match polled {
                    Ok(true) => match stdin.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            for byte in buffer.iter().take(n) {
                                if let Some(evt) = parser.advance(*byte) {
                                    if let Some(converted) = crate::event::convert_event(evt) {
                                        if let Some(ui_event) = to_ui_event(&converted) {
                                            let _ = tx.blocking_send(AppEvent::Ui(ui_event));
                                        }
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
                                if let Some(ui_event) = to_ui_event(&converted) {
                                    let _ = tx.blocking_send(AppEvent::Ui(ui_event));
                                }
                                let _ = tx.blocking_send(AppEvent::Raw(converted));
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    // 2. System Stats Loop (Tokio) — polls every 3s; fast enough for the Monitor view
    //    without burning CPU when the user is in Files/Editor/Git.
    {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            let mut sys_mod = crate::modules::system::SystemModule::new();
            loop {
                if let Ok(data) = sys_mod.get_data() {
                    let _ = tx.send(AppEvent::SystemUpdated(data)).await;
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
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

    // Initial watch sync
    {
        let app_guard = app.lock().unwrap();
        sync_watches(&app_guard, &mut debouncer);
    }

    crate::app::log_debug("Entering main loop");

    let mut panes_needing_refresh = std::collections::HashSet::new();
    let mut last_self_save: std::collections::HashMap<PathBuf, std::time::SystemTime> =
        std::collections::HashMap::new();

    loop {
        let mut needs_draw = false;

        while let Ok(event) = event_rx.try_recv() {
            match event {
                AppEvent::Tick => {
                    // Periodically sync file watches with current paths
                    let app_guard = app.lock().unwrap();
                    sync_watches(&app_guard, &mut debouncer);
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
                AppEvent::Ui(_ui_event) => {}
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
                            let connect_result = tokio::task::spawn_blocking(move || {
                                crate::modules::remote::connect_remote(&remote)
                            })
                            .await;

                            match connect_result {
                                Ok(Ok(session)) => {
                                    let _ =
                                        tx.send(AppEvent::RemoteConnected(p_idx, session)).await;
                                }
                                Ok(Err(e)) => {
                                    let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                        "Connection failed: {e}"
                                    )));
                                }
                                Err(e) => {
                                    let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                        "Connection task failed: {e}"
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
                    let t_refresh = std::time::Instant::now();
                    let current_path = {
                        let t_lock = std::time::Instant::now();
                        let mut app_guard = app.lock().unwrap();
                        crate::app::log_debug(&format!("RefreshFiles lock took {:?}", t_lock.elapsed()));
                        let path = app_guard
                            .panes
                            .get(pane_idx)
                            .and_then(|pane| pane.current_state())
                            .map(|fs| fs.current_path.clone());
                        if let Some(ref p) = path {
                            app_guard.push_recent_folder(p.clone());
                        }
                        // Sync watches (fast bail-out if paths unchanged)
                        let t_watch = std::time::Instant::now();
                        sync_watches(&app_guard, &mut debouncer);
                        crate::app::log_debug(&format!("sync_watches took {:?}", t_watch.elapsed()));
                        path
                    };
                    crate::app::log_debug(&format!("RefreshFiles handler total {:?}", t_refresh.elapsed()));
                    if current_path.is_none() {
                        continue;
                    }
                    panes_needing_refresh.insert(pane_idx);
                }
                AppEvent::FilesChangedOnDisk(path) => {
                    crate::app::log_debug(&format!("FilesChangedOnDisk: {:?}", path));
                    
                    // Check if this was a self-save by comparing file mtime
                    if let Some(&saved_mtime) = last_self_save.get(&path) {
                        if let Ok(meta) = std::fs::metadata(&path) {
                            if let Ok(mtime) = meta.modified() {
                                if mtime == saved_mtime {
                                    last_self_save.remove(&path);
                                    continue; // Skip refreshing/reloading for our own saves
                                }
                            }
                        }
                        last_self_save.remove(&path);
                    }

                    let app_guard = app.lock().unwrap();
                    let mut needs_reload = Vec::new();

                    for (i, pane) in app_guard.panes.iter().enumerate() {
                        if let Some(fs) = pane.current_state() {
                            // Check if the changed path is in or under the current directory
                            let current_path = &fs.current_path;
                            let should_refresh = if let Some(parent) = path.parent() {
                                // File changed - check if parent is current dir or path is in current dir
                                parent == current_path.as_path() || path.starts_with(current_path)
                            } else {
                                // Directory changed
                                path == current_path.as_path() || path.starts_with(current_path)
                            };
                            
                            if should_refresh {
                                crate::app::log_debug(&format!("Refreshing pane {} for path {:?}", i, path));
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
                    let (current_dir, preview_limit_mb, remote_session) = {
                        let app_guard = app.lock().unwrap();
                        if let Some(pane) = app_guard.panes.get(pane_idx) {
                            if let Some(fs) = pane.current_state() {
                                (
                                    fs.current_path.clone(),
                                    app_guard.preview_max_mb.max(1),
                                    fs.remote_session.clone(),
                                )
                            } else {
                                (PathBuf::from("."), app_guard.preview_max_mb.max(1), None)
                            }
                        } else {
                            (PathBuf::from("."), app_guard.preview_max_mb.max(1), None)
                        }
                    };

                    tokio::spawn(async move {
                        let path_str = path.to_string_lossy();
                        let content = if let Some(hash) = path_str.strip_prefix("git://") {
                            if let Some(remote) = &remote_session {
                                match crate::modules::remote::show_commit_patch(
                                    remote,
                                    &current_dir,
                                    hash,
                                ) {
                                    Ok(content) => content,
                                    Err(e) => format!("Error fetching commit data: {}", e),
                                }
                            } else {
                                match crate::modules::files::show_commit_patch(&current_dir, hash) {
                                    Ok(content) => content,
                                    Err(e) => format!("Error fetching commit data: {}", e),
                                }
                            }
                        } else if let Some(file_path) = path_str.strip_prefix("git-diff://") {
                            if let Some(remote) = &remote_session {
                                match crate::modules::remote::show_file_diff(
                                    remote,
                                    &current_dir,
                                    file_path,
                                ) {
                                    Ok(content) => content,
                                    Err(e) => format!("Error fetching diff data: {}", e),
                                }
                            } else {
                                match crate::modules::files::show_file_diff(&current_dir, file_path)
                                {
                                    Ok(content) => content,
                                    Err(e) => format!("Error fetching diff data: {}", e),
                                }
                            }
                        } else if let Some(remote) = &remote_session {
                            match crate::modules::remote::is_dir(remote, &path) {
                                Ok(true) => format!(
                                    "\n\n   << PROJECT VIEW: {} >>\n\n   Select a file from the sidebar to begin editing.",
                                    path.file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                        .unwrap_or_else(|| "/".to_string())
                                ),
                                Ok(false) => crate::modules::remote::read_to_string(remote, &path)
                                    .unwrap_or_else(|e| format!("Error reading remote file: {e}")),
                                Err(e) => format!("Error probing remote path: {e}"),
                            }
                        } else if path.is_dir() {
                            format!(
                                "\n\n   << PROJECT VIEW: {} >>\n\n   Select a file from the sidebar to begin editing.",
                                path.file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "/".to_string())
                            )
                        } else {
                            let (is_binary, is_too_large, size_mb) =
                                crate::modules::files::check_file_suitability(
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

                        let mut editor = dracon_terminal_engine::widgets::TextEditor::with_content(&content);
                        if path_str.starts_with("git://") || path_str.starts_with("git-diff://") {
                            editor.language = "diff".to_string();
                            editor.read_only = true;
                        } else if remote_session.is_some() {
                            let is_dir = crate::modules::remote::is_dir(
                                remote_session.as_ref().unwrap(),
                                &path,
                            )
                            .unwrap_or(false);
                            if is_dir {
                                editor.read_only = true;
                            }
                        } else if path.is_dir() {
                            editor.read_only = true;
                        } else {
                            editor.language = path
                                .extension()
                                .and_then(|s| s.to_str())
                                .unwrap_or("")
                                .to_string();
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
                    let remote_for_save = {
                        let app_guard = app.lock().unwrap();
                        app_guard
                            .panes
                            .iter()
                            .find_map(|pane| {
                                let fs = pane.current_state()?;
                                let preview = pane.preview.as_ref()?;
                                if preview.path == path {
                                    fs.remote_session.clone()
                                } else {
                                    None
                                }
                            })
                            .or_else(|| {
                                let fs = app_guard.current_file_state()?;
                                app_guard.editor_state.as_ref()?;
                                fs.remote_session.clone()
                            })
                    };

                    let save_res = if let Some(remote) = &remote_for_save {
                        crate::modules::remote::write_string(remote, &path, &content)
                    } else {
                        std::fs::write(&path, &content)
                    };

                    match save_res {
                        Ok(_) => {
                            if remote_for_save.is_none() {
                                if let Ok(meta) = std::fs::metadata(&path) {
                                    if let Ok(mtime) = meta.modified() {
                                        last_self_save.insert(path.clone(), mtime);
                                    }
                                }
                            }
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
                    let remote = {
                        let app_guard = app.lock().unwrap();
                        app_guard
                            .current_file_state()
                            .and_then(|fs| fs.remote_session.clone())
                    };
                    if let Some(remote) = remote {
                        let _ = crate::modules::remote::create_file(&remote, &path);
                    } else {
                        let _ = std::fs::File::create(&path);
                    }
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(
                        app.lock().unwrap().focused_pane_index,
                    ));
                }
                AppEvent::CreateFolder(path) => {
                    let remote = {
                        let app_guard = app.lock().unwrap();
                        app_guard
                            .current_file_state()
                            .and_then(|fs| fs.remote_session.clone())
                    };
                    if let Some(remote) = remote {
                        let _ = crate::modules::remote::create_dir_all(&remote, &path);
                    } else {
                        let _ = std::fs::create_dir_all(&path);
                    }
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(
                        app.lock().unwrap().focused_pane_index,
                    ));
                }
                AppEvent::Rename(old, new) => {
                    let remote = {
                        let app_guard = app.lock().unwrap();
                        app_guard
                            .current_file_state()
                            .and_then(|fs| fs.remote_session.clone())
                    };
                    let rename_res = if let Some(remote) = &remote {
                        crate::modules::remote::rename(remote, &old, &new)
                    } else {
                        std::fs::rename(&old, &new)
                    };
                    match rename_res {
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
                            let _ = event_tx
                                .try_send(AppEvent::StatusMsg(format!("Rename failed: {}", e)));
                        }
                    }
                }
                AppEvent::Delete(path) => {
                    let remote = {
                        let app_guard = app.lock().unwrap();
                        app_guard
                            .current_file_state()
                            .and_then(|fs| fs.remote_session.clone())
                    };
                    if let Some(remote) = remote {
                        let _ = crate::modules::remote::remove_path(&remote, &path);
                    } else if path.is_dir() {
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
                        let remote = {
                            let app_guard = app_clone.lock().unwrap();
                            app_guard
                                .current_file_state()
                                .and_then(|fs| fs.remote_session.clone())
                        };
                        let copied = if let Some(remote) = &remote {
                            crate::modules::remote::copy_recursive(remote, &src, &dest).is_ok()
                        } else {
                            dracon_terminal_engine::utils::copy_recursive(&src, &dest).is_ok()
                        };
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
                    let remote = {
                        let app_guard = app.lock().unwrap();
                        app_guard
                            .current_file_state()
                            .and_then(|fs| fs.remote_session.clone())
                    };
                    if remote.is_some() {
                        let _ = event_tx.try_send(AppEvent::StatusMsg(
                            "Symlink is not supported for remote panes".to_string(),
                        ));
                        continue;
                    }
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
                            let _ = event_tx
                                .try_send(AppEvent::StatusMsg(format!("Symlink failed: {}", e)));
                        }
                    }
                }
                AppEvent::SpawnTerminal {
                    path,
                    new_tab,
                    remote,
                    command,
                } => {
                    let remote_cmd = remote.as_ref().map(|r| {
                        crate::modules::remote::build_remote_terminal_command(
                            r,
                            &path,
                            command.as_deref(),
                        )
                    });
                    let cmd_str = remote_cmd.as_deref().or(command.as_deref());
                    dracon_terminal_engine::utils::spawn_terminal_at(&path, new_tab, cmd_str);
                }
                AppEvent::SpawnDetached { cmd, args } => {
                    dracon_terminal_engine::utils::spawn_detached(&cmd, args);
                }
                AppEvent::KillProcess(pid) => {
                    let _ = crate::modules::system::SystemModule::kill_process(pid);
                }
                AppEvent::GitHistoryUpdated(
                    p_idx,
                    t_idx,
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
                        // Store git data in the specified tab, or active tab as fallback
                        let tab_idx = if t_idx < pane.tabs.len() { t_idx } else { pane.active_tab_index };
                        if let Some(fs) = pane.tabs.get_mut(tab_idx) {
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
                    let pane_idx = app_guard.focused_pane_index;
                    needs_draw = true;
                    drop(app_guard);
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(pane_idx));
                }
                AppEvent::Editor => {
                    let mut app_guard = app.lock().unwrap();
                    app_guard.save_current_view_prefs();
                    app_guard.current_view = CurrentView::Editor;
                    app_guard.load_view_prefs(CurrentView::Editor);
                    // Editor opens single-pane by default for a focused editing flow.
                    app_guard.apply_split_mode(false);
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
                        crate::config::save_state_quiet(&app_guard);
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
                        (
                            fs.current_path.clone(),
                            fs.remote_session.clone(),
                            fs.search_filter.clone(),
                        )
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
                let list_path = path.clone();
                let list_remote = remote.clone();
                let list_filter = current_filter.clone();
                let (files, mut metadata, g_files, g_meta) =
                    tokio::task::spawn_blocking(move || {
                        let t_dir = std::time::Instant::now();
                        let (files, metadata) = if let Some(session) = &list_remote {
                            crate::modules::remote::read_dir_with_metadata(session, &list_path)
                                .unwrap_or_else(|_| (Vec::new(), std::collections::HashMap::new()))
                        } else {
                            crate::modules::files::read_dir_with_metadata(&list_path)
                        };

                        let trimmed_filter = list_filter.trim();
                        let (g_files, g_meta) = if trimmed_filter.len() > 3 {
                            if let Some(session) = &list_remote {
                                crate::modules::remote::global_search(
                                    session,
                                    &list_path,
                                    trimmed_filter,
                                )
                            } else {
                                let search_root =
                                    dirs::home_dir().unwrap_or_else(|| list_path.clone());
                                crate::modules::files::global_search(&search_root, trimmed_filter)
                            }
                        } else {
                            (Vec::new(), std::collections::HashMap::new())
                        };

                        crate::app::log_debug(&format!("read_dir+search took {:?} for {:?}", t_dir.elapsed(), list_path));
                        (files, metadata, g_files, g_meta)
                    })
                    .await
                    .unwrap_or_else(|_| {
                        (
                            Vec::new(),
                            std::collections::HashMap::new(),
                            Vec::new(),
                            std::collections::HashMap::new(),
                        )
                    });

                {
                    let t_apply = std::time::Instant::now();
                    let mut app_guard = app_clone.lock().unwrap();
                    crate::app::log_debug(&format!("apply lock took {:?}", t_apply.elapsed()));
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
                                if !filtered_files.is_empty() {
                                filtered_files.push(PathBuf::from("__DIVIDER__"));
                                }
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

                let git_path = path.clone();
                let git_remote = remote.clone();
                let app_for_git = app_clone.clone();
                let tx_for_git = tx.clone();
                tokio::spawn(async move {
                    let git_fetch_path = git_path.clone();
                    let git_data = tokio::task::spawn_blocking(move || {
                        if let Some(session) = &git_remote {
                            crate::modules::remote::fetch_git_data(session, &git_fetch_path)
                        } else {
                            crate::modules::files::fetch_git_data(&git_fetch_path)
                        }
                    })
                    .await
                    .ok()
                    .flatten();

                    let path_still_active = {
                        let app_guard = app_for_git.lock().unwrap();
                        app_guard
                            .panes
                            .get(pane_idx)
                            .and_then(|pane| pane.current_state())
                            .map(|fs| fs.current_path == git_path)
                            .unwrap_or(false)
                    };
                    if !path_still_active {
                        return;
                    }

                    // Get the active tab index for this pane so git data lands in the right place
                    let active_tab_idx = {
                        let app_guard = app_for_git.lock().unwrap();
                        app_guard
                            .panes
                            .get(pane_idx)
                            .map(|p| p.active_tab_index)
                            .unwrap_or(0)
                    };

                    let (history, pending, branch, ahead, behind, summary, remotes, stashes) =
                        git_data.unwrap_or_else(|| {
                            (
                                Vec::new(),
                                Vec::new(),
                                String::new(),
                                0,
                                0,
                                String::new(),
                                Vec::new(),
                                Vec::new(),
                            )
                        });

                    let branch_opt = if branch.is_empty() { None } else { Some(branch) };
                    let summary_opt = if summary.is_empty() {
                        None
                    } else {
                        Some(summary)
                    };

                    let _ = tx_for_git
                        .send(AppEvent::GitHistoryUpdated(
                            pane_idx,
                            active_tab_idx,
                            history,
                            pending,
                            branch_opt,
                            ahead,
                            behind,
                            summary_opt,
                            remotes,
                            stashes,
                        ))
                        .await;
                });
            });
        }

        if needs_draw {
            let mut app_guard = app.lock().unwrap();
            if !app_guard.running {
                shutdown.store(true, Ordering::Release);
                break;
            }
            terminal.draw(|f| ui::draw(f, &mut app_guard))?;
        }

        tokio::time::sleep(Duration::from_millis(33)).await;
    }

    Ok(())
}

fn setup_app(
    tile_queue: Arc<Mutex<Vec<dracon_terminal_engine::compositor::engine::TilePlacement>>>,
) -> (
    Arc<Mutex<App>>,
    mpsc::Sender<AppEvent>,
    mpsc::Receiver<AppEvent>,
) {
    let (tx, rx) = mpsc::channel(1000);
    let mut app = App::new(tile_queue);

    if let Some(state) = crate::config::load_state() {
        if !state.panes.is_empty() {
            app.panes = state.panes;
        }
        for pane in &mut app.panes {
            if pane.tabs.is_empty() {
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                pane.tabs.push(crate::state::FileState::new(
                    cwd,
                    None,
                    app.default_show_hidden,
                    app.single_columns.clone(),
                    crate::state::FileColumn::Name,
                    true,
                ));
                pane.active_tab_index = 0;
            } else if pane.active_tab_index >= pane.tabs.len() {
                pane.active_tab_index = 0;
            }

            for tab in &mut pane.tabs {
                // Never trust persisted transient tab data; force a clean first refresh.
                tab.files.clear();
                tab.metadata.clear();
                tab.search_filter.clear();
                tab.local_count = 0;
                tab.selection.clear_multi();
                tab.selection.anchor = None;
                tab.selection.selected = None;
                *tab.table_state.offset_mut() = 0;
            }
        }
        app.focused_pane_index = state.focused_pane_index;
        if app.focused_pane_index >= app.panes.len() {
            app.focused_pane_index = 0;
        }

        // Ensure CWD is active on start, keeping history
        if let Ok(cwd) = std::env::current_dir() {
            if let Some(pane) = app.panes.get_mut(0) {
                if let Some(fs) = pane.current_state_mut() {
                    // Always start local on pane 1/tab active, otherwise a persisted
                    // remote_session can make startup refresh return an empty listing.
                    fs.remote_session = None;
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
        app.sidebar_scope = state.sidebar_scope;
        app.recent_folders = state.recent_folders;
        if let Some(theme_style) = state.theme_style {
            // Migration: users who had the previous default "Cool" should move to
            // the new default "Legacy Red", while preserving custom themes.
            if theme_style == crate::ui::theme::ThemeStyle::preset_cool() {
                crate::ui::theme::set_style_settings(
                    crate::ui::theme::ThemeStyle::preset_legacy_red(),
                );
            } else {
                crate::ui::theme::set_style_settings(theme_style);
            }
        }
    }

    // Prime visible tabs synchronously so startup never renders as empty while waiting
    // for async refresh/tick scheduling.
    prime_visible_tabs(&mut app);

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

fn prime_visible_tabs(app: &mut App) {
    for pane in &mut app.panes {
        if let Some(fs) = pane.current_state_mut() {
            prime_local_file_state(fs);
        }
    }
}

fn prime_local_file_state(fs: &mut crate::state::FileState) {
    if fs.remote_session.is_some() {
        return;
    }

    let (files, mut metadata) = crate::modules::files::read_dir_with_metadata(&fs.current_path);
    let mut filtered_files: Vec<_> = files
        .into_iter()
        .filter(|p| {
            let is_hidden = p
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.starts_with('.'))
                .unwrap_or(false);
            fs.show_hidden || !is_hidden
        })
        .collect();

    filtered_files.sort_by(|a, b| {
        let meta_a = metadata.get(a);
        let meta_b = metadata.get(b);
        let is_dir_a = meta_a.map(|m| m.is_dir).unwrap_or(false);
        let is_dir_b = meta_b.map(|m| m.is_dir).unwrap_or(false);
        if is_dir_a != is_dir_b {
            return if is_dir_a {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }

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
    fs.files = filtered_files;
    fs.metadata = std::mem::take(&mut metadata);
    if fs.selection.selected.is_none() && !fs.files.is_empty() {
        fs.selection.selected = Some(0);
        fs.table_state.select(Some(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use dracon_terminal_engine::compositor::engine::TilePlacement;

    #[test]
    fn startup_prime_populates_first_pane_listing() {
        let tmp = std::env::temp_dir().join(format!("tiles-startup-prime-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("example.txt"), "ok").unwrap();

        let queue: Arc<Mutex<Vec<TilePlacement>>> = Arc::new(Mutex::new(Vec::new()));
        let mut app = App::new(queue);
        if let Some(fs) = app.current_file_state_mut() {
            fs.current_path = tmp.clone();
            fs.files.clear();
            fs.metadata.clear();
            fs.selection.selected = None;
        }

        prime_visible_tabs(&mut app);

        let fs = app.current_file_state().unwrap();
        assert!(
            !fs.files.is_empty(),
            "startup should hydrate first pane file list"
        );
        assert!(fs
            .files
            .iter()
            .any(|p| p.file_name().and_then(|n| n.to_str()) == Some("example.txt")));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
