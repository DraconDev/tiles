use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;
use tokio::sync::mpsc;

// Terma Imports
use terma::input::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use terma::integration::ratatui::TermaBackend;

// Ratatui Imports
use ratatui::Terminal;

use crate::app::{
    App, AppEvent, AppMode, CommandAction, ContextMenuAction, ContextMenuTarget, CurrentView, DropTarget,
    FileCategory, MonitorSubview, ProcessColumn, SettingsSection, SettingsTarget, SidebarTarget, UndoAction,
};
use crate::icons::IconMode;

mod app;
mod config;
mod event;
mod event_helpers;
mod icons;
mod license;
mod modules;
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
    loop {
        let mut needs_draw = false;

        while let Ok(event) = event_rx.try_recv() {
            match event {
                AppEvent::Tick => {
                    needs_draw = true;
                }
                AppEvent::Raw(raw) => {
                    let mut app_guard = app.lock().unwrap();
                    if handle_event(raw, &mut app_guard, event_tx.clone()) {
                        needs_draw = true;
                    }
                }
                AppEvent::SystemUpdated(data) => {
                    let mut app_guard = app.lock().unwrap();
                    update_system_state(&mut app_guard, data);
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
                            crate::app::log_debug(&format!("Attempting SSH connection to {}:{}", remote.host, remote.port));
                            match std::net::TcpStream::connect(format!("{}:{}", remote.host, remote.port)) {
                                Ok(tcp) => {
                                    let mut sess = ssh2::Session::new().unwrap();
                                    sess.set_tcp_stream(tcp);
                                    sess.set_blocking(true);
                                    
                                    if let Err(e) = sess.handshake() {
                                        crate::app::log_debug(&format!("SSH Handshake failed: {}", e));
                                        let _ = tx.try_send(AppEvent::StatusMsg(format!("Handshake failed: {}", e)));
                                        return;
                                    }
                                    
                                    crate::app::log_debug("SSH Handshake successful, attempting authentication...");
                                    
                                    // Try Agent Auth
                                    let mut auth_ok = false;
                                    if let Ok(mut agent) = sess.agent() {
                                        crate::app::log_debug("SSH Agent found, listing identities...");
                                        if agent.connect().is_ok() {
                                            if let Ok(_identities) = agent.list_identities() {
                                                for identity in agent.identities().unwrap() {
                                                    crate::app::log_debug(&format!("Trying agent identity: {}", identity.comment()));
                                                    if agent.userauth(&remote.user, &identity).is_ok() {
                                                        crate::app::log_debug("SSH Agent authentication successful");
                                                        auth_ok = true;
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    
                                    if !auth_ok {
                                        // Try Key Auth if provided
                                        if let Some(key_path) = &remote.key_path {
                                            crate::app::log_debug(&format!("Trying key authentication with: {:?}", key_path));
                                            if sess.userauth_pubkey_file(&remote.user, None, key_path, None).is_ok() {
                                                crate::app::log_debug("SSH Key authentication successful");
                                                auth_ok = true;
                                            }
                                        }
                                    }
                                    
                                    if !auth_ok {
                                        // Try default key paths as fallback
                                        let home = dirs::home_dir().unwrap_or_default();
                                        let default_keys = vec![
                                            home.join(".ssh/id_rsa"),
                                            home.join(".ssh/id_ed25519"),
                                            home.join(".ssh/id_ecdsa"),
                                        ];
                                        for key in default_keys {
                                            if key.exists() {
                                                crate::app::log_debug(&format!("Trying fallback key: {:?}", key));
                                                if sess.userauth_pubkey_file(&remote.user, None, &key, None).is_ok() {
                                                    crate::app::log_debug("SSH Fallback key authentication successful");
                                                    auth_ok = true;
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    
                                    if auth_ok {
                                        crate::app::log_debug("SSH Connection fully established");
                                        let _ = tx.send(AppEvent::RemoteConnected(p_idx, crate::app::RemoteSession {
                                            name: remote.name.clone(),
                                            host: remote.host.clone(),
                                            user: remote.user.clone(),
                                            session: Arc::new(Mutex::new(sess)),
                                        })).await;
                                        let _ = tx.try_send(AppEvent::StatusMsg(format!("Connected to {}", remote.name)));
                                    } else {
                                        crate::app::log_debug("SSH Authentication failed: no successful method found");
                                        let _ = tx.try_send(AppEvent::StatusMsg(format!("Authentication failed for {}", remote.name)));
                                    }
                                }
                                Err(e) => {
                                    crate::app::log_debug(&format!("TCP Connection failed: {}", e));
                                    let _ = tx.try_send(AppEvent::StatusMsg(format!("Connection failed: {}", e)));
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
                            fs.history = vec![PathBuf::from("/")];
                            fs.history_index = 0;
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(pane_idx));
                        }
                    }
                    needs_draw = true;
                }
                AppEvent::MountDisk(name) => {
                    let _ = event_tx.try_send(AppEvent::StatusMsg(format!("Mounting {}...", name)));
                }
                AppEvent::RefreshFiles(idx) => {
                    let mut app_guard = app.lock().unwrap();
                    if let Some(pane) = app_guard.panes.get_mut(idx) {
                        if let Some(fs) = pane.current_state_mut() {
                            let session_arc = fs.remote_session.as_ref().map(|s| s.session.clone());
                            if let Some(arc) = session_arc {
                                let sess = arc.lock().unwrap();
                                crate::modules::files::update_files(fs, Some(&sess));
                            } else {
                                // 1. Local update (immediate)
                                crate::modules::files::update_files(fs, None);
                                
                                // 2. Trigger Global search if needed (background)
                                if fs.search_filter.len() >= 3 {
                                    let filter = fs.search_filter.clone();
                                    let current_path = fs.current_path.clone();
                                    let show_hidden = fs.show_hidden;
                                    let local_files = fs.files.clone();
                                    let tx = event_tx.clone();
                                    let p_idx = idx;
                                    
                                    tokio::spawn(async move {
                                        let (global_files, metadata) = crate::modules::files::perform_global_search(
                                            filter,
                                            current_path,
                                            show_hidden,
                                            local_files,
                                        );
                                        let _ = tx.try_send(AppEvent::GlobalSearchUpdated(p_idx, global_files, metadata));
                                    });
                                }
                            }
                            needs_draw = true;
                        }
                    }
                }
                AppEvent::GlobalSearchUpdated(pane_idx, global_files, metadata) => {
                    let mut app_guard = app.lock().unwrap();
                    if let Some(pane) = app_guard.panes.get_mut(pane_idx) {
                        if let Some(fs) = pane.current_state_mut() {
                            // Merge metadata
                            for (p, m) in metadata {
                                fs.metadata.insert(p, m);
                            }
                            
                            // Combine with local files
                            if !global_files.is_empty() {
                                // Remove any existing divider/global results
                                if let Some(pos) = fs.files.iter().position(|p| p.to_string_lossy() == "__DIVIDER__") {
                                    fs.files.truncate(pos);
                                }
                                
                                fs.files.push(std::path::PathBuf::from("__DIVIDER__"));
                                fs.files.extend(global_files);
                            }
                            needs_draw = true;
                        }
                    }
                }
                AppEvent::PreviewRequested(_dummy_pane_idx, path) => {
                    let mut app_guard = app.lock().unwrap();
                    let category = crate::modules::files::get_file_category(&path);
                    
                    let mut is_text = false;
                    let mut is_archive = false;

                    crate::app::log_debug(&format!("PreviewRequested for {:?}, Category: {:?}", path, category));

                    if let Ok(_m) = std::fs::metadata(&path) {
                        is_text = matches!(category, FileCategory::Text | FileCategory::Script);
                        is_archive = matches!(category, FileCategory::Archive);
                    }

                    crate::app::log_debug(&format!("is_text: {}, is_archive: {}", is_text, is_archive));

                    if is_text {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let mut editor = terma::widgets::editor::TextEditor::with_content(&content);
                            editor.style = ratatui::style::Style::default()
                                .fg(ratatui::style::Color::Rgb(220, 220, 230))
                                .bg(ratatui::style::Color::Rgb(0, 0, 0));
                            editor.cursor_style = ratatui::style::Style::default()
                                .bg(ratatui::style::Color::Rgb(255, 0, 85))
                                .fg(ratatui::style::Color::Black);
                            
                            app_guard.editor_state = Some(crate::app::PreviewState {
                                path: path.clone(),
                                content: content.clone(),
                                scroll: 0,
                                editor: Some(editor),
                                last_saved: None,
                                image_data: None,
                            });
                            app_guard.mode = AppMode::Editor;
                            needs_draw = true;
                        } else {
                            let _ = event_tx.try_send(AppEvent::StatusMsg(format!("Cannot read text file: {}", path.display())));
                        }
                    } else if is_archive {
                        // Try to list contents
                        let tx = event_tx.clone();
                        let p = path.clone();
                        let app_clone = app.clone();
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                        
                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!("Listing contents of {}...", p.file_name().unwrap_or_default().to_string_lossy())));
                        
                        tokio::spawn(async move {
                            crate::app::log_debug(&format!("Archive listing started for: {:?}", p));
                            
                            let has_lsar = std::process::Command::new("which").arg("lsar").output().map(|o| o.status.success()).unwrap_or(false);
                            let has_7z = std::process::Command::new("which").arg("7z").output().map(|o| o.status.success()).unwrap_or(false);
                            let has_unzip = std::process::Command::new("which").arg("unzip").output().map(|o| o.status.success()).unwrap_or(false);
                            let has_tar = std::process::Command::new("which").arg("tar").output().map(|o| o.status.success()).unwrap_or(false);
                            let has_python = std::process::Command::new("which").arg("python3").output().map(|o| o.status.success()).unwrap_or(false);

                            crate::app::log_debug(&format!("Archive tools found: lsar={}, 7z={}, unzip={}, tar={}, python={}", has_lsar, has_7z, has_unzip, has_tar, has_python));

                            let output = if has_lsar {
                                crate::app::log_debug("Using lsar");
                                std::process::Command::new("lsar").arg(&p).output()
                            } else if has_7z {
                                crate::app::log_debug("Using 7z");
                                std::process::Command::new("7z").arg("l").arg(&p).output()
                            } else if has_unzip {
                                crate::app::log_debug("Using unzip");
                                std::process::Command::new("unzip").arg("-l").arg(&p).output()
                            } else if ext == "zip" && has_python {
                                crate::app::log_debug("Using python3 for zip listing");
                                std::process::Command::new("python3").arg("-m").arg("zipfile").arg("-l").arg(&p).output()
                            } else if has_tar {
                                crate::app::log_debug("Using tar");
                                std::process::Command::new("tar").arg("-tf").arg(&p).output()
                            } else {
                                crate::app::log_debug("No suitable listing tool found");
                                Err(std::io::Error::new(std::io::ErrorKind::NotFound, "No suitable tool to list archive contents"))
                            };

                            match output {
                                Ok(out) if out.status.success() => {
                                    let content = String::from_utf8_lossy(&out.stdout).into_owned();
                                    crate::app::log_debug(&format!("Listing success, content len: {}", content.len()));
                                    
                                    let mut editor = terma::widgets::editor::TextEditor::with_content(&content);
                                    editor.read_only = true;
                                    editor.style = ratatui::style::Style::default()
                                        .fg(ratatui::style::Color::Rgb(220, 220, 230))
                                        .bg(ratatui::style::Color::Rgb(0, 0, 0));
                                    editor.cursor_style = ratatui::style::Style::default()
                                        .bg(ratatui::style::Color::Rgb(255, 0, 85))
                                        .fg(ratatui::style::Color::Black);
                                    
                                    let mut app_lock = app_clone.lock().unwrap();
                                    app_lock.editor_state = Some(crate::app::PreviewState {
                                        path: p.clone(),
                                        content: content.clone(),
                                        scroll: 0,
                                        editor: Some(editor),
                                        last_saved: None,
                                        image_data: None,
                                    });
                                    app_lock.mode = AppMode::Viewer;
                                    crate::app::log_debug("AppMode changed to Viewer");
                                }
                                Ok(out) => {
                                    let err = String::from_utf8_lossy(&out.stderr);
                                    crate::app::log_debug(&format!("Listing tool returned error: {}", err));
                                    let _ = tx.try_send(AppEvent::StatusMsg(format!("Listing failed: {}", err.trim())));
                                }
                                Err(e) => {
                                    crate::app::log_debug(&format!("Listing tool error: {}", e));
                                    let _ = tx.try_send(AppEvent::StatusMsg(format!("Listing error: {}", e)));
                                }
                            }
                            let _ = tx.try_send(AppEvent::Tick); // Force a redraw
                        });
                    } else {
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("unknown").to_lowercase();
                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!("Preview not available for .{} (Use Enter to Open)", ext)));
                    }
                }
                                AppEvent::SpawnTerminal {
                                    path,
                                    new_tab: _,
                                    remote: _,
                                    command: _,
                                } => {
                                    let terminals = [
                                        "x-terminal-emulator",
                                        "gnome-terminal",
                                        "konsole",
                                        "alacritty",
                                        "kitty",
                                        "xfce4-terminal",
                                        "termite",
                                        "urxvt",
                                    ];
                                    for term in terminals {
                                        let args = if term == "gnome-terminal" || term == "xfce4-terminal" {
                                            vec!["--working-directory", path.to_str().unwrap()]
                                        } else if term == "konsole" {
                                            vec!["--workdir", path.to_str().unwrap()]
                                        } else if term == "kitty" {
                                            vec!["--directory", path.to_str().unwrap()]
                                        } else {
                                            vec!["--working-directory", path.to_str().unwrap()]
                                        };
                
                                        if std::process::Command::new(term)
                                            .args(&args)
                                            .stdout(std::process::Stdio::null())
                                            .stderr(std::process::Stdio::null())
                                            .stdin(std::process::Stdio::null())
                                            .spawn()
                                            .is_ok()
                                        {
                                            break;
                                        }
                                    }
                                }
                AppEvent::Delete(path) => {
                    let trash_path = dirs::home_dir().unwrap_or_default().join(".local/share/Trash/files");
                    let _ = std::fs::create_dir_all(&trash_path);
                    let file_name = path.file_name().unwrap_or_default();
                    let dest = trash_path.join(file_name);
                    
                    if let Err(e) = std::fs::rename(&path, &dest) {
                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!("Delete failed: {}", e)));
                    } else {
                        let _undo_action = UndoAction::Delete(dest.clone()); // Store where it is in trash
                        let mut app_guard = app.lock().unwrap();
                        app_guard.undo_stack.push(UndoAction::Move(dest, path.clone())); // Undo is Move back
                        app_guard.redo_stack.clear();
                        for i in 0..app_guard.panes.len() {
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(i));
                        }
                    }
                }
                AppEvent::SaveFile(path, content) => {
                    if let Err(e) = std::fs::write(&path, &content) {
                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!("Error saving: {}", e)));
                    } else {
                        // Update last_saved timestamp
                        let mut app_guard = app.lock().unwrap();
                        if let Some(preview) = &mut app_guard.editor_state {
                            if preview.path == path {
                                preview.last_saved = Some(std::time::Instant::now());
                            }
                        }
                        drop(app_guard);
                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!("Saved {}", path.display())));
                    }
                }
                AppEvent::Rename(src, dest) => {
                    if dest.exists() && src != dest {
                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                            "Error: {} already exists!",
                            dest.display()
                        )));
                    } else {
                        if let Err(e) = crate::modules::files::move_recursive(&src, &dest) {
                            let _ = event_tx.try_send(AppEvent::StatusMsg(format!("Error: {}", e)));
                        } else {
                            let mut app_guard = app.lock().unwrap();
                            app_guard.undo_stack.push(UndoAction::Rename(dest.clone(), src.clone()));
                            app_guard.redo_stack.clear();
                            drop(app_guard);
                            let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                "Moved {} to {}",
                                src.display(),
                                dest.display()
                            )));
                            let app_guard = app.lock().unwrap();
                            for i in 0..app_guard.panes.len() {
                                let _ = event_tx.try_send(AppEvent::RefreshFiles(i));
                            }
                        }
                    }
                }
                AppEvent::CreateFile(path) => {
                    if let Err(e) = std::fs::File::create(&path) {
                        let _ = event_tx
                            .try_send(AppEvent::StatusMsg(format!("Error creating file: {}", e)));
                    } else {
                        let _ = event_tx
                            .try_send(AppEvent::StatusMsg(format!("Created {}", path.display())));
                        let app_guard = app.lock().unwrap();
                        for i in 0..app_guard.panes.len() {
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(i));
                        }
                    }
                }
                AppEvent::CreateFolder(path) => {
                    if let Err(e) = std::fs::create_dir(&path) {
                        let _ = event_tx
                            .try_send(AppEvent::StatusMsg(format!("Error creating folder: {}", e)));
                    } else {
                        let _ = event_tx
                            .try_send(AppEvent::StatusMsg(format!("Created {}", path.display())));
                        let app_guard = app.lock().unwrap();
                        for i in 0..app_guard.panes.len() {
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(i));
                        }
                    }
                }
                AppEvent::Copy(src, dest) => {
                    let tx = event_tx.clone();
                    let app_arc = app.clone();
                    tokio::spawn(async move {
                        let task_id = Uuid::new_v4();
                        let _ = tx.send(AppEvent::TaskProgress(task_id, 0.0, format!("Copying {}...", src.file_name().unwrap_or_default().to_string_lossy()))).await;
                        
                        let res = if src.is_dir() {
                            // Simple way for now: use system 'cp' which is fast but hard to track detailed progress
                            // For true progress we'd need to walk and copy manually
                            std::process::Command::new("cp")
                                .arg("-r")
                                .arg(&src)
                                .arg(&dest)
                                .status()
                                .map(|s| s.success())
                                .unwrap_or(false)
                        } else {
                            std::fs::copy(&src, &dest).is_ok()
                        };

                        if res {
                            let mut app_guard = app_arc.lock().unwrap();
                            app_guard.undo_stack.push(UndoAction::Copy(src.clone(), dest.clone()));
                            app_guard.redo_stack.clear();
                            drop(app_guard);
                            let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                "Copied {} to {}",
                                src.display(),
                                dest.display()
                            )));
                            let _ = tx.try_send(AppEvent::RefreshFiles(0));
                            let _ = tx.try_send(AppEvent::RefreshFiles(1));
                        } else {
                            let _ = tx.try_send(AppEvent::StatusMsg(format!("Error copying")));
                        }
                        let _ = tx.send(AppEvent::TaskFinished(task_id)).await;
                    });
                }
                AppEvent::SpawnDetached { cmd, args } => {
                    let tx = event_tx.clone();
                    let cmd_str = cmd.clone();
                    tokio::spawn(async move {
                        match std::process::Command::new(&cmd)
                            .args(&args)
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .stdin(std::process::Stdio::null())
                            .spawn()
                        {
                            Ok(_) => {
                                let _ = tx.try_send(AppEvent::StatusMsg(format!("Launched {}", cmd_str)));
                            }
                            Err(e) => {
                                let _ = tx.try_send(AppEvent::StatusMsg(format!("Failed to launch {}: {}", cmd_str, e)));
                            }
                        }
                    });
                }
                AppEvent::KillProcess(pid) => {
                    let _ = std::process::Command::new("kill")
                        .arg("-9")
                        .arg(pid.to_string())
                        .status();
                }
                AppEvent::SystemMonitor => {
                    let mut app_guard = app.lock().unwrap();
                    if app_guard.current_view == CurrentView::Processes {
                        app_guard.current_view = CurrentView::Files;
                    } else {
                        app_guard.current_view = CurrentView::Processes;
                        app_guard.monitor_subview = MonitorSubview::Overview;
                    }
                    needs_draw = true;
                }
                AppEvent::TaskProgress(id, progress, status) => {
                    let mut app_guard = app.lock().unwrap();
                    if let Some(task) = app_guard.background_tasks.iter_mut().find(|t| t.id == id) {
                        task.progress = progress;
                        task.status = status;
                    } else {
                        // New task
                        app_guard.background_tasks.push(crate::app::BackgroundTask {
                            id,
                            name: status.clone(),
                            progress,
                            status,
                        });
                    }
                    needs_draw = true;
                }
                AppEvent::TaskFinished(id) => {
                    let mut app_guard = app.lock().unwrap();
                    app_guard.background_tasks.retain(|t| t.id != id);
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

        {
            let mut app_guard = app.lock().unwrap();
            if !app_guard.running {
                break;
            }
            if needs_draw {
                app_guard.terminal_size = (terminal.size()?.width, terminal.size()?.height);
                terminal.draw(|f| ui::draw(f, &mut app_guard))?;
            }
        }

        tokio::time::sleep(Duration::from_millis(16)).await;
    }

    Ok(())
}

fn get_open_with_suggestions(app: &App, ext: &str) -> Vec<String> {
    let mut suggestions = terma::utils::get_open_with_suggestions(ext);
    
    // Add custom tools from App settings (persisted choices)
    if let Some(custom_tools) = app.external_tools.get(ext) {
        for tool in custom_tools {
            if !suggestions.contains(&tool.command) {
                suggestions.insert(0, tool.command.clone());
            }
        }
    }

    // Only return programs that actually exist on the system
    suggestions.into_iter().filter(|s| terma::utils::command_exists(s)).collect()
}

fn update_system_state(app: &mut App, data: terma::system::SystemData) {
    let s = &mut app.system_state;
    s.cpu_usage = data.cpu_usage;
    s.cpu_cores = data.cpu_cores.clone();
    s.mem_usage = data.mem_usage;
    s.total_mem = data.total_mem;
    s.swap_usage = data.swap_usage;
    s.total_swap = data.total_swap;
    s.disks = data.disks;
    s.uptime = data.uptime;
    s.processes = data.processes;

    // Sort processes
    let sort_col = app.process_sort_col;
    let sort_asc = app.process_sort_asc;
    s.processes.sort_by(|a, b| {
        let cmp = match sort_col {
            ProcessColumn::Pid => a.pid.cmp(&b.pid),
            ProcessColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            ProcessColumn::Cpu => a
                .cpu
                .partial_cmp(&b.cpu)
                .unwrap_or(std::cmp::Ordering::Equal),
            ProcessColumn::Mem => a
                .mem
                .partial_cmp(&b.mem)
                .unwrap_or(std::cmp::Ordering::Equal),
            ProcessColumn::User => a.user.to_lowercase().cmp(&b.user.to_lowercase()),
            ProcessColumn::Status => a.status.to_lowercase().cmp(&b.status.to_lowercase()),
        };
        if sort_asc {
            cmp
        } else {
            cmp.reverse()
        }
    });

    s.cpu_history.push(data.cpu_usage as u64);
    if s.cpu_history.len() > 100 {
        s.cpu_history.remove(0);
    }

    if s.core_history.len() != data.cpu_cores.len() {
        s.core_history = vec![vec![0; 100]; data.cpu_cores.len()];
    }
    for (i, &usage) in data.cpu_cores.iter().enumerate() {
        s.core_history[i].push(usage as u64);
        if s.core_history[i].len() > 100 {
            s.core_history[i].remove(0);
        }
    }

    let mem_p = if data.total_mem > 0.0 {
        (data.mem_usage / data.total_mem) * 100.0
    } else {
        0.0
    };
    s.mem_history.push(mem_p as u64);
    if s.mem_history.len() > 100 {
        s.mem_history.remove(0);
    }

    let swap_p = if data.total_swap > 0.0 {
        (data.swap_usage / data.total_swap) * 100.0
    } else {
        0.0
    };
    s.swap_history.push(swap_p as u64);
    if s.swap_history.len() > 100 {
        s.swap_history.remove(0);
    }

    if s.last_net_in > 0 {
        let diff_in = data.net_in.saturating_sub(s.last_net_in);
        let diff_out = data.net_out.saturating_sub(s.last_net_out);
        s.net_in_history.push(diff_in);
        s.net_out_history.push(diff_out);
        if s.net_in_history.len() > 100 {
            s.net_in_history.remove(0);
        }
        if s.net_out_history.len() > 100 {
            s.net_out_history.remove(0);
        }
    }
    s.last_net_in = data.net_in;
    s.last_net_out = data.net_out;
    s.net_in = data.net_in;
    s.net_out = data.net_out;

    app.apply_process_sort();
}

fn setup_app(
    tile_queue: Arc<Mutex<Vec<terma::compositor::engine::TilePlacement>>>,
) -> (
    Arc<Mutex<App>>,
    mpsc::Sender<AppEvent>,
    mpsc::Receiver<AppEvent>,
) {
    let (tx, rx) = mpsc::channel(1000);
    let app = Arc::new(Mutex::new(App::new(tile_queue)));
    (app, tx, rx)
}
fn handle_event(evt: Event, app: &mut App, event_tx: mpsc::Sender<AppEvent>) -> bool {
    match evt {
        Event::Resize(w, h) => {
            if let Some(until) = app.ignore_resize_until {
                if std::time::Instant::now() < until {
                    return true;
                }
            }
            app.terminal_size = (w, h);
            return true;
        }
        Event::Key(key) => {
            let has_control = key.modifiers.contains(KeyModifiers::CONTROL);
            let has_alt = key.modifiers.contains(KeyModifiers::ALT);
            let _has_shift = key.modifiers.contains(KeyModifiers::SHIFT);

            // Global Help / Quit Override
            if key.code == KeyCode::F(1) || key.code == KeyCode::Char('?') {
                if let AppMode::Hotkeys = app.mode {
                    app.mode = app.previous_mode.clone();
                } else {
                    app.previous_mode = app.mode.clone();
                    app.mode = AppMode::Hotkeys;
                }
                return true;
            }
            if key.code == KeyCode::Char('q') || key.code == KeyCode::Char('Q') {
                if has_control {
                    app.running = false;
                    return true;
                }
            }

            // 1. Full-Screen Editor Priority (Traps all input)
            if let AppMode::Editor = app.mode {
                if let Some(preview) = &mut app.editor_state {
                    if let Some(editor) = &mut preview.editor {
                        if key.code == KeyCode::Esc {
                            app.mode = AppMode::Normal;
                            app.editor_state = None;
                            return true;
                        }
                        if let KeyCode::Char('s') | KeyCode::Char('S') = key.code {
                            if has_control {
                                let _ = event_tx.try_send(AppEvent::SaveFile(
                                    preview.path.clone(),
                                    editor.get_content(),
                                ));
                                editor.modified = false;
                                return true;
                            }
                        }
                        if let KeyCode::Char('c') | KeyCode::Char('C') = key.code {
                            if has_control {
                                let line = editor.lines[editor.cursor_row].clone();
                                let mut stdout = std::io::stdout();
                                let _ = terma::visuals::osc::copy_to_clipboard(&mut stdout, &line);
                                let _ = event_tx.try_send(AppEvent::StatusMsg(
                                    "Copied line to clipboard".to_string(),
                                ));
                                return true;
                            }
                        }
                        if let KeyCode::Char('f') | KeyCode::Char('F') = key.code {
                            if has_control {
                                app.previous_mode = app.mode.clone();
                                app.mode = AppMode::EditorSearch;
                                // Pre-fill with current filter if any
                                app.input.set_value(editor.filter_query.clone());
                                return true;
                            }
                        }
                        if let KeyCode::Char('r') | KeyCode::Char('R') = key.code {
                            if has_control {
                                app.previous_mode = app.mode.clone();
                                app.mode = AppMode::EditorReplace;
                                app.input.clear();
                                app.replace_buffer.clear();
                                return true;
                            }
                        }
                        if let KeyCode::Char('g') | KeyCode::Char('G') = key.code {
                            if has_control {
                                app.previous_mode = app.mode.clone();
                                app.mode = AppMode::EditorGoToLine;
                                app.input.clear();
                                return true;
                            }
                        }

                        let (w, h) = app.terminal_size;
                        // Adjust area for the border (1 char padding on all sides)
                        let editor_area = ratatui::layout::Rect::new(
                            1,
                            1,
                            w.saturating_sub(2),
                            h.saturating_sub(2),
                        );
                        if editor.handle_event(&evt, editor_area) {
                            // Force auto-save on modification if enabled (defaulting to true now)
                            if app.auto_save && editor.modified {
                                let _ = event_tx.try_send(AppEvent::SaveFile(
                                    preview.path.clone(),
                                    editor.get_content(),
                                ));
                                editor.modified = false;
                            }
                            return true;
                        }
                        return true;
                    } else {
                        if key.code == KeyCode::Esc {
                            app.mode = AppMode::Normal;
                            app.editor_state = None;
                            return true;
                        }
                        return true;
                    }
                }
            }

            // 2. Global Shortcuts
            match key.code {
                KeyCode::Char('b') | KeyCode::Char('B') if has_control => {
                    app.show_sidebar = !app.show_sidebar;
                    return true;
                }
                KeyCode::Char('i') | KeyCode::Char('I') if has_control => {
                    let state = crate::modules::introspection::WorldState::capture(app);
                    if let Ok(json) = serde_json::to_string_pretty(&state) {
                        let _ = std::fs::write("introspection.json", json);
                        app.last_action_msg = Some((
                            "World state dumped to introspection.json".to_string(),
                            std::time::Instant::now(),
                        ));
                    }
                    return true;
                }
                KeyCode::Char('p') | KeyCode::Char('P') if has_control => {
                    app.toggle_split();
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                    return true;
                }
                KeyCode::Char('\\') if has_control => {
                    app.toggle_split();
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                    return true;
                }
                KeyCode::Char('h') | KeyCode::Char('H') if has_control => {
                    let idx = app.toggle_hidden();
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(idx));
                    return true;
                }
                KeyCode::Backspace if has_control => {
                    let idx = app.toggle_hidden();
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(idx));
                    return true;
                }
                KeyCode::Char('g') | KeyCode::Char('G') if has_control => {
                    app.mode = AppMode::Settings;
                    app.settings_scroll = 0;
                    return true;
                }
                KeyCode::Char('n')
                | KeyCode::Char('N')
                | KeyCode::Char('o')
                | KeyCode::Char('O')
                    if has_control =>
                {
                    if let Some(fs) = app.current_file_state() {
                        let _ = event_tx.try_send(AppEvent::SpawnTerminal {
                            path: fs.current_path.clone(),
                            new_tab: true,
                            remote: fs.remote_session.clone(),
                            command: None,
                        });
                    }
                    return true;
                }
                KeyCode::Char('t') | KeyCode::Char('T') if has_control => {
                    if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                        if let Some(fs) = pane.current_state() {
                            let new_fs = fs.clone();
                            pane.open_tab(new_fs);
                            let _ =
                                event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                        }
                    }
                    return true;
                }
                KeyCode::Left if has_alt => {
                    app.resize_sidebar(-2);
                    return true;
                }
                KeyCode::Right if has_alt => {
                    app.resize_sidebar(2);
                    return true;
                }
                KeyCode::Char(' ') if has_control => {
                    app.input.clear();
                    app.mode = AppMode::CommandPalette;
                    crate::event_helpers::update_commands(app);
                    return true;
                }
                KeyCode::Left if has_control => {
                    if app.sidebar_focus {
                        app.resize_sidebar(-2);
                    } else {
                        app.move_to_other_pane();
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                    }
                    return true;
                }
                KeyCode::Right if has_control => {
                    if app.sidebar_focus {
                        app.resize_sidebar(2);
                    } else {
                        app.move_to_other_pane();
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                    }
                    return true;
                }
                _ => {}
            }

            // 3. Modal Handling
            match &app.mode {
                AppMode::ContextMenu {
                    actions,
                    target,
                    selected_index,
                    ..
                } => {
                    let actions = actions.clone();
                    let target = target.clone();
                    let selected_index = *selected_index;

                    match key.code {
                        KeyCode::Esc => {
                            app.mode = AppMode::Normal;
                            return true;
                        }
                        KeyCode::Up => {
                            let mut new_idx = match selected_index {
                                Some(idx) => {
                                    if idx > 0 {
                                        idx - 1
                                    } else {
                                        actions.len().saturating_sub(1)
                                    }
                                }
                                None => actions.len().saturating_sub(1),
                            };
                            if let Some(ContextMenuAction::Separator) = actions.get(new_idx) {
                                if new_idx > 0 {
                                    new_idx -= 1;
                                }
                            }
                            if let AppMode::ContextMenu {
                                selected_index: ref mut si,
                                ..
                            } = app.mode
                            {
                                *si = Some(new_idx);
                            }
                            return true;
                        }
                        KeyCode::Down => {
                            let mut new_idx = match selected_index {
                                Some(idx) => {
                                    if idx < actions.len().saturating_sub(1) {
                                        idx + 1
                                    } else {
                                        0
                                    }
                                }
                                None => 0,
                            };
                            if let Some(ContextMenuAction::Separator) = actions.get(new_idx) {
                                if new_idx < actions.len().saturating_sub(1) {
                                    new_idx += 1;
                                }
                            }
                            if let AppMode::ContextMenu {
                                selected_index: ref mut si,
                                ..
                            } = app.mode
                            {
                                *si = Some(new_idx);
                            }
                            return true;
                        }
                        KeyCode::Enter => {
                            if let Some(idx) = selected_index {
                                if let Some(action) = actions.get(idx) {
                                    if *action != ContextMenuAction::Separator {
                                        let action = action.clone();
                                        let target = target.clone();
                                        app.mode = AppMode::Normal;
                                        crate::event_helpers::handle_context_menu_action(
                                            &action,
                                            &target,
                                            app,
                                            event_tx.clone(),
                                        );
                                    }
                                }
                            }
                            return true;
                        }
                        _ => return true,
                    }
                }
                AppMode::DragDropMenu { source, target } => match key.code {
                    KeyCode::Char('c') | KeyCode::Char('C') => {
                        let dest = target.join(source.file_name().unwrap());
                        let _ = event_tx.try_send(AppEvent::Copy(source.clone(), dest));
                        app.mode = AppMode::Normal;
                        return true;
                    }
                    KeyCode::Char('m') | KeyCode::Char('M') => {
                        let dest = target.join(source.file_name().unwrap());
                        let _ = event_tx.try_send(AppEvent::Rename(source.clone(), dest));
                        app.mode = AppMode::Normal;
                        return true;
                    }
                    KeyCode::Esc => {
                        app.mode = AppMode::Normal;
                        return true;
                    }
                    _ => return true,
                },
                AppMode::Hotkeys => {
                    if key.code == KeyCode::Esc {
                        app.mode = app.previous_mode.clone();
                        return true;
                    }
                    return true;
                }
                AppMode::EditorReplace => match key.code {
                    KeyCode::Esc => {
                        app.mode = app.previous_mode.clone();
                        app.input.clear();
                        app.replace_buffer.clear();
                        return true;
                    }
                    KeyCode::Tab | KeyCode::Enter => {
                        if app.replace_buffer.is_empty() {
                            // Stage 1: Captured Find term
                            app.replace_buffer = app.input.value.clone();
                            app.input.clear();
                        } else {
                            // Stage 2: Captured Replace term
                            let replace_term = app.input.value.clone();
                            let find_term = app.replace_buffer.clone();
                            
                            if let Some(preview) = &mut app.editor_state {
                                if let Some(editor) = &mut preview.editor {
                                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                                        editor.replace_all(&find_term, &replace_term);
                                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!("Replaced all '{}' with '{}'", find_term, replace_term)));
                                        app.mode = app.previous_mode.clone();
                                        app.input.clear();
                                        app.replace_buffer.clear();
                                    } else {
                                        editor.replace_next(&find_term, &replace_term);
                                        // Stay in Replace mode for incremental next
                                        let (w, h) = app.terminal_size;
                                        let area = ratatui::layout::Rect::new(1, 1, w.saturating_sub(2), h.saturating_sub(2));
                                        editor.ensure_cursor_centered(area);
                                    }
                                }
                            }
                        }
                        return true;
                    }
                    _ => return app.input.handle_event(&evt),
                },
                AppMode::CommandPalette => match key.code {
                    KeyCode::Esc => {
                        app.mode = AppMode::Normal;
                        return true;
                    }
                    KeyCode::Enter => {
                        if let Some(cmd) = app.filtered_commands.get(app.command_index).cloned() {
                            crate::event_helpers::execute_command(
                                cmd.action,
                                app,
                                event_tx.clone(),
                            );
                        }
                        app.mode = AppMode::Normal;
                        app.input.clear();
                        return true;
                    }
                    _ => {
                        let handled = app.input.handle_event(&evt);
                        if handled {
                            crate::event_helpers::update_commands(app);
                        }
                        return handled;
                    }
                },
                AppMode::EditorSearch => match key.code {
                    KeyCode::Esc => {
                        // Clear filter on Cancel
                        if let Some(preview) = &mut app.editor_state {
                            if let Some(editor) = &mut preview.editor {
                                editor.set_filter("");
                            }
                        }
                        app.mode = app.previous_mode.clone();
                        app.input.clear();
                        return true;
                    }
                    KeyCode::Enter => {
                        // Clear filter on Enter (normalize page but keep cursor position)
                        if let Some(preview) = &mut app.editor_state {
                            if let Some(editor) = &mut preview.editor {
                                editor.set_filter("");
                                // Center the view on the result
                                let (w, h) = app.terminal_size;
                                let area = ratatui::layout::Rect::new(1, 1, w.saturating_sub(2), h.saturating_sub(2));
                                editor.ensure_cursor_centered(area);
                            }
                        }
                        app.mode = app.previous_mode.clone();
                        app.input.clear();
                        return true;
                    }
                    KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown => {
                        if let Some(preview) = &mut app.editor_state {
                            if let Some(editor) = &mut preview.editor {
                                let (w, h) = app.terminal_size;
                                let area = ratatui::layout::Rect::new(1, 1, w.saturating_sub(2), h.saturating_sub(2));
                                editor.handle_event(&evt, area);
                            }
                        }
                        return true;
                    }
                    _ => {
                        let handled = app.input.handle_event(&evt);
                        if handled {
                            // Live Filter Update
                            if let Some(preview) = &mut app.editor_state {
                                if let Some(editor) = &mut preview.editor {
                                    editor.set_filter(&app.input.value);
                                }
                            }
                        }
                        return handled;
                    }
                },
                AppMode::EditorGoToLine => match key.code {
                    KeyCode::Esc => {
                        app.mode = app.previous_mode.clone();
                        app.input.clear();
                        return true;
                    }
                    KeyCode::Enter => {
                        let line_str = app.input.value.clone();
                        if let Ok(line_num) = line_str.parse::<usize>() {
                            if let Some(preview) = &mut app.editor_state {
                                if let Some(editor) = &mut preview.editor {
                                    let target = line_num.saturating_sub(1); // 1-based to 0-based
                                    editor.cursor_row = std::cmp::min(target, editor.lines.len().saturating_sub(1));
                                    editor.cursor_col = 0;
                                    
                                    // Ensure screen jumps to cursor and centers it
                                    let (w, h) = app.terminal_size;
                                    let area = ratatui::layout::Rect::new(1, 1, w.saturating_sub(2), h.saturating_sub(2));
                                    editor.ensure_cursor_centered(area);
                                }
                            }
                        }
                        app.mode = app.previous_mode.clone();
                        app.input.clear();
                        return true;
                    }
                    _ => return app.input.handle_event(&evt),
                },
                AppMode::AddRemote(idx) => {
                    let idx = *idx;
                    match key.code {
                        KeyCode::Esc => {
                            app.mode = AppMode::Normal;
                            app.input.clear();
                            return true;
                        }
                        KeyCode::Tab | KeyCode::Enter => {
                            let val = app.input.value.clone();
                            match idx {
                                0 => app.pending_remote.name = val,
                                1 => app.pending_remote.host = val,
                                2 => app.pending_remote.user = val,
                                3 => app.pending_remote.port = val.parse().unwrap_or(22),
                                4 => {
                                    app.pending_remote.key_path = if val.is_empty() {
                                        None
                                    } else {
                                        Some(std::path::PathBuf::from(val))
                                    }
                                }
                                _ => {}
                            }
                            if idx < 4 {
                                app.mode = AppMode::AddRemote(idx + 1);
                                let next_val = match idx + 1 {
                                    1 => app.pending_remote.host.clone(),
                                    2 => app.pending_remote.user.clone(),
                                    3 => app.pending_remote.port.to_string(),
                                    4 => app
                                        .pending_remote
                                        .key_path
                                        .as_ref()
                                        .map(|p| p.to_string_lossy().to_string())
                                        .unwrap_or_default(),
                                    _ => String::new(),
                                };
                                app.input.set_value(next_val);
                            } else {
                                app.remote_bookmarks.push(app.pending_remote.clone());
                                let _ = crate::config::save_state(app);
                                app.mode = AppMode::Normal;
                                app.input.clear();
                            }
                            return true;
                        }
                        _ => return app.input.handle_event(&evt),
                    }
                }
                AppMode::Header(idx) => {
                    let idx = *idx;
                    let total_tabs: usize = app.panes.iter().map(|p| p.tabs.len()).sum();
                    let max_idx = 4 + total_tabs; // 5 icons (0-4) + tabs

                    match key.code {
                        KeyCode::Esc => {
                            app.mode = AppMode::Normal;
                            return true;
                        }
                        KeyCode::Down => {
                            if idx >= 5 {
                                let target_tab_idx = idx - 5;
                                let mut current_global = 0;
                                for (p_i, pane) in app.panes.iter_mut().enumerate() {
                                    for _t_i in 0..pane.tabs.len() {
                                        if current_global == target_tab_idx {
                                            pane.active_tab_index = _t_i;
                                            app.focused_pane_index = p_i;
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(p_i));
                                            break;
                                        }
                                        current_global += 1;
                                    }
                                }
                            }
                            app.mode = AppMode::Normal;
                            return true;
                        }
                        KeyCode::Left => {
                            if idx > 0 {
                                let new_idx = idx - 1;
                                app.mode = AppMode::Header(new_idx);
                                if new_idx >= 5 {
                                    let target_tab_idx = new_idx - 5;
                                    let mut current_global = 0;
                                    for (p_i, pane) in app.panes.iter().enumerate() {
                                        for _t_i in 0..pane.tabs.len() {
                                            if current_global == target_tab_idx {
                                                app.focused_pane_index = p_i;
                                                // Note: we don't switch active_tab_index until Enter is pressed
                                                break;
                                            }
                                            current_global += 1;
                                        }
                                    }
                                }
                            }
                            return true;
                        }
                        KeyCode::Right => {
                            if idx < max_idx {
                                let new_idx = idx + 1;
                                app.mode = AppMode::Header(new_idx);
                                if new_idx >= 5 {
                                    let target_tab_idx = new_idx - 5;
                                    let mut current_global = 0;
                                    for (p_i, pane) in app.panes.iter().enumerate() {
                                        for _t_i in 0..pane.tabs.len() {
                                            if current_global == target_tab_idx {
                                                app.focused_pane_index = p_i;
                                                break;
                                            }
                                            current_global += 1;
                                        }
                                    }
                                }
                            }
                            return true;
                        }
                        KeyCode::Enter => {
                            if idx <= 4 {
                                match idx {
                                    0 => app.mode = AppMode::Settings,
                                    1 => {
                                        if let Some(fs) = app.current_file_state_mut() {
                                            crate::event_helpers::navigate_back(fs);
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(
                                                app.focused_pane_index,
                                            ));
                                        }
                                    }
                                    2 => {
                                        if let Some(fs) = app.current_file_state_mut() {
                                            crate::event_helpers::navigate_forward(fs);
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(
                                                app.focused_pane_index,
                                            ));
                                        }
                                    }
                                    3 => {
                                        app.toggle_split();
                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                                    }
                                    4 => {
                                        let _ = event_tx.try_send(AppEvent::SystemMonitor);
                                    }
                                    _ => {}
                                }
                            } else {
                                // Switch to selected tab
                                let target_tab_idx = idx - 5;
                                let mut current_global = 0;
                                for (p_i, pane) in app.panes.iter_mut().enumerate() {
                                    for _t_i in 0..pane.tabs.len() {
                                        if current_global == target_tab_idx {
                                            pane.active_tab_index = _t_i;
                                            app.focused_pane_index = p_i;
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(p_i));
                                            app.mode = AppMode::Normal;
                                            return true;
                                        }
                                        current_global += 1;
                                    }
                                }
                            }
                            if let AppMode::Header(_) = app.mode {
                                app.mode = AppMode::Normal;
                            }
                            return true;
                        }
                        _ => return true,
                    }
                }
                AppMode::Viewer => {
                    if key.code == KeyCode::Esc || key.code == KeyCode::Char(' ') {
                        app.mode = AppMode::Normal;
                        app.editor_state = None;
                        return true;
                    }
                    if let Some(preview) = &mut app.editor_state {
                        if let Some(editor) = &mut preview.editor {
                            if let KeyCode::Char('f') | KeyCode::Char('F') = key.code {
                                if has_control {
                                    app.previous_mode = app.mode.clone();
                                    app.mode = AppMode::EditorSearch;
                                    // Pre-fill with current filter if any
                                    app.input.set_value(editor.filter_query.clone());
                                    return true;
                                }
                            }
                            if let KeyCode::Char('g') | KeyCode::Char('G') = key.code {
                                if has_control {
                                    app.previous_mode = app.mode.clone();
                                    app.mode = AppMode::EditorGoToLine;
                                    app.input.clear();
                                    return true;
                                }
                            }

                            let (w, h) = app.terminal_size;
                            let editor_area = ratatui::layout::Rect::new(1, 1, w.saturating_sub(2), h.saturating_sub(2));
                            editor.handle_event(&evt, editor_area);
                        }
                    }
                    return true;
                }
                AppMode::OpenWith(path) => match key.code {
                    KeyCode::Esc => {
                        app.mode = AppMode::Normal;
                        app.input.clear();
                        app.open_with_index = 0;
                        return true;
                    }
                    KeyCode::Up => {
                        if app.open_with_index > 0 {
                            app.open_with_index -= 1;
                        }
                        return true;
                    }
                    KeyCode::Down => {
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                        let mut suggestions = terma::utils::get_open_with_suggestions(&ext);
                        if let Some(custom_tools) = app.external_tools.get(&ext) {
                            for tool in custom_tools {
                                if !suggestions.contains(&tool.command) {
                                    suggestions.insert(0, tool.command.clone());
                                }
                            }
                        }
                        if !app.input.value.is_empty() {
                            let query = app.input.value.to_lowercase();
                            suggestions.retain(|s| s.to_lowercase().contains(&query));
                        }
                        if app.open_with_index < suggestions.len().saturating_sub(1) {
                            app.open_with_index += 1;
                        }
                        return true;
                    }
                    KeyCode::Tab => {
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                        let mut suggestions = terma::utils::get_open_with_suggestions(&ext);
                        if let Some(custom_tools) = app.external_tools.get(&ext) {
                            for tool in custom_tools {
                                if !suggestions.contains(&tool.command) {
                                    suggestions.insert(0, tool.command.clone());
                                }
                            }
                        }
                        if !app.input.value.is_empty() {
                            let query = app.input.value.to_lowercase();
                            suggestions.retain(|s| s.to_lowercase().contains(&query));
                        }
                        if let Some(s) = suggestions.get(app.open_with_index) {
                            app.input.set_value(s.to_string());
                        }
                        return true;
                    }
                    KeyCode::Enter => {
                        let mut cmd = app.input.value.clone();
                        
                        // If input is empty or matches selection, use the selected suggestion
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                        let mut suggestions = terma::utils::get_open_with_suggestions(&ext);
                        
                        // Merge with custom tools
                        if let Some(custom_tools) = app.external_tools.get(&ext) {
                            for tool in custom_tools {
                                if !suggestions.contains(&tool.command) {
                                    suggestions.insert(0, tool.command.clone());
                                }
                            }
                        }

                        if !app.input.value.is_empty() {
                            let query = app.input.value.to_lowercase();
                            suggestions.retain(|s| s.to_lowercase().contains(&query));
                        }
                        
                        if let Some(s) = suggestions.get(app.open_with_index) {
                            // If user is just typing a partial and hits Enter, launch the highlighted one
                            cmd = s.to_string();
                        }

                        if !cmd.is_empty() {
                            // Persist this choice for this extension
                            let tools = app.external_tools.entry(ext.clone()).or_default();
                            if !tools.iter().any(|t| t.command == cmd) {
                                tools.insert(0, crate::config::ExternalTool {
                                    name: cmd.clone(),
                                    command: cmd.clone(),
                                });
                                let _ = crate::config::save_state(app);
                            }

                            let _ = event_tx.try_send(AppEvent::SpawnDetached {
                                cmd,
                                args: vec![path.to_string_lossy().to_string()],
                            });
                        }
                        app.mode = AppMode::Normal;
                        app.input.clear();
                        app.open_with_index = 0;
                        return true;
                    }
                    _ => {
                        let handled = app.input.handle_event(&evt);
                        if handled {
                            app.open_with_index = 0; // Reset index on type
                        }
                        return handled;
                    }
                },
                AppMode::Highlight => {
                    if let KeyCode::Char(c) = key.code {
                        if let Some(digit) = c.to_digit(10) {
                            if digit <= 6 {
                                let color = if digit == 0 { None } else { Some(digit as u8) };
                                if let Some(fs) = app.current_file_state() {
                                    let mut paths = Vec::new();
                                    if !fs.multi_select.is_empty() {
                                        for &idx in &fs.multi_select {
                                            if let Some(p) = fs.files.get(idx) {
                                                paths.push(p.clone());
                                            }
                                        }
                                    } else if let Some(idx) = fs.selected_index {
                                        if let Some(p) = fs.files.get(idx) {
                                            paths.push(p.clone());
                                        }
                                    }
                                    for p in paths {
                                        if let Some(col) = color {
                                            app.path_colors.insert(p, col);
                                        } else {
                                            app.path_colors.remove(&p);
                                        }
                                    }
                                    let _ = crate::config::save_state(app);
                                }
                                app.mode = AppMode::Normal;
                                return true;
                            }
                        }
                    } else if key.code == KeyCode::Esc {
                        app.mode = AppMode::Normal;
                        return true;
                    }
                    return false;
                }
                AppMode::Settings => match key.code {
                    KeyCode::Esc => {
                        app.mode = AppMode::Normal;
                        return true;
                    }
                    KeyCode::Char('1') => {
                        app.settings_section = SettingsSection::Columns;
                        app.settings_index = 0;
                        return true;
                    }
                    KeyCode::Char('2') => {
                        app.settings_section = SettingsSection::Tabs;
                        app.settings_index = 0;
                        return true;
                    }
                    KeyCode::Char('3') => {
                        app.settings_section = SettingsSection::General;
                        app.settings_index = 0;
                        return true;
                    }
                    KeyCode::Char('4') => {
                        app.settings_section = SettingsSection::Remotes;
                        app.settings_index = 0;
                        return true;
                    }
                    KeyCode::Char('5') => {
                        app.settings_section = SettingsSection::Shortcuts;
                        app.settings_index = 0;
                        return true;
                    }
                    KeyCode::Up => {
                        app.settings_index = app.settings_index.saturating_sub(1);
                        return true;
                    }
                    KeyCode::Down => {
                        let max = match app.settings_section {
                            SettingsSection::General => 4, // 5 items: 0-4
                            SettingsSection::Columns => 3, // 4 items: 0-3
                            _ => 0,
                        };
                        if app.settings_index < max {
                            app.settings_index += 1;
                        }
                        return true;
                    }
                    KeyCode::Left => {
                        match app.settings_section {
                            SettingsSection::Columns => {
                                app.settings_target = SettingsTarget::SingleMode;
                            }
                            _ => {
                                // Switch section left
                                app.settings_section = match app.settings_section {
                                    SettingsSection::Columns => SettingsSection::Shortcuts,
                                    SettingsSection::Tabs => SettingsSection::Columns,
                                    SettingsSection::General => SettingsSection::Tabs,
                                    SettingsSection::Remotes => SettingsSection::General,
                                    SettingsSection::Shortcuts => SettingsSection::Remotes,
                                };
                                app.settings_index = 0;
                            }
                        }
                        return true;
                    }
                    KeyCode::Right => {
                        match app.settings_section {
                            SettingsSection::Columns => {
                                app.settings_target = SettingsTarget::SplitMode;
                            }
                            _ => {
                                // Switch section right
                                app.settings_section = match app.settings_section {
                                    SettingsSection::Columns => SettingsSection::Tabs,
                                    SettingsSection::Tabs => SettingsSection::General,
                                    SettingsSection::General => SettingsSection::Remotes,
                                    SettingsSection::Remotes => SettingsSection::Shortcuts,
                                    SettingsSection::Shortcuts => SettingsSection::Columns,
                                };
                                app.settings_index = 0;
                            }
                        }
                        return true;
                    }
                    KeyCode::Enter => {
                        match app.settings_section {
                            SettingsSection::General => {
                                match app.settings_index {
                                    0 => app.default_show_hidden = !app.default_show_hidden,
                                    1 => app.confirm_delete = !app.confirm_delete,
                                    2 => app.smart_date = !app.smart_date,
                                    3 => app.auto_save = !app.auto_save,
                                    4 => app.icon_mode = match app.icon_mode {
                                        IconMode::Nerd => IconMode::Unicode,
                                        IconMode::Unicode => IconMode::ASCII,
                                        IconMode::ASCII => IconMode::Nerd,
                                    },
                                    _ => {}
                                }
                                let _ = crate::config::save_state(app);
                            }
                            SettingsSection::Columns => {
                                let col = match app.settings_index {
                                    0 => crate::app::FileColumn::Size,
                                    1 => crate::app::FileColumn::Modified,
                                    2 => crate::app::FileColumn::Created,
                                    3 => crate::app::FileColumn::Permissions,
                                    _ => crate::app::FileColumn::Name,
                                };
                                if col != crate::app::FileColumn::Name {
                                    app.toggle_column(col);
                                }
                            }
                            _ => {}
                        }
                        return true;
                    }
                    // Legacy/Direct Shortcuts
                    KeyCode::Char('t') if app.settings_section == SettingsSection::General => {
                        app.smart_date = !app.smart_date;
                        let _ = crate::config::save_state(app);
                        return true;
                    }
                    KeyCode::Char('a') if app.settings_section == SettingsSection::General => {
                        app.auto_save = !app.auto_save;
                        let _ = crate::config::save_state(app);
                        return true;
                    }
                    KeyCode::Char('i') if app.settings_section == SettingsSection::General => {
                        app.icon_mode = match app.icon_mode {
                            IconMode::Nerd => IconMode::Unicode,
                            IconMode::Unicode => IconMode::ASCII,
                            IconMode::ASCII => IconMode::Nerd,
                        };
                        let _ = crate::config::save_state(app);
                        return true;
                    }
                    KeyCode::Char('h') if app.settings_section == SettingsSection::General => {
                        app.default_show_hidden = !app.default_show_hidden;
                        let _ = crate::config::save_state(app);
                        return true;
                    }
                    KeyCode::Char('d') if app.settings_section == SettingsSection::General => {
                        app.confirm_delete = !app.confirm_delete;
                        let _ = crate::config::save_state(app);
                        return true;
                    }
                    _ => return false,
                },
                AppMode::NewFile | AppMode::NewFolder | AppMode::Rename | AppMode::Delete => match key.code {
                        KeyCode::Esc => {
                            app.mode = AppMode::Normal;
                            app.input.clear();
                            app.rename_selected = false;
                            return true;
                        }
                        KeyCode::Char('[') if has_control => {
                            app.mode = AppMode::Normal;
                            app.input.clear();
                            return true;
                        }
                        KeyCode::Char(c) if c == '\x1b' => {
                            app.mode = AppMode::Normal;
                            app.input.clear();
                            return true;
                        }
                        KeyCode::Backspace if has_control || has_alt => {
                            delete_word_backwards(&mut app.input.value);
                            app.input.cursor_position = app.input.value.len();
                            app.rename_selected = false;
                            return true;
                        }
                        KeyCode::Char('w') if has_control => {
                            delete_word_backwards(&mut app.input.value);
                            app.input.cursor_position = app.input.value.len();
                            app.rename_selected = false;
                            return true;
                        }
                        KeyCode::Char('u') if has_control => {
                            app.input.clear();
                            app.rename_selected = false;
                            return true;
                        }
                        KeyCode::Enter => {
                            let input = app.input.value.clone();
                            if let Some(fs) = app.current_file_state() {
                                let path = fs.current_path.join(&input);
                                crate::app::log_debug(&format!("Action {:?} triggered for path: {:?}", app.mode, path));
                                match app.mode {
                                    AppMode::NewFile => {
                                        let _ = event_tx.try_send(AppEvent::CreateFile(path));
                                    }
                                    AppMode::NewFolder => {
                                        let _ = event_tx.try_send(AppEvent::CreateFolder(path));
                                    }
                                    AppMode::Rename => {
                                        if let Some(idx) = fs.selected_index {
                                            if let Some(old) = fs.files.get(idx) {
                                                let _ = event_tx.try_send(AppEvent::Rename(
                                                    old.clone(),
                                                    old.parent().unwrap().join(&input),
                                                ));
                                            }
                                        }
                                    }
                                    AppMode::Delete => {
                                        let ic = input.trim().to_lowercase();
                                        if ic == "y"
                                            || ic == "yes"
                                            || ic.is_empty()
                                            || !app.confirm_delete
                                        {
                                            let mut paths = Vec::new();
                                            if !fs.multi_select.is_empty() {
                                                for &idx in &fs.multi_select {
                                                    if let Some(p) = fs.files.get(idx) {
                                                        paths.push(p.clone());
                                                    }
                                                }
                                            } else if let Some(idx) = fs.selected_index {
                                                if let Some(path) = fs.files.get(idx) {
                                                    paths.push(path.clone());
                                                }
                                            }
                                            for p in paths {
                                                let _ = event_tx.try_send(AppEvent::Delete(p));
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            } else {
                                crate::app::log_debug("Failed to get current file state during action.");
                            }
                            app.mode = AppMode::Normal;
                            app.input.clear();
                            app.rename_selected = false;
                            return true;
                        }
                        KeyCode::Char(c) if app.mode == AppMode::Rename && app.rename_selected && !has_control && !has_alt => {
                            // Replace stem
                            if let Some(idx) = app.input.value.rfind('.') {
                                if idx > 0 {
                                    let ext = app.input.value[idx..].to_string();
                                    app.input.set_value(format!("{}{}", c, ext));
                                    app.input.cursor_position = 1;
                                } else {
                                    app.input.set_value(c.to_string());
                                    app.input.cursor_position = 1;
                                }
                            } else {
                                app.input.set_value(c.to_string());
                                app.input.cursor_position = 1;
                            }
                            app.rename_selected = false;
                            return true;
                        }
                        KeyCode::Backspace if app.mode == AppMode::Rename && app.rename_selected && !has_control => {
                            // Delete stem
                            if let Some(idx) = app.input.value.rfind('.') {
                                if idx > 0 {
                                    let ext = app.input.value[idx..].to_string();
                                    app.input.set_value(ext);
                                    app.input.cursor_position = 0;
                                } else {
                                    app.input.clear();
                                }
                            } else {
                                app.input.clear();
                            }
                            app.rename_selected = false;
                            return true;
                        }
                        _ => {
                            let res = app.input.handle_event(&evt);
                            if res && app.mode == AppMode::Rename {
                                app.rename_selected = false;
                            }
                            // Also clear on navigation keys that don't "modify" but should end selection
                            if app.mode == AppMode::Rename && app.rename_selected {
                                if let KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End = key.code {
                                    app.rename_selected = false;
                                }
                            }
                            return res;
                                            }
                                        }
                                        _ => {                    // Standard Navigation & Actions
                    if key.code == KeyCode::Esc {
                        if app.current_view == crate::app::CurrentView::Processes {
                            if !app.process_search_filter.is_empty() {
                                app.process_search_filter.clear();
                                app.process_selected_idx = Some(0);
                                *app.process_table_state.offset_mut() = 0;
                            } else {
                                app.current_view = crate::app::CurrentView::Files;
                            }
                            return true;
                        }
                        app.mode = AppMode::Normal;
                        if let Some(fs) = app.current_file_state_mut() {
                            fs.multi_select.clear();
                            fs.selection_anchor = None;
                            if !fs.search_filter.is_empty() {
                                fs.search_filter.clear();
                                fs.selected_index = Some(0);
                                *fs.table_state.offset_mut() = 0;
                                let _ = event_tx
                                    .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                            }
                        }
                        return true;
                    }
                    match key.code {
                        KeyCode::Char('1') if app.current_view == CurrentView::Processes => {
                            app.monitor_subview = MonitorSubview::Overview;
                            return true;
                        }
                        KeyCode::Char('2') if app.current_view == CurrentView::Processes => {
                            app.monitor_subview = MonitorSubview::Applications;
                            return true;
                        }
                        KeyCode::Char('3') if app.current_view == CurrentView::Processes => {
                            app.monitor_subview = MonitorSubview::Processes;
                            return true;
                        }
                        KeyCode::Char('c') if has_control => {
                            if let Some(fs) = app.current_file_state() {
                                if let Some(idx) = fs.selected_index {
                                    if let Some(path) = fs.files.get(idx) {
                                        app.clipboard =
                                            Some((path.clone(), crate::app::ClipboardOp::Copy));
                                    }
                                }
                            }
                            return true;
                        }
                        KeyCode::Char('x') if has_control => {
                            if let Some(fs) = app.current_file_state() {
                                if let Some(idx) = fs.selected_index {
                                    if let Some(path) = fs.files.get(idx) {
                                        app.clipboard =
                                            Some((path.clone(), crate::app::ClipboardOp::Cut));
                                    }
                                }
                            }
                            return true;
                        }
                        KeyCode::Char('v') if has_control => {
                            if let Some((src, op)) = app.clipboard.clone() {
                                if let Some(fs) = app.current_file_state() {
                                    let dest = fs.current_path.join(src.file_name().unwrap());
                                    match op {
                                        crate::app::ClipboardOp::Copy => {
                                            let _ = event_tx.try_send(AppEvent::Copy(src, dest));
                                        }
                                        crate::app::ClipboardOp::Cut => {
                                            let _ = event_tx.try_send(AppEvent::Rename(src, dest));
                                            app.clipboard = None;
                                        }
                                    }
                                }
                            }
                            return true;
                        }
                        KeyCode::Char('a') if has_control => {
                            if let Some(fs) = app.current_file_state_mut() {
                                fs.multi_select = (0..fs.files.len()).collect();
                            }
                            return true;
                        }
                        KeyCode::Char('z') if has_control => {
                            if let Some(action) = app.undo_stack.pop() {
                                match action.clone() {
                                    UndoAction::Rename(old, new) | UndoAction::Move(old, new) => {
                                        let _ = std::fs::rename(&old, &new);
                                        app.redo_stack.push(action);
                                    }
                                    UndoAction::Copy(src, dest) => {
                                        let _ = if dest.is_dir() {
                                            std::fs::remove_dir_all(&dest)
                                        } else {
                                            std::fs::remove_file(&dest)
                                        };
                                        app.redo_stack.push(UndoAction::Copy(src, dest));
                                    }
                                    _ => {}
                                }
                                for i in 0..app.panes.len() {
                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(i));
                                }
                            } else if let Some(fs) = app.current_file_state_mut() {
                                if !fs.search_filter.is_empty() {
                                    fs.search_filter.clear();
                                    let _ = event_tx
                                        .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                }
                            }
                            return true;
                        }
                        KeyCode::Char('y') if has_control => {
                            if let Some(action) = app.redo_stack.pop() {
                                match action.clone() {
                                    UndoAction::Rename(old, new) | UndoAction::Move(old, new) => {
                                        let _ = std::fs::rename(&old, &new);
                                        app.undo_stack.push(action);
                                    }
                                    UndoAction::Copy(src, dest) => {
                                        let _ = crate::modules::files::copy_recursive(&src, &dest);
                                        app.undo_stack.push(action);
                                    }
                                    _ => {}
                                }
                                for i in 0..app.panes.len() {
                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(i));
                                }
                            }
                            return true;
                        }
                        KeyCode::Char('f') if has_control => {
                            app.mode = AppMode::Search;
                            return true;
                        }
                        KeyCode::Char(' ') => {
                            if let Some(fs) = app.current_file_state_mut() {
                                let idx_opt = fs.selected_index.or_else(|| {
                                    if !fs.files.is_empty() {
                                        fs.selected_index = Some(0);
                                        fs.table_state.select(Some(0));
                                        Some(0)
                                    } else {
                                        None
                                    }
                                });

                                if let Some(idx) = idx_opt {
                                    if let Some(path) = fs.files.get(idx).cloned() {
                                        if path.is_dir() {
                                            app.mode = AppMode::Properties;
                                        } else {
                                            let target_pane =
                                                if app.focused_pane_index == 0 { 1 } else { 0 };
                                            let _ = event_tx.try_send(AppEvent::PreviewRequested(
                                                target_pane,
                                                path,
                                            ));
                                        }
                                    }
                                }
                            }
                            return true;
                        }
                        KeyCode::Up => {
                            if app.sidebar_focus {
                                if has_alt {
                                    // Reorder favorites
                                    let target_opt = app
                                        .sidebar_bounds
                                        .iter()
                                        .find(|b| b.index == app.sidebar_index)
                                        .map(|b| b.target.clone());

                                    if let Some(SidebarTarget::Favorite(path)) = target_opt {
                                        if let Some(pos) =
                                            app.starred.iter().position(|p| *p == path)
                                        {
                                            if pos > 0 {
                                                app.starred.swap(pos, pos - 1);
                                                app.sidebar_index -= 1;
                                                let _ = crate::config::save_state(app);
                                            }
                                        }
                                    }
                                } else {
                                    if app.sidebar_index == 0 {
                                        app.mode = AppMode::Header(0); // Go to Burger icon
                                    } else {
                                        app.sidebar_index = app.sidebar_index.saturating_sub(1);
                                    }
                                }
                            } else if let Some(fs) = app.current_file_state_mut() {
                                if let Some(sel) = fs.selected_index {
                                    if sel > 0 {
                                        let next = sel - 1;
                                        if fs.files[next].to_string_lossy() == "__DIVIDER__" {
                                            if next > 0 {
                                                fs.selected_index = Some(next - 1);
                                                fs.table_state.select(Some(next - 1));
                                            }
                                        } else {
                                            fs.selected_index = Some(next);
                                            fs.table_state.select(Some(next));
                                        }
                                        
                                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                                            let anchor = fs.selection_anchor.unwrap_or(sel);
                                            fs.selection_anchor = Some(anchor);
                                            fs.multi_select.clear();
                                            let current = fs.selected_index.unwrap_or(0);
                                            for i in std::cmp::min(anchor, current)..=std::cmp::max(anchor, current) {
                                                fs.multi_select.insert(i);
                                            }
                                        } else {
                                            fs.multi_select.clear();
                                            fs.selection_anchor = Some(fs.selected_index.unwrap());
                                        }
                                    } else {
                                        // At top of list, go to Header (Tab 1 of current pane)
                                        let mut tab_offset = 5;
                                        for i in 0..app.focused_pane_index {
                                            tab_offset += app.panes[i].tabs.len();
                                        }
                                        let current_tab_idx = app.panes[app.focused_pane_index].active_tab_index;
                                        app.mode = AppMode::Header(tab_offset + current_tab_idx);
                                    }
                                } else {
                                    // Empty folder or nothing selected
                                    app.mode = AppMode::Header(5);
                                }
                            }
                            return true;
                        }
                        KeyCode::Down => {
                            if app.sidebar_focus {
                                if has_alt {
                                    // Reorder favorites
                                    let target_opt = app
                                        .sidebar_bounds
                                        .iter()
                                        .find(|b| b.index == app.sidebar_index)
                                        .map(|b| b.target.clone());

                                    if let Some(SidebarTarget::Favorite(path)) = target_opt {
                                        if let Some(pos) =
                                            app.starred.iter().position(|p| *p == path)
                                        {
                                            if pos < app.starred.len() - 1 {
                                                app.starred.swap(pos, pos + 1);
                                                app.sidebar_index += 1;
                                                let _ = crate::config::save_state(app);
                                            }
                                        }
                                    }
                                } else {
                                    // Clamp to max sidebar items (needs tracking or safe bound)
                                    app.sidebar_index += 1;
                                }
                            } else if let Some(fs) = app.current_file_state_mut() {
                                if let Some(sel) = fs.selected_index {
                                    if sel < fs.files.len().saturating_sub(1) {
                                        let next = sel + 1;
                                        if fs.files[next].to_string_lossy() == "__DIVIDER__" {
                                            if next + 1 < fs.files.len() {
                                                fs.selected_index = Some(next + 1);
                                                fs.table_state.select(Some(next + 1));
                                            }
                                        } else {
                                            fs.selected_index = Some(next);
                                            fs.table_state.select(Some(next));
                                        }

                                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                                            let anchor = fs.selection_anchor.unwrap_or(sel);
                                            fs.selection_anchor = Some(anchor);
                                            fs.multi_select.clear();
                                            let current = fs.selected_index.unwrap_or(0);
                                            for i in std::cmp::min(anchor, current)..=std::cmp::max(anchor, current) {
                                                fs.multi_select.insert(i);
                                            }
                                        } else {
                                            fs.multi_select.clear();
                                            fs.selection_anchor = Some(fs.selected_index.unwrap());
                                        }
                                    }
                                } else {
                                    fs.selected_index = Some(0);
                                    fs.table_state.select(Some(0));
                                    fs.selection_anchor = Some(0);
                                }
                            }
                            return true;
                        }
                        KeyCode::Left => {
                            if key.modifiers.contains(KeyModifiers::SHIFT) && !app.sidebar_focus {
                                let other_pane_idx = if app.focused_pane_index == 0 { 1 } else { 0 };
                                if let Some(dest_path) = app.panes.get(other_pane_idx).and_then(|p| p.current_state()).map(|fs| fs.current_path.clone()) {
                                    if let Some(fs) = app.current_file_state() {
                                        let mut paths = Vec::new();
                                        if !fs.multi_select.is_empty() {
                                            for &idx in &fs.multi_select { if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); } }
                                        } else if let Some(idx) = fs.selected_index {
                                            if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); }
                                        }
                                        for p in paths {
                                            let dest = dest_path.join(p.file_name().unwrap());
                                            let _ = event_tx.try_send(AppEvent::Copy(p, dest));
                                        }
                                    }
                                }
                                return true;
                            }
                            if app.panes.len() > 1 && app.focused_pane_index > 0 {
                                app.focused_pane_index -= 1;
                            } else {
                                app.sidebar_focus = true;
                            }
                            return true;
                        }
                        KeyCode::Right => {
                            if key.modifiers.contains(KeyModifiers::SHIFT) && !app.sidebar_focus {
                                let other_pane_idx = if app.focused_pane_index == 0 { 1 } else { 0 };
                                if let Some(dest_path) = app.panes.get(other_pane_idx).and_then(|p| p.current_state()).map(|fs| fs.current_path.clone()) {
                                    if let Some(fs) = app.current_file_state() {
                                        let mut paths = Vec::new();
                                        if !fs.multi_select.is_empty() {
                                            for &idx in &fs.multi_select { if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); } }
                                        } else if let Some(idx) = fs.selected_index {
                                            if let Some(p) = fs.files.get(idx) { paths.push(p.clone()); }
                                        }
                                        for p in paths {
                                            let dest = dest_path.join(p.file_name().unwrap());
                                            let _ = event_tx.try_send(AppEvent::Copy(p, dest));
                                        }
                                    }
                                }
                                return true;
                            }
                            if app.sidebar_focus {
                                app.sidebar_focus = false;
                            } else if app.panes.len() > 1
                                && app.focused_pane_index < app.panes.len() - 1
                            {
                                app.focused_pane_index += 1;
                            }
                            return true;
                        }
                        KeyCode::Enter => {
                            if app.sidebar_focus {
                                let target_opt = app
                                    .sidebar_bounds
                                    .iter()
                                    .find(|b| b.index == app.sidebar_index)
                                    .map(|b| b.target.clone());

                                if let Some(target) = target_opt {
                                    match target {
                                        crate::app::SidebarTarget::Favorite(path) => {
                                            if let Some(fs) = app.current_file_state_mut() {
                                                fs.current_path = path.clone();
                                                fs.selected_index = Some(0);
                                                crate::event_helpers::push_history(
                                                    fs,
                                                    path.clone(),
                                                );
                                                let _ = event_tx.try_send(AppEvent::RefreshFiles(
                                                    app.focused_pane_index,
                                                ));
                                                app.sidebar_focus = false;
                                            }
                                        }
                                        crate::app::SidebarTarget::Remote(idx) => {
                                            let _ = event_tx.try_send(AppEvent::ConnectToRemote(
                                                app.focused_pane_index,
                                                idx,
                                            ));
                                        }
                                        crate::app::SidebarTarget::Disk(name) => {
                                            if let Some(disk) = app
                                                .system_state
                                                .disks
                                                .iter()
                                                .find(|d| d.name == name)
                                            {
                                                if disk.is_mounted {
                                                    let mp = PathBuf::from(&disk.name);
                                                    if let Some(fs) = app.current_file_state_mut() {
                                                        fs.current_path = mp.clone();
                                                        fs.selected_index = Some(0);
                                                        crate::event_helpers::push_history(
                                                            fs,
                                                            mp.clone(),
                                                        );
                                                        let _ = event_tx.try_send(
                                                            AppEvent::RefreshFiles(
                                                                app.focused_pane_index,
                                                            ),
                                                        );
                                                        app.sidebar_focus = false;
                                                    }
                                                } else {
                                                    let _ = event_tx.try_send(AppEvent::MountDisk(
                                                        name.clone(),
                                                    ));
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                return true;
                            }
                            let mut navigate_to = None;
                            if let Some(fs) = app.current_file_state() {
                                if let Some(idx) = fs.selected_index {
                                    if let Some(path) = fs.files.get(idx) {
                                        if path.is_dir() {
                                            navigate_to = Some(path.clone());
                                        } else {
                                            terma::utils::spawn_detached(
                                                "xdg-open",
                                                vec![path.to_string_lossy().to_string()],
                                            );
                                        }
                                    }
                                }
                            }
                            if let Some(p) = navigate_to {
                                if let Some(fs) = app.current_file_state_mut() {
                                    fs.current_path = p.clone();
                                    fs.selected_index = Some(0);
                                    fs.multi_select.clear();
                                    fs.search_filter.clear();
                                    *fs.table_state.offset_mut() = 0;
                                    crate::event_helpers::push_history(fs, p);
                                    let _ = event_tx
                                        .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                }
                            }
                            return true;
                        }
                        KeyCode::F(6) => {
                            let mut to_rename = None;
                            if let Some(fs) = app.current_file_state() {
                                if let Some(p) = fs.selected_index.and_then(|idx| fs.files.get(idx))
                                {
                                    to_rename =
                                        Some(p.file_name().unwrap().to_string_lossy().to_string());
                                }
                            }
                            if let Some(name) = to_rename {
                                app.mode = AppMode::Rename;
                                app.input.set_value(name.clone());
                                if let Some(idx) = name.rfind('.') {
                                    if idx > 0 {
                                        app.input.cursor_position = idx;
                                    } else {
                                        app.input.cursor_position = name.len();
                                    }
                                } else {
                                    app.input.cursor_position = name.len();
                                }
                                app.rename_selected = true;
                                return true;
                            }
                            return false;
                        }
                        KeyCode::Delete => {
                            if let Some(fs) = app.current_file_state() {
                                if fs.selected_index.is_some() {
                                    if app.confirm_delete {
                                        app.mode = AppMode::Delete;
                                    } else {
                                        let mut paths = Vec::new();
                                        if !fs.multi_select.is_empty() {
                                            for &idx in &fs.multi_select {
                                                if let Some(p) = fs.files.get(idx) {
                                                    paths.push(p.clone());
                                                }
                                            }
                                        } else if let Some(idx) = fs.selected_index {
                                            if let Some(p) = fs.files.get(idx) {
                                                paths.push(p.clone());
                                            }
                                        }
                                        for p in paths {
                                            let _ = event_tx.try_send(AppEvent::Delete(p));
                                        }
                                    }
                                    return true;
                                }
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
                                    crate::event_helpers::push_history(fs, home);
                                    let _ = event_tx
                                        .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                    return true;
                                }
                            }
                            return false;
                        }
                        KeyCode::Char(c) if key.modifiers.is_empty() => {
                            if (c as u32) < 32 || c == '\x7f' {
                                return false;
                            }
                            if app.current_view == CurrentView::Processes {
                                app.process_search_filter.push(c);
                                app.process_selected_idx = Some(0);
                                *app.process_table_state.offset_mut() = 0;
                                return true;
                            }
                            if let Some(fs) = app.current_file_state_mut() {
                                fs.search_filter.push(c);
                                fs.selected_index = Some(0);
                                *fs.table_state.offset_mut() = 0;
                                let _ = event_tx
                                    .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                            }
                            return true;
                        }
                        KeyCode::Backspace if !has_control => {
                            if app.current_view == CurrentView::Processes {
                                if !app.process_search_filter.is_empty() {
                                    app.process_search_filter.pop();
                                    app.process_selected_idx = Some(0);
                                    *app.process_table_state.offset_mut() = 0;
                                }
                                return true;
                            }
                            if let Some(fs) = app.current_file_state_mut() {
                                if !fs.search_filter.is_empty() {
                                    fs.search_filter.pop();
                                    fs.selected_index = Some(0);
                                    *fs.table_state.offset_mut() = 0;
                                    let _ = event_tx
                                        .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                } else if let Some(parent) = fs.current_path.parent() {
                                    let p = parent.to_path_buf();
                                    fs.current_path = p.clone();
                                    fs.selected_index = Some(0);
                                    fs.multi_select.clear();
                                    *fs.table_state.offset_mut() = 0;
                                    crate::event_helpers::push_history(fs, p);
                                    let _ = event_tx
                                        .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                }
                            }
                            return true;
                        }
                        // Ctrl+Backspace / Alt+Backspace / Ctrl+W / Ctrl+U
                        KeyCode::Backspace if has_control || has_alt => {
                            if app.current_view == CurrentView::Processes {
                                delete_word_backwards(&mut app.process_search_filter);
                                app.process_selected_idx = Some(0);
                                *app.process_table_state.offset_mut() = 0;
                            } else if let Some(fs) = app.current_file_state_mut() {
                                delete_word_backwards(&mut fs.search_filter);
                                fs.selected_index = Some(0);
                                *fs.table_state.offset_mut() = 0;
                                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                            }
                            return true;
                        }
                        KeyCode::Char('w') if has_control => {
                            if app.current_view == CurrentView::Processes {
                                delete_word_backwards(&mut app.process_search_filter);
                                app.process_selected_idx = Some(0);
                                *app.process_table_state.offset_mut() = 0;
                            } else if let Some(fs) = app.current_file_state_mut() {
                                delete_word_backwards(&mut fs.search_filter);
                                fs.selected_index = Some(0);
                                *fs.table_state.offset_mut() = 0;
                                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                            }
                            return true;
                        }
                        KeyCode::Char('u') if has_control => {
                            if app.current_view == CurrentView::Processes {
                                app.process_search_filter.clear();
                                app.process_selected_idx = Some(0);
                                *app.process_table_state.offset_mut() = 0;
                            } else if let Some(fs) = app.current_file_state_mut() {
                                fs.search_filter.clear();
                                fs.selected_index = Some(0);
                                *fs.table_state.offset_mut() = 0;
                                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                            }
                            return true;
                        }
                        _ => return false,
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
                AppMode::Highlight => {
                    if let MouseEventKind::Down(_) = me.kind {
                        let area_w = 34;
                        let area_h = 5;
                        let area_x = (w.saturating_sub(area_w)) / 2;
                        let area_y = (h.saturating_sub(area_h)) / 2;
                        if column >= area_x
                            && column < area_x + area_w
                            && row >= area_y
                            && row < area_y + area_h
                        {
                            let rel_x = column.saturating_sub(area_x + 3);
                            if row >= area_y + 2 && row <= area_y + 3 {
                                let colors = [1, 2, 3, 4, 5, 6, 0];
                                if let Some(&color_code) = colors.get((rel_x / 4) as usize) {
                                    let color = if color_code == 0 {
                                        None
                                    } else {
                                        Some(color_code as u8)
                                    };
                                    if let Some(fs) = app.current_file_state() {
                                        let mut paths = Vec::new();
                                        if !fs.multi_select.is_empty() {
                                            for &idx in &fs.multi_select {
                                                if let Some(p) = fs.files.get(idx) {
                                                    paths.push(p.clone());
                                                }
                                            }
                                        } else if let Some(idx) = fs.selected_index {
                                            if let Some(p) = fs.files.get(idx) {
                                                paths.push(p.clone());
                                            }
                                        }
                                        for p in paths {
                                            if let Some(col) = color {
                                                app.path_colors.insert(p, col);
                                            } else {
                                                app.path_colors.remove(&p);
                                            }
                                        }
                                        let _ = crate::config::save_state(app);
                                    }
                                    app.mode = AppMode::Normal;
                                }
                            }
                        } else {
                            app.mode = AppMode::Normal;
                        }
                        return true;
                    }
                    if let MouseEventKind::Moved | MouseEventKind::Drag(_) = me.kind {
                        return true; // Trap movement
                    }
                }
                AppMode::ContextMenu {
                    x,
                    y,
                    ref actions,
                    ref target,
                    selected_index: _,
                } => {
                    let (mw, mh) = (25, actions.len() as u16 + 2);
                    let (mut dx, mut dy) = (x, y);
                    if dx + mw > w {
                        dx = w.saturating_sub(mw);
                    }
                    if dy + mh > h {
                        dy = h.saturating_sub(mh);
                    }

                    if let MouseEventKind::Down(_) = me.kind {
                        if column >= dx && column < dx + mw && row >= dy && row < dy + mh {
                            if row > dy && row < dy + mh - 1 {
                                if let Some(action) = actions.get((row - dy - 1) as usize) {
                                    if *action != ContextMenuAction::Separator {
                                        crate::event_helpers::handle_context_menu_action(
                                            action,
                                            target,
                                            app,
                                            event_tx.clone(),
                                        );
                                        if let AppMode::ContextMenu { .. } = app.mode {
                                            app.mode = AppMode::Normal;
                                        }
                                    }
                                }
                            }
                        } else {
                            app.mode = AppMode::Normal;
                        }
                        return true;
                    }
                    if let MouseEventKind::Moved | MouseEventKind::Drag(_) = me.kind {
                        if column >= dx && column < dx + mw && row >= dy && row < dy + mh {
                            if row > dy && row < dy + mh - 1 {
                                let idx = (row - dy - 1) as usize;
                                if idx < actions.len() {
                                    let new_idx = if actions[idx] != ContextMenuAction::Separator {
                                        Some(idx)
                                    } else {
                                        None
                                    };
                                    if let AppMode::ContextMenu { selected_index: ref mut si, .. } = app.mode {
                                        *si = new_idx;
                                    }
                                }
                            } else {
                                if let AppMode::ContextMenu { selected_index: ref mut si, .. } = app.mode {
                                    *si = None;
                                }
                            }
                        } else {
                            if let AppMode::ContextMenu { selected_index: ref mut si, .. } = app.mode {
                                *si = None;
                            }
                        }
                        return true; // Trap movement
                    }
                }
                AppMode::DragDropMenu { source, target } => {
                    match me.kind {
                        MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                            return true; // Trap movement
                        }
                        MouseEventKind::Down(button) => {
                            let (aw, ah) = ((w as f32 * 0.6) as u16, (h as f32 * 0.2) as u16);
                            let (ax, ay) = ((w - aw) / 2, (h - ah) / 2);
                            let inner_x = ax + 1;
                            let inner_y = ay + 1;
                            if column >= ax && column < ax + aw && row >= ay && row < ay + ah {
                                if button == MouseButton::Left {
                                    let rel_y = row.saturating_sub(inner_y + 3);
                                    if rel_y == 0 {
                                        let rel_x = column.saturating_sub(inner_x);
                                        // Match ui/mod.rs bounds: [C] Copy (0-10) [M] Move (12-22) [Esc] Cancel (24-38)
                                        if rel_x < 10 {
                                            let dest = target.join(source.file_name().unwrap());
                                            let _ = event_tx.try_send(AppEvent::Copy(source.clone(), dest));
                                            app.mode = AppMode::Normal;
                                        } else if rel_x >= 12 && rel_x < 22 {
                                            let dest = target.join(source.file_name().unwrap());
                                            let _ = event_tx.try_send(AppEvent::Rename(source.clone(), dest));
                                            app.mode = AppMode::Normal;
                                        } else if rel_x >= 24 && rel_x < 38 {
                                            app.mode = AppMode::Normal;
                                        }
                                    }
                                }
                            } else {
                                app.mode = AppMode::Normal;
                            }
                            return true;
                        }
                        _ => return true,
                    }
                }
                AppMode::OpenWith(path) => {
                    match me.kind {
                        MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                            return true;
                        }
                        MouseEventKind::Down(_button) => {
                            let (aw, ah) = ((w as f32 * 0.6) as u16, (h as f32 * 0.6) as u16);
                            let (ax, ay) = ((w - aw) / 2, (h - ah) / 2);
                            let inner_y = ay + 1;
                            
                            if column >= ax && column < ax + aw && row >= ay && row < ay + ah {
                                if _button == MouseButton::Left {
                                    // Header(2) + Input(3) = 5. Suggestions start at inner_y + 5
                                    if row >= inner_y + 5 {
                                        let rel_y = row.saturating_sub(inner_y + 5);
                                        
                                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                                        let mut suggestions = terma::utils::get_open_with_suggestions(&ext);
                                        
                                        // Merge with custom tools
                                        if let Some(custom_tools) = app.external_tools.get(&ext) {
                                            for tool in custom_tools {
                                                if !suggestions.contains(&tool.command) {
                                                    suggestions.insert(0, tool.command.clone());
                                                }
                                            }
                                        }

                                        if !app.input.value.is_empty() {
                                            let query = app.input.value.to_lowercase();
                                            suggestions.retain(|s| s.to_lowercase().contains(&query));
                                        }

                                        if let Some(s) = suggestions.get(rel_y as usize) {
                                            let cmd = s.to_string();
                                            
                                            // Persist this choice
                                            let tools = app.external_tools.entry(ext.clone()).or_default();
                                            if !tools.iter().any(|t| t.command == cmd) {
                                                tools.insert(0, crate::config::ExternalTool {
                                                    name: cmd.clone(),
                                                    command: cmd.clone(),
                                                });
                                                let _ = crate::config::save_state(app);
                                            }

                                            let _ = event_tx.try_send(AppEvent::SpawnDetached {
                                                cmd,
                                                args: vec![path.to_string_lossy().to_string()],
                                            });
                                            app.mode = AppMode::Normal;
                                            app.input.clear();
                                            app.open_with_index = 0;
                                        }
                                    }
                                }
                            } else {
                                app.mode = AppMode::Normal;
                                app.input.clear();
                                app.open_with_index = 0;
                            }
                            return true;
                        }
                        _ => return true,
                    }
                }
                AppMode::Settings
                | AppMode::ImportServers
                | AppMode::NewFile
                | AppMode::NewFolder
                | AppMode::Rename
                | AppMode::Delete
                | AppMode::Properties
                | AppMode::CommandPalette
                | AppMode::AddRemote(_)
                | AppMode::Hotkeys
                | AppMode::Editor => {
                    match me.kind {
                        MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                            return true; // Trap movement in modals
                        }
                        MouseEventKind::Down(_button) => {

                            if let AppMode::Editor = app.mode {
                                if let Some(preview) = &mut app.editor_state {
                                    if let Some(editor) = &mut preview.editor {
                                        // Area matches ui/mod.rs inner_area (1 char border)
                                        let editor_area = ratatui::layout::Rect::new(
                                            1,
                                            1,
                                            w.saturating_sub(2),
                                            h.saturating_sub(2),
                                        );
                                        if editor_area.contains(ratatui::layout::Position {
                                            x: column,
                                            y: row,
                                        }) {
                                            editor.handle_mouse_event(me, editor_area);
                                        }
                                    }
                                }
                                return true;
                            }
                            let (aw, ah) = match app.mode {
                                AppMode::Settings => {
                                    ((w as f32 * 0.8) as u16, (h as f32 * 0.8) as u16)
                                }
                                AppMode::Properties => {
                                    ((w as f32 * 0.5) as u16, (h as f32 * 0.5) as u16)
                                }
                                AppMode::CommandPalette
                                | AppMode::AddRemote(_)
                                | AppMode::OpenWith(_) => {
                                    ((w as f32 * 0.6) as u16, (h as f32 * 0.2) as u16)
                                }
                                _ => ((w as f32 * 0.4) as u16, (h as f32 * 0.1) as u16),
                            };
                            if let AppMode::Hotkeys = app.mode {
                                // Hotkeys modal is 70%x80%
                                let (hw, hh) = ((w as f32 * 0.7) as u16, (h as f32 * 0.8) as u16);
                                let (hx, hy) = ((w - hw) / 2, (h - hh) / 2);
                                if column < hx || column >= hx + hw || row < hy || row >= hy + hh {
                                    app.mode = app.previous_mode.clone();
                                    return true;
                                }
                                return true;
                            }
                            let (ax, ay) = ((w - aw) / 2, (h - ah) / 2);
                            if column >= ax && column < ax + aw && row >= ay && row < ay + ah {
                                // ... existing logic ...
                                if let AppMode::Settings = app.mode {
                                    let inner_x = ax + 1;
                                    let inner_y = ay + 1;
                                    if column < inner_x + 15 {
                                        let rel_y = row.saturating_sub(inner_y);
                                        match rel_y {
                                            0 => app.settings_section = SettingsSection::Columns,
                                            1 => app.settings_section = SettingsSection::Tabs,
                                            2 => app.settings_section = SettingsSection::General,
                                            3 => app.settings_section = SettingsSection::Remotes,
                                            4 => app.settings_section = SettingsSection::Shortcuts,
                                            _ => {}
                                        }
                                        app.settings_index = 0;
                                    } else {
                                        match app.settings_section {
                                            SettingsSection::General => {
                                                let rel_y = row.saturating_sub(inner_y + 1);
                                                if rel_y <= 4 {
                                                    app.settings_index = rel_y as usize;
                                                    match rel_y {
                                                        0 => app.default_show_hidden = !app.default_show_hidden,
                                                        1 => app.confirm_delete = !app.confirm_delete,
                                                        2 => app.smart_date = !app.smart_date,
                                                        3 => app.auto_save = !app.auto_save,
                                                        4 => {
                                                            app.icon_mode = match app.icon_mode {
                                                                IconMode::Nerd => IconMode::Unicode,
                                                                IconMode::Unicode => IconMode::ASCII,
                                                                IconMode::ASCII => IconMode::Nerd,
                                                            };
                                                        }
                                                        _ => {}
                                                    }
                                                    let _ = crate::config::save_state(app);
                                                }
                                            }
                                            SettingsSection::Columns => {
                                                if row >= inner_y && row < inner_y + 3 {
                                                    let cx = column.saturating_sub(inner_x + 15);
                                                    if cx < 12 {
                                                        app.settings_target = SettingsTarget::SingleMode;
                                                    } else if cx < 25 {
                                                        app.settings_target = SettingsTarget::SplitMode;
                                                    }
                                                } else if row >= inner_y + 4 {
                                                    let ry = row.saturating_sub(inner_y + 4);
                                                    if ry <= 3 {
                                                        app.settings_index = ry as usize;
                                                        match ry {
                                                            0 => app.toggle_column(crate::app::FileColumn::Size),
                                                            1 => app.toggle_column(crate::app::FileColumn::Modified),
                                                            2 => app.toggle_column(crate::app::FileColumn::Created),
                                                            3 => app.toggle_column(crate::app::FileColumn::Permissions),
                                                            _ => {}
                                                        }
                                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            } else {
                                app.mode = AppMode::Normal;
                                app.input.clear();
                            }
                            return true;
                        }
                        MouseEventKind::ScrollUp => {
                            if let AppMode::Editor = app.mode {
                                if let Some(preview) = &mut app.editor_state {
                                    if let Some(editor) = &mut preview.editor {
                                        editor.handle_mouse_event(
                                            me,
                                            ratatui::layout::Rect::new(
                                                0,
                                                0,
                                                w,
                                                h.saturating_sub(1),
                                            ),
                                        );
                                    }
                                }
                            } else if let AppMode::Settings = app.mode {
                                app.settings_scroll = app.settings_scroll.saturating_sub(2);
                            }
                        }
                        MouseEventKind::ScrollDown => {
                            if let AppMode::Editor = app.mode {
                                if let Some(preview) = &mut app.editor_state {
                                    if let Some(editor) = &mut preview.editor {
                                        editor.handle_mouse_event(
                                            me,
                                            ratatui::layout::Rect::new(
                                                0,
                                                0,
                                                w,
                                                h.saturating_sub(1),
                                            ),
                                        );
                                    }
                                }
                            } else if let AppMode::Settings = app.mode {
                                app.settings_scroll = app.settings_scroll.saturating_add(2);
                            }
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
                    
                    if button == MouseButton::Left && column >= sw.saturating_sub(1) && column <= sw + 1 {
                        app.is_resizing_sidebar = true;
                        return true;
                    } else {
                        app.is_resizing_sidebar = false;
                    }

                    // Header Icons
                    if row == 0 {
                        if let Some((_, action_id)) = app
                            .header_icon_bounds
                            .iter()
                            .find(|(r, _)| column >= r.x && column < r.x + r.width && row == r.y)
                        {
                            match action_id.as_str() {
                                "back" => {
                                    if let Some(fs) = app.current_file_state_mut() {
                                        crate::event_helpers::navigate_back(fs);
                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(
                                            app.focused_pane_index,
                                        ));
                                    }
                                }
                                "forward" => {
                                    if let Some(fs) = app.current_file_state_mut() {
                                        crate::event_helpers::navigate_forward(fs);
                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(
                                            app.focused_pane_index,
                                        ));
                                    }
                                }
                                "split" => {
                                    app.toggle_split();
                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                                }
                                "burger" => {
                                    app.mode = AppMode::Settings;
                                    app.settings_scroll = 0;
                                }
                                "monitor" => {
                                    let _ = event_tx.try_send(AppEvent::StatusMsg(
                                        "Launching System Monitor...".to_string(),
                                    ));
                                    let _ = event_tx.try_send(AppEvent::SystemMonitor);
                                }
                                _ => {}
                            }
                            app.sidebar_focus = false;
                            return true;
                        }
                    }

                    // Tabs
                    if row == 0 {
                        if let Some((_, p_idx, t_idx)) = app
                            .tab_bounds
                            .iter()
                            .find(|(r, _, _)| {
                                r.contains(ratatui::layout::Position { x: column, y: row })
                            })
                            .cloned()
                        {
                            if button == MouseButton::Left {
                                if let Some(p) = app.panes.get_mut(p_idx) {
                                    p.active_tab_index = t_idx;
                                    app.focused_pane_index = p_idx;
                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(p_idx));
                                }
                            } else if button == MouseButton::Right {
                                if let Some(p) = app.panes.get_mut(p_idx) {
                                    if p.tabs.len() > 1 {
                                        p.tabs.remove(t_idx);
                                        if p.active_tab_index >= p.tabs.len() {
                                            p.active_tab_index = p.tabs.len() - 1;
                                        }
                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(p_idx));
                                    }
                                }
                            }
                            app.sidebar_focus = false;
                            return true;
                        }
                    }

                    // Breadcrumbs
                    for (p_idx, pane) in app.panes.iter_mut().enumerate() {
                        if let Some(fs) = pane.current_state_mut() {
                            if let Some(path) = fs
                                .breadcrumb_bounds
                                .iter()
                                .find(|(r, _)| {
                                    r.contains(ratatui::layout::Position { x: column, y: row })
                                })
                                .map(|(_, p)| p.clone())
                            {
                                if button == MouseButton::Middle {
                                    let mut nfs = fs.clone();
                                    nfs.current_path = path.clone();
                                    nfs.selected_index = Some(0);
                                    nfs.search_filter.clear();
                                    *nfs.table_state.offset_mut() = 0;
                                    nfs.history = vec![path];
                                    nfs.history_index = 0;
                                    pane.open_tab(nfs);
                                } else {
                                    fs.current_path = path.clone();
                                    fs.selected_index = Some(0);
                                    fs.multi_select.clear();
                                    fs.search_filter.clear();
                                    *fs.table_state.offset_mut() = 0;
                                    crate::event_helpers::push_history(fs, path);
                                }
                                let _ = event_tx.try_send(AppEvent::RefreshFiles(p_idx));
                                app.focused_pane_index = p_idx;
                                app.sidebar_focus = false;
                                return true;
                            }
                        }
                    }

                    // Pane focus & Sorting
                    if column >= sw {
                        let cw = w.saturating_sub(sw);
                        let pc = app.panes.len();
                        let pw = if pc > 0 { cw / pc as u16 } else { cw };
                        let cp = (column.saturating_sub(sw) / pw) as usize;
                        if cp < pc {
                            if row == 1 || row == 2 {
                                if let Some(fs) =
                                    app.panes.get_mut(cp).and_then(|p| p.current_state_mut())
                                {
                                    for (r, col) in &fs.column_bounds {
                                        if column >= r.x && column < r.x + r.width + 1 {
                                            if fs.sort_column == *col {
                                                fs.sort_ascending = !fs.sort_ascending;
                                            } else {
                                                fs.sort_column = *col;
                                                fs.sort_ascending = true;
                                            }
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(cp));
                                            return true;
                                        }
                                    }
                                }
                            }
                            app.focused_pane_index = cp;
                            app.sidebar_focus = false;
                        }
                    }

                    // Monitor Subviews
                    if app.current_view == CurrentView::Processes {
                        // Tab Clicks
                        if let Some((_, subview)) = app
                            .monitor_subview_bounds
                            .iter()
                            .find(|(r, _)| column >= r.x && column < r.x + r.width && row == r.y)
                        {
                            app.monitor_subview = *subview;
                            return true;
                        }

                        // Column Sorting Clicks
                        if app.monitor_subview == MonitorSubview::Processes
                            || app.monitor_subview == MonitorSubview::Applications
                        {
                            if let Some((_, col)) =
                                app.process_column_bounds.iter().find(|(r, _)| {
                                    column >= r.x && column < r.x + r.width && row == r.y
                                })
                            {
                                if app.process_sort_col == *col {
                                    app.process_sort_asc = !app.process_sort_asc;
                                } else {
                                    app.process_sort_col = *col;
                                    app.process_sort_asc = true;
                                }
                                return true;
                            }
                        }

                        // Selection Clicks (Approximate based on row)
                        // Content area typically starts at y=6 (3 nav + 1 margin + 1 header + 1 margin)
                        if row >= 6 {
                            let scroll_offset = app.process_table_state.offset();
                            let rel_row = (row - 6) as usize + scroll_offset;
                            if app.monitor_subview == MonitorSubview::Processes
                                || app.monitor_subview == MonitorSubview::Applications
                            {
                                app.process_selected_idx = Some(rel_row);
                                app.process_table_state.select(Some(rel_row));
                                return true;
                            }
                        }
                    }

                    if column < sw {
                        app.sidebar_focus = true;
                        app.drag_start_pos = Some((column, row));
                        if let Some(b) = app.sidebar_bounds.iter().find(|b| b.y == row).cloned() {
                            app.sidebar_index = b.index;
                            if button == MouseButton::Left {
                                match &b.target {
                                    SidebarTarget::Favorite(path) => {
                                        if let Some(fs) = app.current_file_state_mut() {
                                            fs.current_path = path.clone();
                                            fs.selected_index = Some(0);
                                            crate::event_helpers::push_history(fs, path.clone());
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(
                                                app.focused_pane_index,
                                            ));
                                        }
                                    }
                                    SidebarTarget::Remote(idx) => {
                                        let _ = event_tx.try_send(AppEvent::ConnectToRemote(
                                            app.focused_pane_index,
                                            *idx,
                                        ));
                                    }
                                    SidebarTarget::Disk(name) => {
                                        if let Some(disk) =
                                            app.system_state.disks.iter().find(|d| d.name == *name)
                                        {
                                            if disk.is_mounted {
                                                let mp = PathBuf::from(&disk.name);
                                                if let Some(fs) = app.current_file_state_mut() {
                                                    fs.current_path = mp.clone();
                                                    fs.selected_index = Some(0);
                                                    crate::event_helpers::push_history(
                                                        fs,
                                                        mp.clone(),
                                                    );
                                                    let _ =
                                                        event_tx.try_send(AppEvent::RefreshFiles(
                                                            app.focused_pane_index,
                                                        ));
                                                }
                                            } else {
                                                let _ = event_tx
                                                    .try_send(AppEvent::MountDisk(name.clone()));
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            if let SidebarTarget::Favorite(ref p) = b.target {
                                app.drag_source = Some(p.clone());
                            }
                            if button == MouseButton::Right {
                                let t = match &b.target {
                                    SidebarTarget::Favorite(p) => {
                                        Some(ContextMenuTarget::SidebarFavorite(p.clone()))
                                    }
                                    SidebarTarget::Remote(i) => {
                                        Some(ContextMenuTarget::SidebarRemote(*i))
                                    }
                                    SidebarTarget::Storage(i) => {
                                        Some(ContextMenuTarget::SidebarStorage(*i))
                                    }
                                    _ => None,
                                };
                                if let Some(target) = t {
                                    let actions = crate::event_helpers::get_context_menu_actions(
                                        &target, app,
                                    );
                                    app.mode = AppMode::ContextMenu {
                                        x: column,
                                        y: row,
                                        target,
                                        actions,
                                        selected_index: None,
                                    };
                                }
                            }
                        }
                        return true;
                    }

                    if column >= sw {
                        app.sidebar_focus = false;
                    }

                    if row >= 3 {
                        // Breadcrumb Click Check
                        if let Some(fs) = app.current_file_state_mut() {
                            if let Some((_, path)) = fs.breadcrumb_bounds.iter().find(|(r, _)| {
                                r.contains(ratatui::layout::Position { x: column, y: row })
                            }) {
                                let p = path.clone();
                                fs.current_path = p.clone();
                                fs.selected_index = Some(0);
                                fs.search_filter.clear();
                                *fs.table_state.offset_mut() = 0;
                                crate::event_helpers::push_history(fs, p);
                                let _ = event_tx
                                    .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                return true;
                            }
                        }

                        let idx = crate::event_helpers::fs_mouse_index(row, app);
                        let mut sp = None;
                        let mut is_dir = false;
                        let has_mods = me.modifiers.contains(KeyModifiers::SHIFT)
                            || me.modifiers.contains(KeyModifiers::CONTROL);

                        if let Some(fs) = app.current_file_state_mut() {
                            if idx < fs.files.len() {
                                if fs.files[idx].to_string_lossy() == "__DIVIDER__" {
                                    // Click divider to jump to the first global result
                                    if idx + 1 < fs.files.len() {
                                        fs.selected_index = Some(idx + 1);
                                        fs.table_state.select(Some(idx + 1));
                                    }
                                    return true;
                                }
                                if button == MouseButton::Left {
                                    if me.modifiers.contains(KeyModifiers::CONTROL) {
                                        if fs.multi_select.contains(&idx) {
                                            fs.multi_select.remove(&idx);
                                        } else {
                                            fs.multi_select.insert(idx);
                                        }
                                        fs.selected_index = Some(idx);
                                        fs.table_state.select(Some(idx));
                                    } else if me.modifiers.contains(KeyModifiers::SHIFT) {
                                        let anchor = fs
                                            .selection_anchor
                                            .unwrap_or(fs.selected_index.unwrap_or(0));
                                        fs.multi_select.clear();
                                        for i in
                                            std::cmp::min(anchor, idx)..=std::cmp::max(anchor, idx)
                                        {
                                            fs.multi_select.insert(i);
                                        }
                                        fs.selected_index = Some(idx);
                                        fs.table_state.select(Some(idx));
                                    } else {
                                        fs.multi_select.clear();
                                        fs.selection_anchor = Some(idx);
                                        fs.selected_index = Some(idx);
                                        fs.table_state.select(Some(idx));
                                    }
                                } else if !fs.multi_select.contains(&idx) {
                                    fs.multi_select.clear();
                                    fs.selected_index = Some(idx);
                                    fs.table_state.select(Some(idx));
                                }
                                let p = fs.files[idx].clone();
                                is_dir = fs.metadata.get(&p).map(|m| m.is_dir).unwrap_or(false);
                                sp = Some(p);
                            } else if button == MouseButton::Left && !has_mods {
                                fs.selected_index = None;
                                fs.table_state.select(None);
                                fs.multi_select.clear();
                                fs.selection_anchor = None;
                            } else if button == MouseButton::Right {
                                let target = ContextMenuTarget::EmptySpace;
                                let actions =
                                    crate::event_helpers::get_context_menu_actions(&target, app);
                                app.mode = AppMode::ContextMenu {
                                    x: column,
                                    y: row,
                                    target,
                                    actions,
                                    selected_index: None,
                                };
                                return true;
                            }
                        }
                        if let Some(path) = sp {
                            if button == MouseButton::Right {
                                let target = if is_dir {
                                    ContextMenuTarget::Folder(idx)
                                } else {
                                    ContextMenuTarget::File(idx)
                                };
                                let actions =
                                    crate::event_helpers::get_context_menu_actions(&target, app);
                                app.mode = AppMode::ContextMenu {
                                    x: column,
                                    y: row,
                                    target,
                                    actions,
                                    selected_index: None,
                                };
                                return true;
                            }
                            if button == MouseButton::Middle {
                                if is_dir {
                                    if let Some(p) = app.panes.get_mut(app.focused_pane_index) {
                                        if let Some(fs) = p.current_state() {
                                            let mut nfs = fs.clone();
                                            nfs.current_path = path.clone();
                                            nfs.selected_index = Some(0);
                                            nfs.search_filter.clear();
                                            *nfs.table_state.offset_mut() = 0;
                                            nfs.history = vec![path.clone()];
                                            nfs.history_index = 0;
                                            p.open_tab(nfs);
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(
                                                app.focused_pane_index,
                                            ));
                                        }
                                    }
                                } else {
                                    let _ = event_tx.try_send(AppEvent::PreviewRequested(
                                        if app.focused_pane_index == 0 { 1 } else { 0 },
                                        path.clone(),
                                    ));
                                }
                                return true;
                            }
                            app.drag_source = Some(path.clone());
                            app.drag_start_pos = Some((column, row));
                            if button == MouseButton::Left
                                && app.mouse_last_click.elapsed() < Duration::from_millis(500)
                                && app.mouse_click_pos == (column, row)
                            {
                                if path.is_dir() {
                                    if let Some(fs) = app.current_file_state_mut() {
                                        fs.current_path = path.clone();
                                        fs.selected_index = Some(0);
                                        fs.multi_select.clear();
                                        fs.search_filter.clear();
                                        *fs.table_state.offset_mut() = 0;
                                        crate::event_helpers::push_history(fs, path);
                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(
                                            app.focused_pane_index,
                                        ));
                                    }
                                } else {
                                    terma::utils::spawn_detached(
                                        "xdg-open",
                                        vec![path.to_string_lossy().to_string()],
                                    );
                                }
                            }
                            app.mouse_last_click = std::time::Instant::now();
                            app.mouse_click_pos = (column, row);
                        }
                    } else if row == h.saturating_sub(1) && column < 9 {
                        app.running = false;
                        return true;
                    }
                    return true;
                }
                MouseEventKind::Up(_) => {
                    if app.is_resizing_sidebar {
                        app.is_resizing_sidebar = false;
                        let _ = crate::config::save_state(app);
                        return true;
                    }
                    if app.is_dragging {
                        if let Some((source, target)) =
                            app.drag_source.take().zip(app.hovered_drop_target.take())
                        {
                            match target {
                                DropTarget::ImportServers | DropTarget::RemotesHeader => {
                                    if source.extension().map(|e| e == "toml").unwrap_or(false) {
                                        let _ = app.import_servers(source);
                                        let _ = crate::config::save_state(app);
                                    }
                                }
                                DropTarget::Favorites => {
                                    if source.is_dir() && !app.starred.contains(&source) {
                                        app.starred.push(source);
                                        let _ = crate::config::save_state(app);
                                    }
                                }
                                DropTarget::Pane(t_idx) => {
                                    if let Some(dest_dir) =
                                        app.panes.get(t_idx).and_then(|p| p.current_state()).map(
                                            |fs| fs.current_path.clone(),
                                        )
                                    {
                                        app.mode = AppMode::DragDropMenu { source, target: dest_dir };
                                    }
                                }
                                DropTarget::Folder(target_path) => {
                                    if target_path != source {
                                        app.mode = AppMode::DragDropMenu { source, target: target_path };
                                    }
                                }
                                DropTarget::ReorderFavorite(target_sidebar_idx) => {
                                    if let Some(SidebarTarget::Favorite(src_path)) = app
                                        .sidebar_bounds
                                        .iter()
                                        .find(|b| {
                                            if let SidebarTarget::Favorite(p) = &b.target {
                                                *p == source
                                            } else {
                                                false
                                            }
                                        })
                                        .map(|b| b.target.clone())
                                    {
                                        if let Some(SidebarTarget::Favorite(dest_path)) = app
                                            .sidebar_bounds
                                            .iter()
                                            .find(|b| b.index == target_sidebar_idx)
                                            .map(|b| b.target.clone())
                                        {
                                            let src_pos =
                                                app.starred.iter().position(|p| *p == src_path);
                                            let dest_pos =
                                                app.starred.iter().position(|p| *p == dest_path);

                                            if let (Some(s), Some(d)) = (src_pos, dest_pos) {
                                                let item = app.starred.remove(s);
                                                app.starred.insert(d, item);
                                                let _ = crate::config::save_state(app);
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    } else if column < app.sidebar_width() {
                        if let Some(b) = app.sidebar_bounds.iter().find(|b| b.y == row) {
                            match &b.target {
                                SidebarTarget::Header(h) if h == "REMOTES" => {
                                    app.mode = AppMode::ImportServers;
                                    app.input.set_value("servers.toml".to_string());
                                }
                                SidebarTarget::Favorite(p) => {
                                    let p = p.clone();
                                    if let Some(fs) = app.current_file_state_mut() {
                                        fs.current_path = p.clone();
                                        fs.remote_session = None;
                                        fs.selected_index = Some(0);
                                        fs.search_filter.clear();
                                        *fs.table_state.offset_mut() = 0;
                                        crate::event_helpers::push_history(fs, p);
                                    }
                                    let _ = event_tx
                                        .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                }
                                SidebarTarget::Storage(idx) => {
                                    if let Some(disk) = app.system_state.disks.get(*idx) {
                                        if !disk.is_mounted {
                                            let dev = disk.device.clone();
                                            let tx = event_tx.clone();
                                            let p_idx = app.focused_pane_index;
                                            tokio::spawn(async move {
                                                if let Ok(out) =
                                                    std::process::Command::new("udisksctl")
                                                        .arg("mount")
                                                        .arg("-b")
                                                        .arg(&dev)
                                                        .output()
                                                {
                                                    if String::from_utf8_lossy(&out.stdout)
                                                        .contains("Mounted")
                                                    {
                                                        tokio::time::sleep(Duration::from_millis(
                                                            200,
                                                        ))
                                                        .await;
                                                        let _ = tx
                                                            .send(AppEvent::RefreshFiles(p_idx))
                                                            .await;
                                                    }
                                                }
                                            });
                                        } else {
                                            let p = std::path::PathBuf::from(&disk.name);
                                            if let Some(fs) = app.current_file_state_mut() {
                                                fs.current_path = p.clone();
                                                fs.remote_session = None;
                                                fs.selected_index = Some(0);
                                                fs.search_filter.clear();
                                                *fs.table_state.offset_mut() = 0;
                                                crate::event_helpers::push_history(fs, p);
                                            }
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(
                                                app.focused_pane_index,
                                            ));
                                        }
                                    }
                                }
                                SidebarTarget::Remote(idx) => {
                                    crate::event_helpers::execute_command(
                                        CommandAction::ConnectToRemote(*idx),
                                        app,
                                        event_tx.clone(),
                                    )
                                }
                                _ => {}
                            }
                        }
                    }
                    app.is_dragging = false;
                    app.drag_start_pos = None;
                    app.drag_source = None;
                    app.hovered_drop_target = None;
                    return true;
                }
                MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                    app.mouse_pos = (column, row);
                    if app.is_resizing_sidebar {
                        app.sidebar_width_percent = (column as f32 / w as f32 * 100.0) as u16;
                        app.sidebar_width_percent = app.sidebar_width_percent.clamp(5, 50);
                        return true;
                    }

                    // Header Icon Hover
                    if let Some((_, id)) = app
                        .header_icon_bounds
                        .iter()
                        .find(|(r, _)| r.contains(ratatui::layout::Position { x: column, y: row }))
                    {
                        app.hovered_header_icon = Some(id.clone());
                    } else {
                        app.hovered_header_icon = None;
                    }

                    if let Some((sx, sy)) = app.drag_start_pos {
                        if ((column as i16 - sx as i16).pow(2) + (row as i16 - sy as i16).pow(2))
                            as f32
                            >= 1.0
                        {
                            app.is_dragging = true;
                        }
                    }

                    if app.is_dragging {
                        let sw = app.sidebar_width();
                        if let Some((sx, _)) = app.drag_start_pos {
                            if sx < sw {
                                if let Some(src) = app.drag_source.clone() {
                                    if let Some(h) = app.sidebar_bounds.iter().find(|b| b.y == row)
                                    {
                                        if let SidebarTarget::Favorite(t) = &h.target {
                                            if &src != t {
                                                if let Some(si) =
                                                    app.starred.iter().position(|p| p == &src)
                                                {
                                                    if let Some(ei) =
                                                        app.starred.iter().position(|p| p == t)
                                                    {
                                                        let item = app.starred.remove(si);
                                                        app.starred.insert(ei, item);
                                                        app.sidebar_index = h.index;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if app.mode == AppMode::ImportServers {
                            let (aw, ah) = ((w as f32 * 0.6) as u16, (h as f32 * 0.2) as u16);
                            let (ax, ay) = ((w - aw) / 2, (h - ah) / 2);
                            if column >= ax && column < ax + aw && row >= ay && row < ay + ah {
                                app.hovered_drop_target = Some(DropTarget::ImportServers);
                            } else if column >= sw && row >= 3 {
                                app.hovered_drop_target = Some(DropTarget::ImportServers);
                            } else {
                                app.hovered_drop_target = None;
                            }
                        } else if column < sw {
                            if let Some(b) = app.sidebar_bounds.iter().find(|b| b.y == row) {
                                if let SidebarTarget::Header(h) = &b.target {
                                    if h == "REMOTES" {
                                        app.hovered_drop_target = Some(DropTarget::RemotesHeader);
                                    } else {
                                        app.hovered_drop_target = Some(DropTarget::Favorites);
                                    }
                                } else if let SidebarTarget::Favorite(_) = &b.target {
                                    app.hovered_drop_target =
                                        Some(DropTarget::ReorderFavorite(b.index));
                                } else {
                                    app.hovered_drop_target = None;
                                }
                            }
                        } else {
                            // Check Breadcrumbs for Drop
                            if let Some(fs) = app.current_file_state() {
                                if let Some((_, path)) =
                                    fs.breadcrumb_bounds.iter().find(|(r, _)| {
                                        r.contains(ratatui::layout::Position { x: column, y: row })
                                    })
                                {
                                    app.hovered_drop_target =
                                        Some(DropTarget::Folder(path.clone()));
                                }
                            }
                            let cw = w.saturating_sub(sw);
                            let pc = app.panes.len();
                            let mut drop_pane_idx = None;
                            if pc > 1 {
                                let hi = (column.saturating_sub(sw) / (cw / pc as u16)) as usize;
                                if hi < pc {
                                    drop_pane_idx = Some(hi);
                                }
                            } else if pc == 1 {
                                drop_pane_idx = Some(0);
                            }

                            if let Some(hi) = drop_pane_idx {
                                // Check for folder hover in THIS pane
                                if row >= 3 {
                                    if let Some(fs) = app.panes.get(hi).and_then(|p| p.current_state()) {
                                        let mouse_row_offset = row.saturating_sub(3) as usize;
                                        let idx = fs.table_state.offset() + mouse_row_offset;
                                        if idx < fs.files.len() {
                                            let path = &fs.files[idx];
                                            if path.is_dir() {
                                                app.hovered_drop_target = Some(DropTarget::Folder(path.clone()));
                                                return true;
                                            }
                                        }
                                    }
                                }
                                
                                // Fallback to Pane (root of pane) only if not focused
                                if hi != app.focused_pane_index {
                                    app.hovered_drop_target = Some(DropTarget::Pane(hi));
                                    return true;
                                }
                            }
                        }
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
                        let max_offset = fs
                            .files
                            .len()
                            .saturating_sub(fs.view_height.saturating_sub(4));
                        let new_offset = (fs.table_state.offset() + 3).min(max_offset);
                        *fs.table_state.offset_mut() = new_offset;
                    }
                    return true;
                }
                _ => {}
            }
        }
        Event::Paste(text) => {
            if let AppMode::Editor = app.mode {
                if let Some(preview) = &mut app.editor_state {
                    if let Some(editor) = &mut preview.editor {
                        for c in text.chars() {
                            editor.handle_event(
                                &Event::Key(terma::input::event::KeyEvent {
                                    code: KeyCode::Char(c),
                                    modifiers: terma::input::event::KeyModifiers::empty(),
                                    kind: terma::input::event::KeyEventKind::Press,
                                }),
                                ratatui::layout::Rect::new(
                                    0,
                                    0,
                                    app.terminal_size.0,
                                    app.terminal_size.1,
                                ),
                            );
                        }
                        if app.auto_save {
                            let _ = event_tx.try_send(AppEvent::SaveFile(
                                preview.path.clone(),
                                editor.get_content(),
                            ));
                            editor.modified = false;
                        }
                        return true;
                    }
                }
            }
        }
        _ => {}
    }
    false
}

fn delete_word_backwards(s: &mut String) {
    if s.is_empty() {
        return;
    }
    let mut i = s.len();
    
    // Skip trailing whitespace
    while i > 0 {
        let prev = s[..i].chars().next_back().unwrap();
        if prev.is_whitespace() {
            i -= prev.len_utf8();
        } else {
            break;
        }
    }
    // Skip the word
    while i > 0 {
        let prev = s[..i].chars().next_back().unwrap();
        if !prev.is_whitespace() {
            i -= prev.len_utf8();
        } else {
            break;
        }
    }
    
    s.truncate(i);
}
