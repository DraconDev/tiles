use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use uuid::Uuid;

use notify::RecursiveMode;

// Terma Imports
use terma::input::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind, KeyEventKind};
use terma::integration::ratatui::TermaBackend;

// Ratatui Imports
use ratatui::Terminal;

use crate::app::{
    App, AppEvent, AppMode, CommandAction, ContextMenuAction, ContextMenuTarget, CurrentView,
    DropTarget, FileCategory, FileColumn, MonitorSubview, ProcessColumn, SettingsSection,
    SettingsTarget, SidebarTarget, UndoAction,
};
use crate::icons::{Icon, IconMode};
use unicode_width::UnicodeWidthStr;
use terma::utils::get_visual_width;

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

    // Watcher Setup
    let tx_clone = event_tx.clone();
    let mut debouncer = notify_debouncer_mini::new_debouncer(Duration::from_millis(500), move |res: notify_debouncer_mini::DebounceEventResult| {
        if let Ok(events) = res {
            for event in events {
                let _ = tx_clone.blocking_send(AppEvent::FilesChangedOnDisk(event.path));
            }
        }
    })?;
    let mut watched_paths: std::collections::HashMap<usize, PathBuf> = std::collections::HashMap::new();

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
                    if handle_event(raw, &mut app_guard, event_tx.clone(), &mut panes_needing_refresh) {
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
                            crate::app::log_debug(&format!(
                                "Attempting SSH connection to {}:{}",
                                remote.host, remote.port
                            ));
                            match std::net::TcpStream::connect(format!(
                                "{}:{}",
                                remote.host, remote.port
                            )) {
                                Ok(tcp) => {
                                    let mut sess = ssh2::Session::new().unwrap();
                                    sess.set_tcp_stream(tcp);
                                    sess.set_blocking(true);

                                    if let Err(e) = sess.handshake() {
                                        crate::app::log_debug(&format!(
                                            "SSH Handshake failed: {}",
                                            e
                                        ));
                                        let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                            "Handshake failed: {}",
                                            e
                                        )));
                                        return;
                                    }

                                    crate::app::log_debug(
                                        "SSH Handshake successful, attempting authentication...",
                                    );

                                    // Try Agent Auth
                                    let mut auth_ok = false;
                                    if let Ok(mut agent) = sess.agent() {
                                        crate::app::log_debug(
                                            "SSH Agent found, listing identities...",
                                        );
                                        if agent.connect().is_ok() {
                                            if let Ok(_identities) = agent.list_identities() {
                                                for identity in agent.identities().unwrap() {
                                                    crate::app::log_debug(&format!(
                                                        "Trying agent identity: {}",
                                                        identity.comment()
                                                    ));
                                                    if agent
                                                        .userauth(&remote.user, &identity)
                                                        .is_ok()
                                                    {
                                                        crate::app::log_debug(
                                                            "SSH Agent authentication successful",
                                                        );
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
                                            crate::app::log_debug(&format!(
                                                "Trying key authentication with: {:?}",
                                                key_path
                                            ));
                                            if sess
                                                .userauth_pubkey_file(
                                                    &remote.user,
                                                    None,
                                                    key_path,
                                                    None,
                                                )
                                                .is_ok()
                                            {
                                                crate::app::log_debug(
                                                    "SSH Key authentication successful",
                                                );
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
                                                crate::app::log_debug(&format!(
                                                    "Trying fallback key: {:?}",
                                                    key
                                                ));
                                                if sess
                                                    .userauth_pubkey_file(
                                                        &remote.user,
                                                        None,
                                                        &key,
                                                        None,
                                                    )
                                                    .is_ok()
                                                {
                                                    crate::app::log_debug("SSH Fallback key authentication successful");
                                                    auth_ok = true;
                                                    break;
                                                }
                                            }
                                        }
                                    }

                                    if auth_ok {
                                        crate::app::log_debug("SSH Connection fully established");
                                        let _ = tx
                                            .send(AppEvent::RemoteConnected(
                                                p_idx,
                                                crate::app::RemoteSession {
                                                    name: remote.name.clone(),
                                                    host: remote.host.clone(),
                                                    user: remote.user.clone(),
                                                    session: Arc::new(Mutex::new(sess)),
                                                },
                                            ))
                                            .await;
                                        let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                            "Connected to {}",
                                            remote.name
                                        )));
                                    } else {
                                        crate::app::log_debug(
                                            "SSH Authentication failed: no successful method found",
                                        );
                                        let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                            "Authentication failed for {}",
                                            remote.name
                                        )));
                                    }
                                }
                                Err(e) => {
                                    crate::app::log_debug(&format!("TCP Connection failed: {}", e));
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
                AppEvent::FilesChangedOnDisk(path) => {
                    // SHIELD: Ignore our own log file to prevent infinite refresh loops
                    if let Some(filename) = path.file_name() {
                        if filename == "debug.log" {
                            continue;
                        }
                    }

                    let mut app_guard = app.lock().unwrap();
                    let mut needs_reload = Vec::new();

                    // Check if open previews/editors need reload
                    if let Some(editor_state) = &mut app_guard.editor_state {
                        if editor_state.path == path {
                            if let Some(editor) = &editor_state.editor {
                                if !editor.modified {
                                    needs_reload.push((None, path.clone()));
                                }
                            }
                        }
                    }

                    for i in 0..app_guard.panes.len() {
                        let pane = &mut app_guard.panes[i];
                        if let Some(preview) = &mut pane.preview {
                            if preview.path == path {
                                if let Some(editor) = &preview.editor {
                                    if !editor.modified {
                                        needs_reload.push((Some(i), path.clone()));
                                    }
                                }
                            }
                        }

                        if let Some(fs) = pane.current_state() {
                            if path == fs.current_path || path.parent() == Some(fs.current_path.as_path()) {
                                 panes_needing_refresh.insert(i);
                            }
                        }
                    }

                    // Perform reloads
                    for (pane_idx, p) in needs_reload {
                        if let Ok(content) = std::fs::read_to_string(&p) {
                            if let Some(p_idx) = pane_idx {
                                if let Some(preview) = &mut app_guard.panes[p_idx].preview {
                                    if let Some(editor) = &mut preview.editor {
                                        editor.lines = content.lines().map(|s| s.to_string()).collect();
                                        if editor.lines.is_empty() { editor.lines.push(String::new()); }
                                        // Ensure trailing empty line for extra line after everything if not present
                                        if !editor.lines.last().map(|l| l.is_empty()).unwrap_or(false) {
                                            editor.lines.push(String::new());
                                        }
                                        editor.invalidate_from(0);
                                        preview.content = content;
                                        preview.highlighted_lines = None;
                                    }
                                }
                            } else if let Some(editor_state) = &mut app_guard.editor_state {
                                if let Some(editor) = &mut editor_state.editor {
                                    editor.lines = content.lines().map(|s| s.to_string()).collect();
                                    if editor.lines.is_empty() { editor.lines.push(String::new()); }
                                    if !editor.lines.last().map(|l| l.is_empty()).unwrap_or(false) {
                                        editor.lines.push(String::new());
                                    }
                                    editor.invalidate_from(0);
                                    editor_state.content = content;
                                    editor_state.highlighted_lines = None;
                                }
                            }
                        }
                    }
                }
                AppEvent::RefreshFiles(idx) => {
                    panes_needing_refresh.insert(idx);
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
                                if let Some(pos) = fs
                                    .files
                                    .iter()
                                    .position(|p| p.to_string_lossy() == "__DIVIDER__")
                                {
                                    fs.files.truncate(pos);
                                }

                                fs.files.push(std::path::PathBuf::from("__DIVIDER__"));
                                fs.files.extend(global_files);
                            }
                            needs_draw = true;
                        }
                    }
                }
                AppEvent::PreviewRequested(target_pane_idx, path) => {
                    let mut app_guard = app.lock().unwrap();
                    let category = crate::modules::files::get_file_category(&path);

                    let mut is_text = false;
                    let mut is_archive = false;

                    crate::app::log_debug(&format!(
                        "PreviewRequested for {:?}, Category: {:?}",
                        path, category
                    ));

                    if let Ok(_m) = std::fs::metadata(&path) {
                        is_text = matches!(category, FileCategory::Text | FileCategory::Script);
                        is_archive = matches!(category, FileCategory::Archive);
                    }

                    if is_text {
                        let (is_bin, is_large, mb) = terma::utils::check_file_suitability(&path, 1024 * 1024);
                        if is_large {
                            let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                "File too large for preview: {} ({} MB)",
                                path.display(),
                                mb
                            )));
                        } else if is_bin {
                             let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                "Binary file detected: {}",
                                path.display()
                            )));
                        } else {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                let mut editor =
                                    terma::widgets::editor::TextEditor::with_content(&content);
                                editor.wrap = true;
                                editor.style = ratatui::style::Style::default()
                                    .fg(ratatui::style::Color::Rgb(255, 255, 255));
                                editor.cursor_style = ratatui::style::Style::default()
                                    .bg(ratatui::style::Color::Rgb(255, 0, 85))
                                    .fg(ratatui::style::Color::Black);
                                
                                // Set language for syntax highlighting
                                let lang = path.extension()
                                    .and_then(|e| e.to_str())
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| {
                                        path.file_name()
                                            .map(|n| n.to_string_lossy().to_string())
                                            .unwrap_or_default()
                                    })
                                    .to_lowercase();
                                editor.language = lang;

                                if let Some(pane) = app_guard.panes.get_mut(target_pane_idx) {
                                    pane.preview = Some(crate::app::PreviewState {
                                        path: path.clone(),
                                        content: content.clone(),
                                        scroll: 0,
                                        editor: Some(editor),
                                        last_saved: None,
                                        image_data: None,
                                        highlighted_lines: None,
                                    });
                                }
                                needs_draw = true;
                            } else {
                                let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                    "Cannot read file as text: {}",
                                    path.display()
                                )));
                            }
                        }
                    } else if is_archive {
                        // Try to list contents
                        let tx = event_tx.clone();
                        let p = path.clone();
                        let app_clone = app.clone();
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();

                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                            "Listing contents of {}...",
                            p.file_name().unwrap_or_default().to_string_lossy()
                        )));

                        tokio::spawn(async move {
                            crate::app::log_debug(&format!("Archive listing started for: {:?}", p));

                            let has_lsar = std::process::Command::new("which")
                                .arg("lsar")
                                .output()
                                .map(|o| o.status.success())
                                .unwrap_or(false);
                            let has_7z = std::process::Command::new("which")
                                .arg("7z")
                                .output()
                                .map(|o| o.status.success())
                                .unwrap_or(false);
                            let has_unzip = std::process::Command::new("which")
                                .arg("unzip")
                                .output()
                                .map(|o| o.status.success())
                                .unwrap_or(false);
                            let has_tar = std::process::Command::new("which")
                                .arg("tar")
                                .output()
                                .map(|o| o.status.success())
                                .unwrap_or(false);
                            let has_python = std::process::Command::new("which")
                                .arg("python3")
                                .output()
                                .map(|o| o.status.success())
                                .unwrap_or(false);

                            crate::app::log_debug(&format!(
                                "Archive tools found: lsar={}, 7z={}, unzip={}, tar={}, python={}",
                                has_lsar, has_7z, has_unzip, has_tar, has_python
                            ));

                            let output = if has_lsar {
                                crate::app::log_debug("Using lsar");
                                std::process::Command::new("lsar").arg(&p).output()
                            } else if has_7z {
                                crate::app::log_debug("Using 7z");
                                std::process::Command::new("7z").arg("l").arg(&p).output()
                            } else if has_unzip {
                                crate::app::log_debug("Using unzip");
                                std::process::Command::new("unzip")
                                    .arg("-l")
                                    .arg(&p)
                                    .output()
                            } else if ext == "zip" && has_python {
                                crate::app::log_debug("Using python3 for zip listing");
                                std::process::Command::new("python3")
                                    .arg("-m")
                                    .arg("zipfile")
                                    .arg("-l")
                                    .arg(&p)
                                    .output()
                            } else if has_tar {
                                crate::app::log_debug("Using tar");
                                std::process::Command::new("tar")
                                    .arg("-tf")
                                    .arg(&p)
                                    .output()
                            } else {
                                crate::app::log_debug("No suitable listing tool found");
                                Err(std::io::Error::new(
                                    std::io::ErrorKind::NotFound,
                                    "No suitable tool to list archive contents",
                                ))
                            };

                            match output {
                                Ok(out) if out.status.success() => {
                                    let content = String::from_utf8_lossy(&out.stdout).into_owned();
                                    crate::app::log_debug(&format!(
                                        "Listing success, content len: {}",
                                        content.len()
                                    ));

                                    let mut editor =
                                        terma::widgets::editor::TextEditor::with_content(&content);
                                    editor.read_only = true;
                                    editor.wrap = true;
                                    editor.style = ratatui::style::Style::default()
                                        .fg(ratatui::style::Color::Rgb(255, 255, 255));
                                    editor.cursor_style = ratatui::style::Style::default()
                                        .bg(ratatui::style::Color::Rgb(255, 0, 85))
                                        .fg(ratatui::style::Color::Black);
                                    editor.language = "text".to_string();

                                    let mut app_lock = app_clone.lock().unwrap();
                                    app_lock.editor_state = Some(crate::app::PreviewState {
                                        path: p.clone(),
                                        content: content.clone(),
                                        scroll: 0,
                                        editor: Some(editor),
                                        last_saved: None,
                                        image_data: None,
                                        highlighted_lines: None,
                                    });
                                    app_lock.mode = AppMode::Viewer;
                                    crate::app::log_debug("AppMode changed to Viewer");
                                }
                                Ok(out) => {
                                    let err = String::from_utf8_lossy(&out.stderr);
                                    crate::app::log_debug(&format!(
                                        "Listing tool returned error: {}",
                                        err
                                    ));
                                    let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                        "Listing failed: {}",
                                        err.trim()
                                    )));
                                }
                                Err(e) => {
                                    crate::app::log_debug(&format!("Listing tool error: {}", e));
                                    let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                        "Listing error: {}",
                                        e
                                    )));
                                }
                            }
                            let _ = tx.try_send(AppEvent::Tick); // Force a redraw
                        });
                    } else {
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("unknown")
                            .to_lowercase();
                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                            "Preview not available for .{} (Use Enter to Open)",
                            ext
                        )));
                    }
                }
                AppEvent::SpawnTerminal {
                    path,
                    new_tab,
                    remote,
                    command,
                } => {
                    let mut final_command = command;
                    let mut local_path = path.clone();

                    if let Some(r) = remote {
                        let ssh_base = format!("ssh {}@{}", r.user, r.host);
                        let remote_path = path.to_string_lossy();
                        let ssh_cmd = if let Some(c) = final_command {
                            format!("{} -t \"cd '{}'; {}\"", ssh_base, remote_path, c)
                        } else {
                            format!("{} -t \"cd '{}'; exec $SHELL\"", ssh_base, remote_path)
                        };
                        final_command = Some(ssh_cmd);
                        // If it's remote, the local path might not exist, use home
                        local_path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
                    }

                    terma::utils::spawn_terminal_at(&local_path, new_tab, final_command.as_deref());
                }
                AppEvent::Delete(path) => {
                    let trash_path = dirs::home_dir()
                        .unwrap_or_default()
                        .join(".local/share/Trash/files");
                    let _ = std::fs::create_dir_all(&trash_path);
                    let file_name = path.file_name().unwrap_or_default();
                    let dest = trash_path.join(file_name);

                    if let Err(e) = std::fs::rename(&path, &dest) {
                        let _ =
                            event_tx.try_send(AppEvent::StatusMsg(format!("Delete failed: {}", e)));
                    } else {
                        let _undo_action = UndoAction::Delete(dest.clone()); // Store where it is in trash
                        let mut app_guard = app.lock().unwrap();
                        app_guard
                            .undo_stack
                            .push(UndoAction::Move(dest, path.clone())); // Undo is Move back
                        app_guard.redo_stack.clear();
                        for i in 0..app_guard.panes.len() {
                            panes_needing_refresh.insert(i);
                        }
                    }
                }
                AppEvent::SaveFile(path, content) => {
                    if let Err(e) = std::fs::write(&path, &content) {
                        let _ =
                            event_tx.try_send(AppEvent::StatusMsg(format!("Error saving: {}", e)));
                    } else {
                        // Update last_saved timestamp
                        let mut app_guard = app.lock().unwrap();
                        if let Some(preview) = &mut app_guard.editor_state {
                            if preview.path == path {
                                preview.last_saved = Some(std::time::Instant::now());
                            }
                        }
                        for pane in &mut app_guard.panes {
                            if let Some(preview) = &mut pane.preview {
                                if preview.path == path {
                                    preview.last_saved = Some(std::time::Instant::now());
                                }
                            }
                        }
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
                            app_guard
                                .undo_stack
                                .push(UndoAction::Rename(dest.clone(), src.clone()));
                            app_guard.redo_stack.clear();
                            drop(app_guard);
                            let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                "Moved {} to {}",
                                src.display(),
                                dest.display()
                            )));
                            let app_guard = app.lock().unwrap();
                            for i in 0..app_guard.panes.len() {
                                panes_needing_refresh.insert(i);
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
                            panes_needing_refresh.insert(i);
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
                            panes_needing_refresh.insert(i);
                        }
                    }
                }
                AppEvent::Copy(src, dest) => {
                    let tx = event_tx.clone();
                    let app_arc = app.clone();
                    tokio::spawn(async move {
                        let task_id = Uuid::new_v4();
                        let _ = tx
                            .send(AppEvent::TaskProgress(
                                task_id,
                                0.0,
                                format!(
                                    "Copying {}...",
                                    src.file_name().unwrap_or_default().to_string_lossy()
                                ),
                            ))
                            .await;

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
                            app_guard
                                .undo_stack
                                .push(UndoAction::Copy(src.clone(), dest.clone()));
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
                AppEvent::Symlink(src, dest) => {
                    let tx = event_tx.clone();
                    #[cfg(unix)]
                    {
                        if let Err(e) = std::os::unix::fs::symlink(&src, &dest) {
                            let _ = tx.try_send(AppEvent::StatusMsg(format!("Error symlinking: {}", e)));
                        } else {
                            let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                "Symlinked {} to {}",
                                src.display(),
                                dest.display()
                            )));
                            let _ = tx.try_send(AppEvent::RefreshFiles(0));
                            let _ = tx.try_send(AppEvent::RefreshFiles(1));
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = tx.try_send(AppEvent::StatusMsg(format!("Symlinking only supported on Unix")));
                    }
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
                                let _ = tx
                                    .try_send(AppEvent::StatusMsg(format!("Launched {}", cmd_str)));
                            }
                            Err(e) => {
                                let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                    "Failed to launch {}: {}",
                                    cmd_str, e
                                )));
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
                AppEvent::GitHistory => {
                    let (p_idx, t_idx, path, base_path) = {
                        let app_guard = app.lock().unwrap();
                        let p_idx = app_guard.focused_pane_index;
                        let pane = &app_guard.panes[p_idx];
                        let t_idx = pane.active_tab_index;
                        let tab = &pane.tabs[t_idx];
                        let base_path = tab.current_path.clone();
                        
                        // If a file is selected, show history for THAT file
                        let target_path = if let Some(idx) = tab.selection.selected {
                            if let Some(p) = tab.files.get(idx) {
                                if p.to_string_lossy() != "__DIVIDER__" {
                                    p.clone()
                                } else {
                                    base_path.clone()
                                }
                            } else {
                                base_path.clone()
                            }
                        } else {
                            base_path.clone()
                        };
                        
                        (p_idx, t_idx, target_path, base_path)
                    };

                    let tx = event_tx.clone();
                    tokio::spawn(async move {
                        let history = crate::modules::files::get_git_history(&path, 100);
                        let pending = crate::modules::files::get_git_status(&base_path);
                        let _ = tx.send(AppEvent::GitHistoryUpdated(p_idx, t_idx, history, pending)).await;
                    });

                    let mut app_guard = app.lock().unwrap();
                    if app_guard.current_view == CurrentView::Git {
                        app_guard.current_view = CurrentView::Files;
                    } else {
                        app_guard.current_view = CurrentView::Git;
                    }
                    needs_draw = true;
                }
                AppEvent::GitHistoryUpdated(p_idx, t_idx, history, pending) => {
                    let mut app_guard = app.lock().unwrap();
                    if let Some(pane) = app_guard.panes.get_mut(p_idx) {
                        if let Some(tab) = pane.tabs.get_mut(t_idx) {
                            tab.git_history = history;
                            tab.git_pending = pending;
                            if tab.git_history_state.selected().is_none() && !tab.git_history.is_empty() {
                                tab.git_history_state.select(Some(0));
                            }
                        }
                    }
                    needs_draw = true;
                }
                AppEvent::Editor => {
                    let mut app_guard = app.lock().unwrap();
                    if app_guard.current_view == CurrentView::Editor {
                        app_guard.current_view = CurrentView::Files;
                        app_guard.show_sidebar = true; // Restore regular sidebar
                        app_guard.sidebar_focus = false;
                        app_guard.show_side_panel = false;
                        
                        // Clear all previews
                        for pane in &mut app_guard.panes {
                            pane.preview = None;
                        }
                    } else {
                        app_guard.current_view = CurrentView::Editor;
                        app_guard.show_sidebar = false; // IDE starts clean
                        app_guard.show_side_panel = false;
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

        // --- PERFORM COALESCED REFRESHES ---
        if !panes_needing_refresh.is_empty() {
            let mut app_guard = app.lock().unwrap();
            for idx in panes_needing_refresh.drain() {
                if let Some(pane) = app_guard.panes.get_mut(idx) {
                    if let Some(fs) = pane.current_state_mut() {
                        let session_arc = fs.remote_session.as_ref().map(|s| s.session.clone());
                        if let Some(arc) = session_arc {
                            let sess = arc.lock().unwrap();
                            crate::modules::files::update_files(fs, Some(&sess));
                        } else {
                            // 1. Local update (immediate)
                            crate::modules::files::update_files(fs, None);

                            // Update Watcher
                            let p = fs.current_path.clone();
                            let needs_update = watched_paths.get(&idx).map(|old| *old != p).unwrap_or(true);
                            
                            if needs_update {
                                if let Some(old) = watched_paths.get(&idx) {
                                    let _ = debouncer.watcher().unwatch(old);
                                }
                                if let Err(e) = debouncer.watcher().watch(&p, RecursiveMode::NonRecursive) {
                                     crate::app::log_debug(&format!("Watch failed for {:?}: {}", p, e));
                                } else {
                                     watched_paths.insert(idx, p);
                                }
                            }

                            // Restore selection if navigating back
                            if let Some(pending) = &fs.pending_select_path {
                                if let Some(pos) = fs.files.iter().position(|p| p == pending) {
                                    fs.selection.selected = Some(pos);
                                    fs.selection.anchor = Some(pos);
                                    fs.table_state.select(Some(pos));
                                    if fs.view_height > 0 {
                                        *fs.table_state.offset_mut() =
                                            pos.saturating_sub(fs.view_height / 2);
                                    }
                                }
                                fs.pending_select_path = None;
                            }

                            // 2. Trigger Global search if needed (background)
                            if fs.search_filter.len() >= 3 {
                                let filter = fs.search_filter.clone();
                                let current_path = fs.current_path.clone();
                                let show_hidden = fs.show_hidden;
                                let local_files = fs.files.clone();
                                let tx = event_tx.clone();
                                let p_idx = idx;

                                tokio::spawn(async move {
                                    let (global_files, metadata) =
                                        crate::modules::files::perform_global_search(
                                            filter,
                                            current_path,
                                            show_hidden,
                                            local_files,
                                        );
                                    let _ = tx.try_send(AppEvent::GlobalSearchUpdated(
                                        p_idx,
                                        global_files,
                                        metadata,
                                    ));
                                });
                            }
                        }
                    }
                } else {
                    // Pane is gone, cleanup watcher
                    if let Some(old) = watched_paths.remove(&idx) {
                        let _ = debouncer.watcher().unwatch(&old);
                    }
                }
            }
            needs_draw = true;
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
    suggestions
        .into_iter()
        .filter(|s| terma::utils::command_exists(s))
        .collect()
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
fn handle_event(
    evt: Event,
    app: &mut App,
    event_tx: mpsc::Sender<AppEvent>,
    panes_needing_refresh: &mut std::collections::HashSet<usize>,
) -> bool {
    // SHIELD: Global input cooldown to prevent artifact leakage (e.g. from Escape sequences)
    if let Some(until) = app.input_shield_until {
        if std::time::Instant::now() < until {
            // Still ignore resize events normally, but consume others
            match evt {
                Event::Resize(w, h) => {
                    app.terminal_size = (w, h);
                }
                _ => {}
            }
            return true; 
        }
    }

    match evt {
        Event::Resize(w, h) => {
            app.terminal_size = (w, h);
            return true;
        }
        Event::Key(key) => {
            if key.kind != KeyEventKind::Press {
                return false;
            }
            if key.code == KeyCode::Char('?') {
                crate::app::log_debug("DEBUG: '?' key detected at top level");
            }
            crate::app::log_debug(&format!(
                "KEY EVENT: {:?} Modifiers: {:?}",
                key.code, key.modifiers
            ));
            let has_control = key.modifiers.contains(KeyModifiers::CONTROL);
            let has_alt = key.modifiers.contains(KeyModifiers::ALT);
            let has_shift = key.modifiers.contains(KeyModifiers::SHIFT);

            if key.code == KeyCode::Char('q') || key.code == KeyCode::Char('Q') {
                if has_control {
                    app.running = false;
                    return true;
                }
            }

            // Global Escape (Ctrl+[)
            if has_control && key.code == KeyCode::Char('[') {
                if matches!(app.mode, AppMode::Normal) {
                    match app.current_view {
                        CurrentView::Git | CurrentView::Processes => {
                            app.current_view = CurrentView::Files;
                            return true;
                        }
                        CurrentView::Editor => {
                            app.save_current_view_prefs();
                            app.current_view = CurrentView::Files;
                            app.load_view_prefs(CurrentView::Files);
                            for pane in &mut app.panes {
                                pane.preview = None;
                            }
                            app.input_shield_until = Some(std::time::Instant::now() + std::time::Duration::from_millis(50));
                            return true;
                        }
                        _ => {}
                    }
                } else {
                    app.mode = AppMode::Normal;
                    app.input.clear();
                    app.rename_selected = false;
                    return true;
                }
            }

            // --- GLOBAL OVERRIDES (High Priority) ---
            if has_control {
                match key.code {
                    KeyCode::Char('p') | KeyCode::Char('P') => {
                        app.toggle_split();
                        app.save_current_view_prefs();
                        let _ = crate::config::save_state(app);
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                        return true;
                    }
                    KeyCode::Char('b') | KeyCode::Char('B') => {
                        app.show_sidebar = !app.show_sidebar;
                        app.save_current_view_prefs();
                        return true;
                    }
                    KeyCode::Char('e') | KeyCode::Char('E') => {
                        let _ = event_tx.try_send(AppEvent::Editor);
                        return true;
                    }
                    KeyCode::Char('l') | KeyCode::Char('L') => {
                        let _ = event_tx.try_send(AppEvent::GitHistory);
                        return true;
                    }
                    _ => {}
                }
            }

            // View-Specific Esc Handling (Prioritize over mode checks)
            if key.code == KeyCode::Esc && matches!(app.mode, AppMode::Normal) {
                match app.current_view {
                    CurrentView::Git | CurrentView::Processes => {
                        app.current_view = CurrentView::Files;
                        return true;
                    }
                    CurrentView::Editor => {
                        app.save_current_view_prefs();
                        app.current_view = CurrentView::Files;
                        app.load_view_prefs(CurrentView::Files);
                        // Clear previews to show file list
                        for pane in &mut app.panes {
                            pane.preview = None;
                        }
                        // SHIELD: Prevent trailing escape sequence fragments from leaking into search
                        app.input_shield_until = Some(std::time::Instant::now() + std::time::Duration::from_millis(50));
                        return true;
                    }
                    _ => {}
                }
            }

            // IDE/Editor Mode Key Handling
            if app.current_view == CurrentView::Editor && !app.sidebar_focus && matches!(app.mode, AppMode::Normal) {
                let (w, h) = app.terminal_size;
                let sw = app.sidebar_width();
                let pc = app.panes.len();
                let cw = w.saturating_sub(sw);
                let pw = if pc > 0 { cw / pc as u16 } else { cw };
                let pane_idx = app.focused_pane_index;

                let pane_area = ratatui::layout::Rect::new(
                    sw + (pane_idx as u16 * pw),
                    1, 
                    pw,
                    h.saturating_sub(1), // Header(1)
                );

                if let Some(pane) = app.panes.get_mut(pane_idx) {
                    if let Some(preview) = &mut pane.preview {
                        if let Some(editor) = &mut preview.editor {
                            // Manual Save
                            if has_control && (key.code == KeyCode::Char('s') || key.code == KeyCode::Char('S')) {
                                let _ = event_tx.try_send(AppEvent::SaveFile(
                                    preview.path.clone(),
                                    editor.get_content(),
                                ));
                                editor.modified = false;
                                return true;
                            }

                            // 1. Copy (Selection or Line)
                            if (has_control && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('C'))) || (has_control && key.code == KeyCode::Insert) {
                                let content = if let Some(selected) = editor.get_selected_text() {
                                    selected
                                } else {
                                    editor.lines.get(editor.cursor_row).cloned().unwrap_or_default()
                                };
                                app.editor_clipboard = Some(content.clone());
                                terma::utils::set_clipboard_text(&content);
                                let _ = event_tx.try_send(AppEvent::StatusMsg("Copied to clipboard".to_string()));
                                return true;
                            }

                            // 2. Cut (Selection or Line)
                            if (has_control && (key.code == KeyCode::Char('x') || key.code == KeyCode::Char('X'))) || (key.modifiers.contains(KeyModifiers::SHIFT) && key.code == KeyCode::Delete) {
                                let content = if let Some(selected) = editor.get_selected_text() {
                                    selected
                                } else {
                                    editor.lines.get(editor.cursor_row).cloned().unwrap_or_default()
                                };
                                app.editor_clipboard = Some(content.clone());
                                terma::utils::set_clipboard_text(&content);
                                
                                if let Some(_) = editor.get_selection_range() {
                                    editor.push_history();
                                    editor.delete_selection();
                                } else {
                                    // Cut line if no selection
                                    editor.delete_line(editor.cursor_row);
                                }
                                
                                let _ = event_tx.try_send(AppEvent::StatusMsg("Cut to clipboard".to_string()));
                                if app.auto_save {
                                    let _ = event_tx.try_send(AppEvent::SaveFile(preview.path.clone(), editor.get_content()));
                                    editor.modified = false;
                                }
                                return true;
                            }

                            // 3. Paste
                            if (has_control && (key.code == KeyCode::Char('v') || key.code == KeyCode::Char('V'))) || (key.modifiers.contains(KeyModifiers::SHIFT) && key.code == KeyCode::Insert) {
                                let text_to_paste = app.editor_clipboard.clone().or_else(|| terma::utils::get_clipboard_text());
                                if let Some(text) = text_to_paste {
                                    editor.insert_string(&text);
                                    editor.modified = true;
                                    if app.auto_save {
                                        let _ = event_tx.try_send(AppEvent::SaveFile(preview.path.clone(), editor.get_content()));
                                        editor.modified = false;
                                    }
                                }
                                return true;
                            }

                            // 4. Undo / Redo
                            if has_control && (key.code == KeyCode::Char('z') || key.code == KeyCode::Char('Z')) {
                                editor.handle_event(&evt, pane_area);
                                return true;
                            }
                            if has_control && (key.code == KeyCode::Char('y') || key.code == KeyCode::Char('Y')) {
                                editor.handle_event(&evt, pane_area);
                                return true;
                            }

                            // Search / Replace / GoToLine
                            if has_control {
                                match key.code {
                                    KeyCode::Char('f') | KeyCode::Char('F') => {
                                        app.previous_mode = AppMode::Normal;
                                        app.mode = AppMode::EditorSearch;
                                        app.input.set_value(editor.filter_query.clone());
                                        return true;
                                    }
                                    KeyCode::Char('g') | KeyCode::Char('G') => {
                                        app.previous_mode = AppMode::Normal;
                                        app.mode = AppMode::EditorGoToLine;
                                        app.input.clear();
                                        return true;
                                    }
                                    KeyCode::Char('r') | KeyCode::Char('R') => {
                                        app.previous_mode = AppMode::Normal;
                                        app.mode = AppMode::EditorReplace;
                                        app.input.clear();
                                        app.replace_buffer.clear();
                                        let _ = event_tx.try_send(AppEvent::StatusMsg(
                                            "Replace: Type term to FIND, then press Enter/Tab".to_string(),
                                        ));
                                        return true;
                                    }
                                    _ => {}
                                }
                            }
                            if key.code == KeyCode::F(2) {
                                app.previous_mode = AppMode::Normal;
                                app.mode = AppMode::EditorReplace;
                                app.input.clear();
                                app.replace_buffer.clear();
                                let _ = event_tx.try_send(AppEvent::StatusMsg(
                                    "Replace: Type term to FIND, then press Enter/Tab".to_string(),
                                ));
                                return true;
                            }

                            if editor.handle_event(&evt, pane_area) {
                                if app.auto_save && editor.modified {
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
            }

            // 1. Full-Screen Editor Priority (Traps all input)
            if let AppMode::Editor = app.mode {
                if let Some(preview) = &mut app.editor_state {
                    if let Some(editor) = &mut preview.editor {
                        if matches!(app.mode, AppMode::Editor) {
                            if key.code == KeyCode::Esc {
                                app.mode = AppMode::Normal;
                                app.editor_state = None;
                                return true;
                            }
                        }
                        // 1. Copy (Selection or Line)
                        if (has_control && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('C'))) || (has_control && key.code == KeyCode::Insert) {
                            let content = if let Some(selected) = editor.get_selected_text() {
                                selected
                            } else {
                                editor.lines.get(editor.cursor_row).cloned().unwrap_or_default()
                            };
                            app.editor_clipboard = Some(content.clone());
                            terma::utils::set_clipboard_text(&content);
                            let _ = event_tx.try_send(AppEvent::StatusMsg("Copied to clipboard".to_string()));
                            return true;
                        }

                        // 2. Cut (Selection or Line)
                        if (has_control && (key.code == KeyCode::Char('x') || key.code == KeyCode::Char('X'))) || (key.modifiers.contains(KeyModifiers::SHIFT) && key.code == KeyCode::Delete) {
                            let content = if let Some(selected) = editor.get_selected_text() {
                                selected
                            } else {
                                editor.lines.get(editor.cursor_row).cloned().unwrap_or_default()
                            };
                            app.editor_clipboard = Some(content.clone());
                            terma::utils::set_clipboard_text(&content);

                            if let Some(_) = editor.get_selection_range() {
                                editor.push_history();
                                editor.delete_selection();
                            } else {
                                // Cut line if no selection
                                editor.delete_line(editor.cursor_row);
                            }

                            let _ = event_tx.try_send(AppEvent::StatusMsg("Cut to clipboard".to_string()));
                            if app.auto_save {
                                let _ = event_tx.try_send(AppEvent::SaveFile(preview.path.clone(), editor.get_content()));
                                editor.modified = false;
                            }
                            return true;
                        }

                        // 3. Paste
                        if (has_control && (key.code == KeyCode::Char('v') || key.code == KeyCode::Char('V'))) || (key.modifiers.contains(KeyModifiers::SHIFT) && key.code == KeyCode::Insert) {
                            let text_to_paste = app.editor_clipboard.clone().or_else(|| terma::utils::get_clipboard_text());
                            if let Some(text) = text_to_paste {
                                editor.insert_string(&text);
                                editor.modified = true;
                                if app.auto_save {
                                    let _ = event_tx.try_send(AppEvent::SaveFile(preview.path.clone(), editor.get_content()));
                                    editor.modified = false;
                                }
                            }
                            return true;
                        }

                        // 4. Undo / Redo
                        if has_control && (key.code == KeyCode::Char('z') || key.code == KeyCode::Char('Z')) {
                            let (w, h) = app.terminal_size;
                            let editor_area = ratatui::layout::Rect::new(1, 1, w.saturating_sub(2), h.saturating_sub(2));
                            editor.handle_event(&evt, editor_area);
                            return true;
                        }
                        if has_control && (key.code == KeyCode::Char('y') || key.code == KeyCode::Char('Y')) {
                            let (w, h) = app.terminal_size;
                            let editor_area = ratatui::layout::Rect::new(1, 1, w.saturating_sub(2), h.saturating_sub(2));
                            editor.handle_event(&evt, editor_area);
                            return true;
                        }
                        if has_control && (key.code == KeyCode::Char('f') || key.code == KeyCode::Char('F')) {
                            app.previous_mode = AppMode::Normal;
                            app.mode = AppMode::EditorSearch;
                            // Pre-fill with current filter if any
                            app.input.set_value(editor.filter_query.clone());
                            return true;
                        }
                        if let KeyCode::Char('r') | KeyCode::Char('R') | KeyCode::F(2) = key.code {
                            if has_control || key.code == KeyCode::F(2) {
                                app.previous_mode = AppMode::Normal;
                                app.mode = AppMode::EditorReplace;
                                app.input.clear();
                                app.replace_buffer.clear();
                                let _ = event_tx.try_send(AppEvent::StatusMsg(
                                    "Replace: Type term to FIND, then press Enter/Tab".to_string(),
                                ));
                                return true;
                            }
                        }
                        if has_control && (key.code == KeyCode::Char('g') || key.code == KeyCode::Char('G')) {
                            app.previous_mode = AppMode::Normal;
                            app.mode = AppMode::EditorGoToLine;
                            app.input.clear();
                            return true;
                        }

                        let (w, h) = app.terminal_size;
                        // Adjust area for the border (1 char padding on all sides)
                        let editor_area = ratatui::layout::Rect::new(
                            1,
                            1,
                            w.saturating_sub(2),
                            h.saturating_sub(2),
                        );

                        if key.code == KeyCode::Delete && !has_control && !has_alt && !has_shift {
                            if editor.cursor_row == editor.lines.len().saturating_sub(1) && 
                               editor.cursor_col == editor.lines[editor.cursor_row].len() {
                                // At EOF
                                app.mode = AppMode::DeleteFile(preview.path.clone());
                                app.input.set_value("y".to_string()); 
                                return true;
                            }
                        }

                        if editor.handle_event(&evt, editor_area) {
                            // AUTO-SYNC SELECTION TO CLIPBOARD
                            if let Some(selected_text) = editor.get_selected_text() {
                                if selected_text.width() > 1 {
                                    app.editor_clipboard = Some(selected_text.clone());
                                    terma::utils::set_clipboard_text(&selected_text);
                                }
                            }

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

            // Global Help Override (Moved here so Editor takes priority)
            if key.code == KeyCode::F(1) || (key.code == KeyCode::Char('?') && !matches!(app.mode, AppMode::Editor | AppMode::NewFile | AppMode::NewFolder | AppMode::Rename | AppMode::Command | AppMode::Search | AppMode::EditorSearch | AppMode::EditorReplace | AppMode::EditorGoToLine)) {
                crate::app::log_debug("Help Triggered");
                if let AppMode::Hotkeys = app.mode {
                    app.mode = app.previous_mode.clone();
                } else {
                    app.previous_mode = app.mode.clone();
                    app.mode = AppMode::Hotkeys;
                }
                return true;
            }

            // 2. Global Shortcuts
            match key.code {
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
                KeyCode::Enter if has_alt => {
                    app.mode = AppMode::Properties;
                    return true;
                }
                KeyCode::Char('h') | KeyCode::Char('H') if has_control => {
                    let idx = app.toggle_hidden();
                    if let Some(fs) = app.panes.get(idx).and_then(|p| p.current_state()) {
                        app.default_show_hidden = fs.show_hidden;
                    }
                    let _ = crate::config::save_state(app);
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
                            // Borrow `app` mutably once outside the `if let`
                            let file_state_option = app.current_file_state();
                            if let Some(_) = file_state_option {
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
                            }
                            return true;
                        }
                        _ => return true,
                    }
                }
                AppMode::DragDropMenu { sources, target } => match key.code {
                    KeyCode::Char('c') | KeyCode::Char('C') => {
                        for source in sources {
                            let dest = target.join(
                                source
                                    .file_name()
                                    .unwrap_or_else(|| std::ffi::OsStr::new("root")),
                            );
                            let _ = event_tx.try_send(AppEvent::Copy(source.clone(), dest));
                        }
                        app.mode = AppMode::Normal;
                        return true;
                    }
                    KeyCode::Char('m') | KeyCode::Char('M') => {
                        for source in sources {
                            let dest = target.join(
                                source
                                    .file_name()
                                    .unwrap_or_else(|| std::ffi::OsStr::new("root")),
                            );
                            let _ = event_tx.try_send(AppEvent::Rename(source.clone(), dest));
                        }
                        if let Some(fs) = app.current_file_state_mut() {
                            fs.selection.clear_multi();
                            fs.selection.anchor = None;
                        }
                        app.mode = AppMode::Normal;
                        return true;
                    }
                    KeyCode::Char('l') | KeyCode::Char('L') => {
                        for source in sources {
                            let dest = target.join(
                                source
                                    .file_name()
                                    .unwrap_or_else(|| std::ffi::OsStr::new("root")),
                            );
                            let _ = event_tx.try_send(AppEvent::Symlink(source.clone(), dest));
                        }
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
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('q') | KeyCode::Char('?') | KeyCode::F(1) => {
                            app.mode = app.previous_mode.clone();
                            return true;
                        }
                        _ => return true,
                    }
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
                            let _ = event_tx.try_send(AppEvent::StatusMsg(
                                format!("Replace '{}' with: (Enter: next, ^Enter: all)", app.replace_buffer)
                            ));
                        } else {
                            // Stage 2: Captured Replace term
                            let replace_term = app.input.value.clone();
                            let find_term = app.replace_buffer.clone();

                            // 1. Check global editor_state (legacy/viewer)
                            if let Some(preview) = &mut app.editor_state {
                                if let Some(editor) = &mut preview.editor {
                                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                                        editor.push_history();
                                        editor.replace_all(&find_term, &replace_term);
                                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                            "Replaced all '{}' with '{}'",
                                            find_term, replace_term
                                        )));
                                        app.mode = app.previous_mode.clone();
                                        app.input.clear();
                                        app.replace_buffer.clear();
                                    } else {
                                        editor.push_history();
                                        if !editor.replace_next(&find_term, &replace_term) {
                                            let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                                "No more occurrences of '{}' found",
                                                find_term
                                            )));
                                        }
                                        app.input.clear();
                                        app.replace_buffer.clear();
                                        app.mode = app.previous_mode.clone();
                                        let (w, h) = app.terminal_size;
                                        let area = ratatui::layout::Rect::new(1, 1, w.saturating_sub(2), h.saturating_sub(2));
                                        editor.ensure_cursor_centered(area);
                                    }
                                }
                            }
                            
                            // 2. Check focused pane's preview (IDE mode)
                            let focused_idx = app.focused_pane_index;
                            let (w, h) = app.terminal_size;
                            let sw = app.sidebar_width();
                            let pc = app.panes.len();
                            let cw = w.saturating_sub(sw);
                            let pw = if pc > 0 { cw / pc as u16 } else { cw };
                            let pane_area = ratatui::layout::Rect::new(
                                sw + (focused_idx as u16 * pw),
                                1, pw, h.saturating_sub(1)
                            );

                            if let Some(pane) = app.panes.get_mut(focused_idx) {
                                if let Some(preview) = &mut pane.preview {
                                    if let Some(editor) = &mut preview.editor {
                                        if key.modifiers.contains(KeyModifiers::CONTROL) {
                                            editor.push_history();
                                            editor.replace_all(&find_term, &replace_term);
                                            let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                                "Replaced all '{}' with '{}'",
                                                find_term, replace_term
                                            )));
                                            app.mode = app.previous_mode.clone();
                                            app.input.clear();
                                            app.replace_buffer.clear();
                                        } else {
                                            editor.push_history();
                                            if !editor.replace_next(&find_term, &replace_term) {
                                                let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                                    "No more occurrences of '{}' found",
                                                    find_term
                                                )));
                                            }
                                            app.input.clear();
                                            app.replace_buffer.clear();
                                            app.mode = app.previous_mode.clone();
                                            editor.ensure_cursor_centered(pane_area);
                                        }
                                    }
                                }
                            }
                        }
                        return true;
                    }
                    _ => {
                        let res = app.input.handle_event(&evt);
                        if res {
                            // If we are in Stage 1 (Find) and user cleared the input, cancel
                            if app.replace_buffer.is_empty() && app.input.value.is_empty() {
                                app.mode = app.previous_mode.clone();
                                app.input.clear();
                                app.replace_buffer.clear();
                                return true;
                            }
                        }
                        return res;
                    }
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
                        // 1. Clear filter on Cancel (global)
                        if let Some(preview) = &mut app.editor_state {
                            if let Some(editor) = &mut preview.editor {
                                editor.set_filter("");
                            }
                        }
                        // 2. Clear filter on Cancel (IDE)
                        let focused_idx = app.focused_pane_index;
                        if let Some(pane) = app.panes.get_mut(focused_idx) {
                            if let Some(preview) = &mut pane.preview {
                                if let Some(editor) = &mut preview.editor {
                                    editor.set_filter("");
                                }
                            }
                        }
                        app.mode = app.previous_mode.clone();
                        app.input.clear();
                        return true;
                    }
                    KeyCode::Enter => {
                        // 1. Clear filter on Enter (global)
                        if let Some(preview) = &mut app.editor_state {
                            if let Some(editor) = &mut preview.editor {
                                editor.set_filter("");
                                let (w, h) = app.terminal_size;
                                let area = ratatui::layout::Rect::new(1, 1, w.saturating_sub(2), h.saturating_sub(2));
                                editor.ensure_cursor_centered(area);
                            }
                        }
                        // 2. Clear filter on Enter (IDE)
                        let (w, h) = app.terminal_size;
                        let sw = app.sidebar_width();
                        let pc = app.panes.len();
                        let cw = w.saturating_sub(sw);
                        let pw = if pc > 0 { cw / pc as u16 } else { cw };
                        let focused_idx = app.focused_pane_index;
                        let pane_area = ratatui::layout::Rect::new(
                            sw + (focused_idx as u16 * pw),
                            1, pw, h.saturating_sub(1)
                        );

                        if let Some(pane) = app.panes.get_mut(focused_idx) {
                            if let Some(preview) = &mut pane.preview {
                                if let Some(editor) = &mut preview.editor {
                                    editor.set_filter("");
                                    editor.ensure_cursor_centered(pane_area);
                                }
                            }
                        }
                        app.mode = app.previous_mode.clone();
                        app.input.clear();
                        return true;
                    }
                    KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown => {
                        // Navigation in Search Mode
                        if let Some(preview) = &mut app.editor_state {
                            if let Some(editor) = &mut preview.editor {
                                let (w, h) = app.terminal_size;
                                let area = ratatui::layout::Rect::new(1, 1, w.saturating_sub(2), h.saturating_sub(2));
                                editor.handle_event(&evt, area);
                            }
                        }
                        let (w, h) = app.terminal_size;
                        let sw = app.sidebar_width();
                        let pc = app.panes.len();
                        let cw = w.saturating_sub(sw);
                        let pw = if pc > 0 { cw / pc as u16 } else { cw };
                        let focused_idx = app.focused_pane_index;
                        let pane_area = ratatui::layout::Rect::new(
                            sw + (focused_idx as u16 * pw),
                            1, pw, h.saturating_sub(1)
                        );

                        if let Some(pane) = app.panes.get_mut(focused_idx) {
                            if let Some(preview) = &mut pane.preview {
                                if let Some(editor) = &mut preview.editor {
                                    editor.handle_event(&evt, pane_area);
                                }
                            }
                        }
                        return true;
                    }
                    _ => {
                        let handled = app.input.handle_event(&evt);
                        if handled {
                            // If user cleared the search via backspace, cancel search mode
                            if app.input.value.is_empty() {
                                // 1. Clear filter (global)
                                if let Some(preview) = &mut app.editor_state {
                                    if let Some(editor) = &mut preview.editor {
                                        editor.set_filter("");
                                    }
                                }
                                // 2. Clear filter (IDE)
                                let focused_idx = app.focused_pane_index;
                                if let Some(pane) = app.panes.get_mut(focused_idx) {
                                    if let Some(preview) = &mut pane.preview {
                                        if let Some(editor) = &mut preview.editor {
                                            editor.set_filter("");
                                        }
                                    }
                                }
                                app.mode = app.previous_mode.clone();
                                return true;
                            }

                            // Update Live Filters
                            if let Some(preview) = &mut app.editor_state {
                                if let Some(editor) = &mut preview.editor {
                                    editor.set_filter(&app.input.value);
                                }
                            }
                            let focused_idx = app.focused_pane_index;
                            if let Some(pane) = app.panes.get_mut(focused_idx) {
                                if let Some(preview) = &mut pane.preview {
                                    if let Some(editor) = &mut preview.editor {
                                        editor.set_filter(&app.input.value);
                                    }
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
                            let target = line_num.saturating_sub(1); // 1-based to 0-based

                            // 1. Update global editor_state (legacy/viewer)
                            if let Some(preview) = &mut app.editor_state {
                                if let Some(editor) = &mut preview.editor {
                                    editor.cursor_row = std::cmp::min(target, editor.lines.len().saturating_sub(1));
                                    editor.cursor_col = 0;
                                    let (w, h) = app.terminal_size;
                                    let area = ratatui::layout::Rect::new(1, 1, w.saturating_sub(2), h.saturating_sub(2));
                                    editor.ensure_cursor_centered(area);
                                }
                            }
                            // 2. Update focused pane's preview (IDE mode)
                            let focused_idx = app.focused_pane_index;
                            let sw = app.sidebar_width();
                            let (w, h) = app.terminal_size;
                            let cw = w.saturating_sub(sw);
                            let pc = app.panes.len();
                            let pw = if pc > 0 { cw / pc as u16 } else { cw };
                            let pane_area = ratatui::layout::Rect::new(
                                sw + (focused_idx as u16 * pw),
                                1, pw, h.saturating_sub(1)
                            );

                            if let Some(pane) = app.panes.get_mut(focused_idx) {
                                if let Some(preview) = &mut pane.preview {
                                    if let Some(editor) = &mut preview.editor {
                                        editor.cursor_row = std::cmp::min(target, editor.lines.len().saturating_sub(1));
                                        editor.cursor_col = 0;
                                        editor.ensure_cursor_centered(pane_area);
                                    }
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
                                        .map(|p: &std::path::PathBuf| p.to_string_lossy().to_string())
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
                    let max_idx = 6 + total_tabs; // 7 icons (0-6) + tabs

                    match key.code {
                        KeyCode::Esc => {
                            app.mode = AppMode::Normal;
                            return true;
                        }
                        KeyCode::Down => {
                            if idx >= 7 {
                                let target_tab_idx = idx - 7;
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
                                if new_idx >= 7 {
                                    let target_tab_idx = new_idx - 7;
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
                                if new_idx >= 7 {
                                    let target_tab_idx = new_idx - 7;
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
                            if idx <= 6 {
                                match idx {
                                    0 => app.mode = AppMode::Settings,
                                    1 => {
                                        crate::event_helpers::navigate_back(app);
                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(
                                            app.focused_pane_index,
                                        ));
                                    }
                                    2 => {
                                        crate::event_helpers::navigate_forward(app);
                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(
                                            app.focused_pane_index,
                                        ));
                                    }
                                    3 => {
                                        app.toggle_split();
                                        let _ = crate::config::save_state(app);
                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                                    }
                                    4 => {
                                        let _ = event_tx.try_send(AppEvent::SystemMonitor);
                                    }
                                    5 => {
                                        let _ = event_tx.try_send(AppEvent::GitHistory);
                                    }
                                    6 => {
                                        let _ = event_tx.try_send(AppEvent::Editor);
                                    }
                                    _ => {}
                                }
                            } else {
                                // Switch to selected tab
                                let target_tab_idx = idx - 7;
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
                    if matches!(app.mode, AppMode::Viewer) {
                        if key.code == KeyCode::Esc || key.code == KeyCode::Char(' ') {
                            app.mode = AppMode::Normal;
                            app.editor_state = None;
                            return true;
                        }
                    }
                    if key.code == KeyCode::Char('?') || key.code == KeyCode::F(1) {
                        app.previous_mode = app.mode.clone();
                        app.mode = AppMode::Hotkeys;
                        return true;
                    }
                    if let Some(preview) = &mut app.editor_state {
                        if let Some(editor) = &mut preview.editor {
                            if has_control && (key.code == KeyCode::Char('f') || key.code == KeyCode::Char('F')) {
                                app.previous_mode = AppMode::Normal;
                                app.mode = AppMode::EditorSearch;
                                // Pre-fill with current filter if any
                                app.input.set_value(editor.filter_query.clone());
                                return true;
                            }
                            if let KeyCode::Char('r') | KeyCode::Char('R') | KeyCode::F(2) = key.code {
                                if has_control || key.code == KeyCode::F(2) {
                                    app.previous_mode = AppMode::Normal;
                                    app.mode = AppMode::EditorReplace;
                                    app.input.clear();
                                    app.replace_buffer.clear();
                                    let _ = event_tx.try_send(AppEvent::StatusMsg(
                                        "Replace: Type term to FIND, then press Enter/Tab".to_string(),
                                    ));
                                    return true;
                                }
                            }
                            if has_control && (key.code == KeyCode::Char('g') || key.code == KeyCode::Char('G')) {
                                app.previous_mode = AppMode::Normal;
                                app.mode = AppMode::EditorGoToLine;
                                app.input.clear();
                                return true;
                            }

                            let (w, h) = app.terminal_size;
                            let editor_area = ratatui::layout::Rect::new(
                                1,
                                1,
                                w.saturating_sub(2),
                                h.saturating_sub(2),
                            );
                            if has_control {
                                match key.code {
                                    KeyCode::Char('c') | KeyCode::Char('C') => {
                                        if let Some(text) = editor.get_selected_text() {
                                            terma::utils::set_clipboard_text(&text);
                                            return true;
                                        }
                                    }
                                    KeyCode::Char('x') | KeyCode::Char('X') => {
                                        if let Some(text) = editor.get_selected_text() {
                                            terma::utils::set_clipboard_text(&text);
                                        }
                                        editor.handle_event(&evt, editor_area);
                                        return true;
                                    }
                                    KeyCode::Char('v') | KeyCode::Char('V') => {
                                        if let Some(text) = terma::utils::get_clipboard_text() {
                                            editor.insert_string(&text);
                                            return true;
                                        }
                                    }
                                    KeyCode::Char('z') | KeyCode::Char('Z') => {
                                        editor.handle_event(&evt, editor_area);
                                        return true;
                                    }
                                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                                        editor.handle_event(&evt, editor_area);
                                        return true;
                                    }
                                    _ => {}
                                }
                            }
                            editor.handle_event(&evt, editor_area);

                            // AUTO-SYNC SELECTION TO CLIPBOARD
                            if let Some(selected_text) = editor.get_selected_text() {
                                if selected_text.width() > 1 {
                                    app.editor_clipboard = Some(selected_text.clone());
                                    terma::utils::set_clipboard_text(&selected_text);
                                }
                            }
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
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();
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
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();
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
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();
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
                                tools.insert(
                                    0,
                                    crate::config::ExternalTool {
                                        name: cmd.clone(),
                                        command: cmd.clone(),
                                    },
                                );
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
                                    if !fs.selection.is_empty() {
                                        for &idx in fs.selection.multi_selected_indices() {
                                            if let Some(p) = fs.files.get(idx) {
                                                paths.push(p.clone());
                                            }
                                        }
                                    } else if let Some(idx) = fs.selection.selected {
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
                            SettingsSection::General => 5, // 6 items: 0-5
                            SettingsSection::Columns => 3, // 4 items: 0-3
                            SettingsSection::Tabs => app.panes.iter().map(|p| p.tabs.len()).sum::<usize>().saturating_sub(1),
                            SettingsSection::Remotes => app.remote_bookmarks.len().saturating_sub(1),
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
                                    3 => app.semantic_coloring = !app.semantic_coloring,
                                    4 => app.auto_save = !app.auto_save,
                                    5 => {
                                        app.icon_mode = match app.icon_mode {
                                            IconMode::Nerd => IconMode::Unicode,
                                            IconMode::Unicode => IconMode::ASCII,
                                            IconMode::ASCII => IconMode::Nerd,
                                        }
                                    }
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
                            SettingsSection::Tabs => {
                                let mut tab_counter = 0;
                                let mut to_remove = None;
                                for (p_idx, pane) in app.panes.iter().enumerate() {
                                    for t_idx in 0..pane.tabs.len() {
                                        if tab_counter == app.settings_index {
                                            to_remove = Some((p_idx, t_idx));
                                            break;
                                        }
                                        tab_counter += 1;
                                    }
                                    if to_remove.is_some() { break; }
                                }
                                if let Some((p_idx, t_idx)) = to_remove {
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
                            }
                            SettingsSection::Remotes => {
                                if let Some(_bookmark) = app.remote_bookmarks.get(app.settings_index) {
                                    let _ = event_tx.try_send(AppEvent::ConnectToRemote(app.focused_pane_index, app.settings_index));
                                    app.mode = AppMode::Normal;
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
                    KeyCode::Char('s') if app.settings_section == SettingsSection::General => {
                        app.semantic_coloring = !app.semantic_coloring;
                        let _ = crate::config::save_state(app);
                        return true;
                    }
                    KeyCode::Char('d') if app.settings_section == SettingsSection::General => {
                        app.confirm_delete = !app.confirm_delete;
                        let _ = crate::config::save_state(app);
                        return true;
                    }
                    KeyCode::Char('a') if app.settings_section == SettingsSection::General => {
                        app.auto_save = !app.auto_save;
                        let _ = crate::config::save_state(app);
                        return true;
                    }
                    KeyCode::Char('s') if app.settings_section == SettingsSection::Columns => {
                        app.toggle_column(crate::app::FileColumn::Size);
                        return true;
                    }
                    KeyCode::Char('m') if app.settings_section == SettingsSection::Columns => {
                        app.toggle_column(crate::app::FileColumn::Modified);
                        return true;
                    }
                    KeyCode::Char('c') if app.settings_section == SettingsSection::Columns => {
                        app.toggle_column(crate::app::FileColumn::Created);
                        return true;
                    }
                    KeyCode::Char('p') if app.settings_section == SettingsSection::Columns => {
                        app.toggle_column(crate::app::FileColumn::Permissions);
                        return true;
                    }
                    _ => return false,
                },
                AppMode::NewFile | AppMode::NewFolder | AppMode::Rename | AppMode::Delete | AppMode::DeleteFile(_) => {
                    match key.code {
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
                            
                            // Special case for DeleteFile (no current_file_state needed for path)
                            if let AppMode::DeleteFile(ref path) = app.mode {
                                let ic = input.trim().to_lowercase();
                                if ic == "y" || ic == "yes" || ic.is_empty() || !app.confirm_delete {
                                    let path = path.clone();
                                    let _ = event_tx.try_send(AppEvent::Delete(path));
                                    app.mode = AppMode::Normal;
                                    app.current_view = CurrentView::Files;
                                    // Clear editors
                                    for pane in &mut app.panes {
                                        pane.preview = None;
                                    }
                                    app.editor_state = None;
                                } else {
                                    app.mode = AppMode::Normal;
                                }
                                app.input.clear();
                                return true;
                            }

                            if let Some(fs) = app.current_file_state() {
                                let path = fs.current_path.join(&input);
                                crate::app::log_debug(&format!(
                                    "Action {:?} triggered for path: {:?}",
                                    app.mode, path
                                ));
                                match app.mode {
                                    AppMode::NewFile => {
                                        let _ = event_tx.try_send(AppEvent::CreateFile(path));
                                    }
                                    AppMode::NewFolder => {
                                        let _ = event_tx.try_send(AppEvent::CreateFolder(path));
                                    }
                                    AppMode::Rename => {
                                        if let Some(idx) = fs.selection.selected {
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
                                            if !fs.selection.is_empty() {
                                                for &idx in fs.selection.multi_selected_indices() {
                                                    if let Some(p) = fs.files.get(idx) {
                                                        paths.push(p.clone());
                                                    }
                                                }
                                            } else if let Some(idx) = fs.selection.selected {
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
                                crate::app::log_debug(
                                    "Failed to get current file state during action.",
                                );
                            }
                            app.mode = AppMode::Normal;
                            app.input.clear();
                            app.rename_selected = false;
                            return true;
                        }
                        KeyCode::Char(c)
                            if app.mode == AppMode::Rename
                                && app.rename_selected
                                && !has_control
                                && !has_alt =>
                        {
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
                        KeyCode::Backspace
                            if app.mode == AppMode::Rename
                                && app.rename_selected
                                && !has_control =>
                        {
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
                                if let KeyCode::Left
                                | KeyCode::Right
                                | KeyCode::Home
                                | KeyCode::End = key.code
                                {
                                    app.rename_selected = false;
                                }
                            }
                            return res;
                        }
                    }
                }
                _ => {
                    // Standard Navigation & Actions
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
                            fs.selection.clear_multi();
                            fs.selection.anchor = None;
                            if !fs.search_filter.is_empty() {
                                fs.search_filter.clear();
                                fs.selection.selected = Some(0);
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
                            if app.current_view != CurrentView::Editor {
                                if let Some(fs) = app.current_file_state() {
                                    if let Some(idx) = fs.selection.selected {
                                        if let Some(path) = fs.files.get(idx) {
                                            app.clipboard =
                                                Some((path.clone(), crate::app::ClipboardOp::Copy));
                                        }
                                    }
                                }
                            }
                            return true;
                        }
                        KeyCode::Char('x') if has_control => {
                            if app.current_view != CurrentView::Editor {
                                if let Some(fs) = app.current_file_state() {
                                    if let Some(idx) = fs.selection.selected {
                                        if let Some(path) = fs.files.get(idx) {
                                            app.clipboard =
                                                Some((path.clone(), crate::app::ClipboardOp::Cut));
                                        }
                                    }
                                }
                            }
                            return true;
                        }
                        KeyCode::Char('v') if has_control => {
                            if app.current_view != CurrentView::Editor {
                                if let Some((src, op)) = app.clipboard.clone() {
                                    if let Some(fs) = app.current_file_state() {
                                        let dest = fs.current_path.join(
                                            src.file_name()
                                                .unwrap_or_else(|| std::ffi::OsStr::new("root")),
                                        );
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
                            }
                            return true;
                        }
                        KeyCode::Char('a') if has_control => {
                            if app.current_view != CurrentView::Editor {
                                if let Some(fs) = app.current_file_state_mut() {
                                    fs.selection.select_all(fs.files.len());
                                }
                            }
                            return true;
                        }
                        KeyCode::Char('z') if has_control => {
                            if app.current_view != CurrentView::Editor {
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
                                        panes_needing_refresh.insert(i);
                                    }
                                } else if let Some(fs) = app.current_file_state_mut() {
                                    if !fs.search_filter.is_empty() {
                                        fs.search_filter.clear();
                                        let _ = event_tx
                                            .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                    }
                                }
                            }
                            return true;
                        }
                        KeyCode::Char('y') if has_control => {
                            if app.current_view != CurrentView::Editor {
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
                                        panes_needing_refresh.insert(i);
                                    }
                                }
                            }
                            return true;
                        }
                        KeyCode::Char('f') if has_control => {
                            app.mode = AppMode::Search;
                            return true;
                        }
                        KeyCode::Insert => {
                            let mut should_save = false;
                            if let Some(fs) = app.current_file_state_mut() {
                                if let Some(idx) = fs.selection.selected {
                                    fs.selection.toggle(idx);
                                    should_save = true;
                                    // Move down after toggle
                                    if idx < fs.files.len().saturating_sub(1) {
                                        let next_idx = idx + 1;
                                        fs.selection.selected = Some(next_idx);
                                        fs.selection.anchor = Some(next_idx);
                                        fs.table_state.select(Some(next_idx));
                                        if next_idx >= fs.table_state.offset() + fs.view_height {
                                            *fs.table_state.offset_mut() = next_idx.saturating_sub(fs.view_height - 1);
                                        }
                                    }
                                }
                            }
                            if should_save {
                                let _ = crate::config::save_state(app);
                            }
                            return true;
                        }
                        KeyCode::Char(' ') => {
                            if let Some(fs) = app.current_file_state_mut() {
                                let _new_idx = match fs.selection.selected {
                                    Some(sel) if sel > 0 => sel - 1,
                                    _ => 0,
                                };
                                if fs.selection.selected.is_none() && !fs.files.is_empty() {
                                    fs.selection.selected = Some(0);
                                    fs.table_state.select(Some(0));
                                    fs.selection.anchor = Some(0);
                                }

                                if let Some(idx) = fs.selection.selected {
                                    if let Some(path) = fs.files.get(idx).cloned() {
                                        let is_dir = path.is_dir();
                                        if is_dir {
                                            // Show folder stats/properties instead of switching to editor
                                            app.mode = AppMode::Properties;
                                        } else {
                                            // SMART TARGETING: If we are going to collapse to single pane, target pane 0.
                                            let mut target_pane = app.focused_pane_index;
                                            let will_go_single = !app.view_prefs.editor.is_split_mode || (app.is_split_mode && {
                                                let other_idx = if app.focused_pane_index == 0 { 1 } else { 0 };
                                                app.panes.get(other_idx).map(|p| p.preview.is_none()).unwrap_or(true)
                                            });

                                            if will_go_single {
                                                target_pane = 0;
                                            }

                                            // Request preview in the determined target pane
                                            let _ = event_tx.try_send(AppEvent::PreviewRequested(
                                                target_pane,
                                                path,
                                            ));
                                            
                                            app.save_current_view_prefs();
                                            app.current_view = CurrentView::Editor;
                                            app.load_view_prefs(CurrentView::Editor);

                                            // Smart Single Panel: If we are in split mode but other pane is empty, go single.
                                            if app.is_split_mode {
                                                let other_idx = if app.focused_pane_index == 0 { 1 } else { 0 };
                                                if let Some(other_pane) = app.panes.get(other_idx) {
                                                    if other_pane.preview.is_none() {
                                                        app.apply_split_mode(false);
                                                        app.save_current_view_prefs();
                                                    }
                                                }
                                            }

                                            // If we collapsed to single, ensure focus is on 0
                                            if app.panes.len() == 1 {
                                                app.focused_pane_index = 0;
                                            }

                                            app.sidebar_focus = false;
                                        }
                                    }
                                }
                            }
                            return true;
                        }
                        KeyCode::Up => {
                            let shift = key.modifiers.contains(KeyModifiers::SHIFT);
                            app.move_up(shift);
                            return true;
                        }
                        KeyCode::Down => {
                            let shift = key.modifiers.contains(KeyModifiers::SHIFT);
                            app.move_down(shift);
                            return true;
                        }
                        KeyCode::Left => {
                            if key.modifiers.contains(KeyModifiers::SHIFT) && !app.sidebar_focus {
                                let other_pane_idx =
                                    if app.focused_pane_index == 0 { 1 } else { 0 };
                                if let Some(dest_path) = app
                                    .panes
                                    .get(other_pane_idx)
                                    .and_then(|p| p.current_state())
                                    .map(|fs| fs.current_path.clone())
                                {
                                     if let Some(fs) = app.current_file_state() {
                                         let mut paths = Vec::new();
                                         if !fs.selection.is_empty() {
                                             for &idx in fs.selection.multi_selected_indices() {
                                                 if let Some(p) = fs.files.get(idx) {
                                                     paths.push(p.clone());
                                                 }
                                             }
                                         } else if let Some(idx) = fs.selection.selected {
                                             if let Some(p) = fs.files.get(idx) {
                                                 paths.push(p.clone());
                                             }
                                         }
                                         for p in paths {
                                             let dest =
                                                 dest_path.join(p.file_name().unwrap_or_else(
                                                     || std::ffi::OsStr::new("root"),
                                                 ));
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
                                let other_pane_idx =
                                    if app.focused_pane_index == 0 { 1 } else { 0 };
                                if let Some(dest_path) = app
                                    .panes
                                    .get(other_pane_idx)
                                    .and_then(|p| p.current_state())
                                    .map(|fs| fs.current_path.clone())
                                {
                                     if let Some(fs) = app.current_file_state() {
                                         let mut paths = Vec::new();
                                         if !fs.selection.is_empty() {
                                             for &idx in fs.selection.multi_selected_indices() {
                                                 if let Some(p) = fs.files.get(idx) {
                                                     paths.push(p.clone());
                                                 }
                                             }
                                         } else if let Some(idx) = fs.selection.selected {
                                             if let Some(p) = fs.files.get(idx) {
                                                 paths.push(p.clone());
                                             }
                                         }
                                         for p in paths {
                                             let dest =
                                                 dest_path.join(p.file_name().unwrap_or_else(
                                                     || std::ffi::OsStr::new("root"),
                                                 ));
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
                            if app.current_view == CurrentView::Editor && app.sidebar_focus {
                                let target_opt = app
                                    .sidebar_bounds
                                    .iter()
                                    .find(|b| b.index == app.sidebar_index)
                                    .map(|b| b.target.clone());

                                if let Some(SidebarTarget::Project(path)) = target_opt {
                                    if path.is_dir() {
                                        if app.expanded_folders.contains(&path) {
                                            app.expanded_folders.remove(&path);
                                        } else {
                                            app.expanded_folders.insert(path);
                                        }
                                    } else {
                                        let _ = event_tx.try_send(AppEvent::PreviewRequested(
                                            app.focused_pane_index,
                                            path,
                                        ));
                                        app.sidebar_focus = false;
                                    }
                                }
                                return true;
                            }
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
                                                 fs.selection.selected = Some(0);
                                                 fs.selection.anchor = Some(0);
                                                 fs.selection.clear_multi();
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
                                                         fs.selection.selected = Some(0);
                                                         fs.selection.anchor = Some(0);
                                                         fs.selection.clear_multi();
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
                                 if let Some(idx) = fs.selection.selected {
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
                                // Save selection for current folder before leaving
                                 if let Some(fs) = app.current_file_state() {
                                     let path = fs.current_path.clone();
                                     let idx = fs.selection.selected.unwrap_or(0);
                                     app.folder_selections.insert(path, idx);
                                 }

                                 if let Some(fs) = app.current_file_state_mut() {
                                     fs.current_path = p.clone();
                                     fs.selection.selected = Some(0); // Reset selection to top (Task 4)
                                     fs.selection.anchor = Some(0);
                                     fs.selection.clear_multi();
                                     fs.search_filter.clear();
                                     *fs.table_state.offset_mut() = 0;
                                     crate::event_helpers::push_history(fs, p);
                                     let _ = event_tx
                                         .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                 }
                            }
                            return true;
                        }
                        KeyCode::F(2) => {
                             app.selection_mode = !app.selection_mode;
                             if !app.selection_mode {
                                 if let Some(fs) = app.current_file_state_mut() {
                                     fs.selection.clear_multi();
                                 }
                             }
                            return true;
                        }
                        KeyCode::F(6) => {
                            let mut to_rename = None;
                            if let Some(fs) = app.current_file_state() {
                                if let Some(p) = fs.selection.selected.and_then(|idx| fs.files.get(idx))
                                {
                                    to_rename = Some(
                                        p.file_name()
                                            .unwrap_or_else(|| std::ffi::OsStr::new("root"))
                                            .to_string_lossy()
                                            .to_string(),
                                    );
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
                                if fs.selection.selected.is_some() {
                                    if app.confirm_delete {
                                        app.mode = AppMode::Delete;
                                    } else {
                                        let mut paths = Vec::new();
                                        if !fs.selection.is_empty() {
                                            for &idx in fs.selection.multi_selected_indices() {
                                                if let Some(p) = fs.files.get(idx) {
                                                    paths.push(p.clone());
                                                }
                                            }
                                        } else if let Some(idx) = fs.selection.selected {
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
                                    fs.selection.selected = Some(0);
                                    fs.selection.anchor = Some(0);
                                    fs.selection.clear_multi();
                                    *fs.table_state.offset_mut() = 0;
                                    crate::event_helpers::push_history(fs, home);
                                    let _ = event_tx
                                        .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                    return true;
                                }
                            }
                            return false;
                        }
                        KeyCode::Char('r') if key.modifiers.is_empty() => {
                            let mut to_rename = None;
                            if let Some(fs) = app.current_file_state() {
                                if let Some(p) = fs.selection.selected.and_then(|idx| fs.files.get(idx))
                                {
                                    to_rename = Some(
                                        p.file_name()
                                            .unwrap_or_else(|| std::ffi::OsStr::new("root"))
                                            .to_string_lossy()
                                            .to_string(),
                                    );
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
                        KeyCode::Char(c) if key.modifiers.is_empty() => {
                            if (c as u32) < 32 || c == '\x7f' || c == '\x1b' {
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
                                fs.selection.selected = Some(0);
                                fs.selection.anchor = Some(0);
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

                            let mut handled_search = false;
                            if let Some(fs) = app.current_file_state_mut() {
                                if !fs.search_filter.is_empty() {
                                    fs.search_filter.pop();
                                    fs.selection.selected = Some(0);
                                    fs.selection.anchor = Some(0);
                                    *fs.table_state.offset_mut() = 0;
                                    let _ = event_tx
                                        .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                    handled_search = true;
                                }
                            }

                            if !handled_search {
                                crate::event_helpers::navigate_up(app);
                                let _ = event_tx
                                    .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
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
                                fs.selection.selected = Some(0);
                                *fs.table_state.offset_mut() = 0;
                                let _ = event_tx
                                    .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
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
                                fs.selection.selected = Some(0);
                                *fs.table_state.offset_mut() = 0;
                                let _ = event_tx
                                    .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
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
                                fs.selection.selected = Some(0);
                                fs.selection.anchor = Some(0);
                                *fs.table_state.offset_mut() = 0;
                                let _ = event_tx
                                    .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
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
                                        if !fs.selection.is_empty() {
                                            for &idx in fs.selection.multi_selected_indices() {
                                                if let Some(p) = fs.files.get(idx) {
                                                    paths.push(p.clone());
                                                }
                                            }
                                        } else if let Some(idx) = fs.selection.selected {
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
                    if let MouseEventKind::Moved | MouseEventKind::Drag(_) | MouseEventKind::Up(_) = me.kind
                    {
                        return true; // Trap movement and release
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
                                    if let AppMode::ContextMenu {
                                        selected_index: ref mut si,
                                        ..
                                    } = app.mode
                                    {
                                        *si = new_idx;
                                    }
                                }
                            } else {
                                if let AppMode::ContextMenu {
                                    selected_index: ref mut si,
                                    ..
                                } = app.mode
                                {
                                    *si = None;
                                }
                            }
                        } else {
                            if let AppMode::ContextMenu {
                                selected_index: ref mut si,
                                ..
                            } = app.mode
                            {
                                *si = None;
                            }
                        }
                        return true; // Trap movement
                    }
                }
                AppMode::DragDropMenu { sources, target } => {
                    match me.kind {
                        MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                            return true; // Trap movement
                        }
                        MouseEventKind::Down(button) => {
                            let area = terma::layout::centered_rect(60, 20, ratatui::layout::Rect::new(0, 0, w, h));
                            let inner = ratatui::widgets::Block::default().borders(ratatui::widgets::Borders::ALL).inner(area);
                            
                            if column >= area.x && column < area.x + area.width && row >= area.y && row < area.y + area.height {
                                if button == MouseButton::Left {
                                    let button_y_offset = if sources.len() == 1 {
                                        3
                                    } else {
                                        let display_count = std::cmp::min(sources.len(), 3);
                                        let mut offset = 1 + display_count;
                                        if sources.len() > 3 {
                                            offset += 1;
                                        }
                                        offset + 2
                                    };

                                    if row == inner.y + button_y_offset as u16 {
                                        let rel_x = column.saturating_sub(inner.x);
                                        // [C] Copy (0-10) [M] Move (12-22) [L] Link (24-34) [Esc] Cancel (36-50)
                                        if rel_x < 10 {
                                            for source in sources {
                                                let dest =
                                                    target.join(source.file_name().unwrap_or_else(
                                                        || std::ffi::OsStr::new("root"),
                                                    ));
                                                let _ = event_tx
                                                    .try_send(AppEvent::Copy(source.clone(), dest));
                                            }
                                            app.mode = AppMode::Normal;
                                        } else if rel_x >= 12 && rel_x < 22 {
                                            for source in sources {
                                                let dest =
                                                    target.join(source.file_name().unwrap_or_else(
                                                        || std::ffi::OsStr::new("root"),
                                                    ));
                                                let _ = event_tx.try_send(AppEvent::Rename(
                                                    source.clone(),
                                                    dest,
                                                ));
                                            }
                                            if let Some(fs) = app.current_file_state_mut() {
                                                fs.selection.clear_multi();
                                                fs.selection.anchor = None;
                                            }
                                            app.mode = AppMode::Normal;
                                        } else if rel_x >= 24 && rel_x < 34 {
                                            for source in sources {
                                                let dest =
                                                    target.join(source.file_name().unwrap_or_else(
                                                        || std::ffi::OsStr::new("root"),
                                                    ));
                                                let _ = event_tx.try_send(AppEvent::Symlink(
                                                    source.clone(),
                                                    dest,
                                                ));
                                            }
                                            app.mode = AppMode::Normal;
                                        } else if rel_x >= 36 && rel_x < 50 {
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

                                        let ext = path
                                            .extension()
                                            .and_then(|e| e.to_str())
                                            .unwrap_or("")
                                            .to_lowercase();
                                        let mut suggestions =
                                            terma::utils::get_open_with_suggestions(&ext);

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
                                            suggestions
                                                .retain(|s| s.to_lowercase().contains(&query));
                                        }

                                        if let Some(s) = suggestions.get(rel_y as usize) {
                                            let cmd = s.to_string();

                                            // Persist this choice
                                            let tools =
                                                app.external_tools.entry(ext.clone()).or_default();
                                            if !tools.iter().any(|t| t.command == cmd) {
                                                tools.insert(
                                                    0,
                                                    crate::config::ExternalTool {
                                                        name: cmd.clone(),
                                                        command: cmd.clone(),
                                                    },
                                                );
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
                | AppMode::Editor
                | AppMode::EditorSearch
                | AppMode::EditorReplace
                | AppMode::EditorGoToLine
                | AppMode::Viewer => {
                    if let AppMode::Editor | AppMode::Viewer | AppMode::EditorSearch | AppMode::EditorReplace | AppMode::EditorGoToLine = app.mode {
                        if let Some(preview) = &mut app.editor_state {
                            if let Some(editor) = &mut preview.editor {
                                let editor_area = ratatui::layout::Rect::new(
                                    1,
                                    1,
                                    w.saturating_sub(2),
                                    h.saturating_sub(2),
                                );

                                if let MouseEventKind::Down(MouseButton::Left) = me.kind {
                                    // 1. Header Button Handling
                                    if row == 0 {
                                        if column >= w.saturating_sub(10) {
                                            app.running = false;
                                            return true;
                                        } else if column >= w.saturating_sub(20) {
                                            app.mode = AppMode::Normal;
                                            app.editor_state = None;
                                            return true;
                                        }
                                        return true; // Trap all header clicks
                                    }

                                    let now = std::time::Instant::now();
                                    if now.duration_since(app.mouse_last_click)
                                        < std::time::Duration::from_millis(500)
                                        && app.mouse_click_pos == (column, row)
                                    {
                                        app.mouse_click_count += 1;
                                    } else {
                                        app.mouse_click_count = 1;
                                    }

                                    // Fix: Editor area starts at y=1 (header), plus 1 for border/breadcrumbs = 2
                                    // So content starts at y=2 effectively. 
                                    // If we use editor_area.y which is 1, we might need to subtract 1 more if the widget has padding.
                                    // Let's assume the widget handles its own local coordinates if we pass the correct area.
                                    // But here we are doing logic OUTSIDE the widget to detect double/triple clicks.
                                    
                                    // The TextEditor widget renders content starting at area.y.
                                    // In draw_editor_view -> draw_pane_editor:
                                    // Header is at 0.
                                    // Pane starts at 1 (global header).
                                    // Inside Pane: Breadcrumbs at 0 (relative to pane), so y=1 absolute.
                                    // Editor Area starts at 1 (relative to pane), so y=2 absolute.
                                    
                                    // We passed editor_area as Rect(1, 1, ...). Wait, no.
                                    // In `draw` (Files/Global): Editor takes full screen. Header at 0. Editor widget at 1.
                                    // In `draw` (IDE): Global Header at 0. Pane Breadcrumbs at 1. Editor content at 2.
                                    
                                    // The logic here (lines 3647) seems to assume `editor_area` is defined as:
                                    // let editor_area = ratatui::layout::Rect::new(1, 1, ...);
                                    // This matches the "Global Editor" mode (AppMode::Editor).
                                    
                                    // BUT the user complains about "editor page" which likely means IDE mode (CurrentView::Editor).
                                    // Ah, I see `AppMode::Editor | AppMode::Viewer` block above.
                                    // And a separate `if app.current_view == CurrentView::Editor` block earlier (around line 1170).
                                    // I need to check the EARLIER block for IDE mode mouse handling.
                                    
                                    // Wait, I missed the IDE mode mouse handling in the previous read?
                                    // Let me search for it.
                                    
                                    let rel_row = (row - editor_area.y) as usize;
                                    let target_row = editor.scroll_row + rel_row;

                                    match app.mouse_click_count {
                                        2 => {
                                            // Double click: Select Word
                                            if target_row < editor.lines.len() {
                                                let gutter = if editor.show_line_numbers {
                                                    let total_lines = editor.lines.len();
                                                    if total_lines < 100 {
                                                        3
                                                    } else if total_lines < 1000 {
                                                        4
                                                    } else if total_lines < 10000 {
                                                        5
                                                    } else {
                                                        6
                                                    }
                                                } else {
                                                    0
                                                };

                                                if column >= editor_area.x + gutter as u16 {
                                                    let rel_col =
                                                        (column - editor_area.x - gutter as u16)
                                                            as usize;
                                                    let target_visual = editor.scroll_col + rel_col;
                                                    let byte_col = editor
                                                        .get_byte_index_from_visual(
                                                            target_row,
                                                            target_visual,
                                                        );
                                                    editor.select_word_at(target_row, byte_col);
                                                }
                                            }
                                        }
                                        3 => {
                                            // Triple click: Select Line
                                            if target_row < editor.lines.len() {
                                                editor.select_line_at(target_row);
                                            }
                                            app.mouse_click_count = 0; // Reset after triple
                                        }
                                        _ => {
                                            editor.handle_mouse_event(me, editor_area);
                                        }
                                    }

                                    app.mouse_last_click = now;
                                    app.mouse_click_pos = (column, row);
                                } else {
                                    editor.handle_mouse_event(me, editor_area);
                                }

                                // AUTO-SYNC SELECTION TO CLIPBOARD (on every mouse event in editor)
                                if let Some(selected_text) = editor.get_selected_text() {
                                    if selected_text.width() > 1 {
                                        app.editor_clipboard = Some(selected_text.clone());
                                        terma::utils::set_clipboard_text(&selected_text);
                                    }
                                }
                            }
                        }
                        return true;
                    }

                    match me.kind {
                        MouseEventKind::Moved | MouseEventKind::Drag(_) | MouseEventKind::Up(_) => {
                            return true; // Trap movement and release in modals
                        }
                        MouseEventKind::Down(_button) => {
                            if let AppMode::Settings = app.mode {
                                // Full Screen Settings handling
                                if row == 0 {
                                    if column >= w.saturating_sub(10) {
                                        app.mode = AppMode::Normal;
                                        return true;
                                    }
                                }

                                let inner_x = 1; // Border(1)
                                let inner_y = 1; // Border(1)
                                
                                if column < inner_x + 20 { // Section list width is now 20
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
                                            // General uses Table starting at y=2 (Border 1 + Header 1)
                                            let rel_y = row.saturating_sub(inner_y + 1);
                                            if rel_y <= 5 { // 6 items: 0-5
                                                app.settings_index = rel_y as usize;
                                                match rel_y {
                                                    0 => app.default_show_hidden = !app.default_show_hidden,
                                                    1 => app.confirm_delete = !app.confirm_delete,
                                                    2 => app.smart_date = !app.smart_date,
                                                    3 => app.semantic_coloring = !app.semantic_coloring,
                                                    4 => app.auto_save = !app.auto_save,
                                                    5 => {
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
                                            // Columns uses Tabs at y=1, List at y=4
                                            if row >= inner_y && row < inner_y + 3 {
                                                let cx = column.saturating_sub(inner_x + 20);
                                                if cx < 12 {
                                                    app.settings_target = SettingsTarget::SingleMode;
                                                } else if cx < 25 {
                                                    app.settings_target = SettingsTarget::SplitMode;
                                                }
                                            } else if row >= inner_y + 4 {
                                                let ry = row.saturating_sub(inner_y + 4);
                                                if ry <= 3 {
                                                    app.settings_index = ry as usize;
                                                    let col = match ry {
                                                        0 => crate::app::FileColumn::Size,
                                                        1 => crate::app::FileColumn::Modified,
                                                        2 => crate::app::FileColumn::Created,
                                                        3 => crate::app::FileColumn::Permissions,
                                                        _ => crate::app::FileColumn::Name,
                                                    };
                                                    if col != crate::app::FileColumn::Name {
                                                        app.toggle_column(col);
                                                    }
                                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                return true;
                            }

                            let (aw, ah) = match app.mode {
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
                                if let AppMode::Properties = app.mode {
                                    // Click inside properties?
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
                    // --- FULL SCREEN VIEW PRIORITIES ---
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
                        // Content area typically starts at y=6 (1 header + 1 border + 3 nav + 1 margin)
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
                        return true; // Trap all clicks in full screen monitor
                    }

                    if app.current_view == CurrentView::Git {
                        if row >= 2 { // Header(1) + border(1) = 2
                            if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                                if let Some(tab) = pane.tabs.get_mut(pane.active_tab_index) {
                                    let scroll_offset = tab.git_history_state.offset();
                                    let rel_row = (row - 2) as usize + scroll_offset;
                                    if rel_row < tab.git_history.len() {
                                        tab.git_history_state.select(Some(rel_row));
                                        return true;
                                    }
                                }
                            }
                        }
                        return true; // Trap all clicks in full screen git
                    }

                    let sw = app.sidebar_width();

                    if button == MouseButton::Left
                        && column >= sw.saturating_sub(1)
                        && column <= sw + 1
                    {
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
                                    crate::event_helpers::navigate_back(app);
                                    let _ = event_tx
                                        .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                }
                                "forward" => {
                                    crate::event_helpers::navigate_forward(app);
                                    let _ = event_tx
                                        .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                }
                                "split" => {
                                    app.toggle_split();
                                    app.save_current_view_prefs();
                                    let _ = crate::config::save_state(app);
                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                                }
                                "burger" => {
                                    app.save_current_view_prefs();
                                    app.mode = AppMode::Settings;
                                    app.settings_scroll = 0;
                                }
                                "monitor" => {
                                    let _ = event_tx.try_send(AppEvent::StatusMsg(
                                        "Launching System Monitor...".to_string(),
                                    ));
                                    let _ = event_tx.try_send(AppEvent::SystemMonitor);
                                }
                                "git" => {
                                    let _ = event_tx.try_send(AppEvent::GitHistory);
                                }
                                "project" => {
                                    let _ = event_tx.try_send(AppEvent::Editor);
                                }
                                _ => {}
                            }
                            app.sidebar_focus = false;
                            return true;
                        }
                    }

                    // Tabs
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
                                     nfs.selection.selected = Some(0);
                                     nfs.selection.anchor = Some(0);
                                     nfs.selection.clear_multi();
                                     nfs.search_filter.clear();
                                     *nfs.table_state.offset_mut() = 0;
                                     nfs.history = vec![path];
                                     nfs.history_index = 0;
                                     pane.open_tab(nfs);
                                } else {
                                     if path.is_file() {
                                         let _ = event_tx.try_send(AppEvent::PreviewRequested(p_idx, path.clone()));
                                         app.save_current_view_prefs();
                                         app.current_view = CurrentView::Editor;
                                         app.load_view_prefs(CurrentView::Editor);
                                     } else {
                                         fs.current_path = path.clone();
                                         fs.selection.selected = Some(0);
                                         fs.selection.anchor = Some(0);
                                         fs.selection.clear_multi();
                                         fs.search_filter.clear();
                                         *fs.table_state.offset_mut() = 0;
                                         crate::event_helpers::push_history(fs, path);
                                     }
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

                    // IDE/Editor Mode clicks
                    if app.current_view == CurrentView::Editor && column >= sw && (matches!(app.mode, AppMode::Normal) || matches!(app.mode, AppMode::EditorSearch) || matches!(app.mode, AppMode::EditorReplace) || matches!(app.mode, AppMode::EditorGoToLine)) {
                        let cw = w.saturating_sub(sw);
                        let pc = app.panes.len();
                        let pw = if pc > 0 { cw / pc as u16 } else { cw };
                        let cp = (column.saturating_sub(sw) / pw) as usize;
                        if cp < pc {
                            app.focused_pane_index = cp;
                            app.sidebar_focus = false;

                            // Pass event to TextEditor
                            if let Some(pane) = app.panes.get_mut(cp) {
                                if let Some(preview) = &mut pane.preview {
                                    if let Some(editor) = &mut preview.editor {
                                        // Area calculation for TextEditor mouse handling:
                                        // sw: sidebar width
                                        // cp: current pane index
                                        // pw: pane width
                                        // +1 for left rounded border
                                        // y=3: Global Header(1) + Rounded Border(1) + Breadcrumbs(1)
                                        let pane_area = ratatui::layout::Rect::new(
                                            sw + (cp as u16 * pw) + 1,
                                            3, 
                                            pw.saturating_sub(2), // Left/Right borders
                                            h.saturating_sub(4), // Header(1) + Top/Bottom borders(2) + Breadcrumbs(1)
                                        );

                                        if let MouseEventKind::Down(MouseButton::Left) = me.kind {
                                            let now = std::time::Instant::now();
                                            if now.duration_since(app.mouse_last_click) < std::time::Duration::from_millis(500)
                                                && app.mouse_click_pos == (column, row) {
                                                app.mouse_click_count += 1;
                                            } else {
                                                app.mouse_click_count = 1;
                                            }

                                            let rel_row = (row - pane_area.y) as usize;
                                            let target_row = editor.scroll_row + rel_row;

                                            match app.mouse_click_count {
                                                2 => {
                                                    if target_row < editor.lines.len() {
                                                        let gutter = editor.gutter_width();
                                                        if column >= pane_area.x + gutter as u16 {
                                                            let rel_col = (column - pane_area.x - gutter as u16) as usize;
                                                            let target_visual = editor.scroll_col + rel_col;
                                                            let byte_col = editor.get_byte_index_from_visual(target_row, target_visual);
                                                            editor.select_word_at(target_row, byte_col);
                                                        }
                                                    }
                                                }
                                                3 => {
                                                    if target_row < editor.lines.len() {
                                                        editor.select_line_at(target_row);
                                                    }
                                                    app.mouse_click_count = 0;
                                                }
                                                _ => {
                                                    editor.handle_mouse_event(me, pane_area);
                                                }
                                            }
                                            app.mouse_last_click = now;
                                            app.mouse_click_pos = (column, row);
                                        } else {
                                            editor.handle_mouse_event(me, pane_area);
                                        }

                                        // AUTO-SYNC SELECTION TO CLIPBOARD
                                        if let Some(selected_text) = editor.get_selected_text() {
                                            if selected_text.width() > 1 {
                                                app.editor_clipboard = Some(selected_text.clone());
                                                terma::utils::set_clipboard_text(&selected_text);
                                            }
                                        }
                                    }
                                }
                            }
                            return true;
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
                                             fs.selection.selected = Some(0);
                                             fs.selection.anchor = Some(0);
                                             fs.selection.clear_multi();
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
                                    SidebarTarget::Project(path) => {
                                        if path.is_dir() {
                                            if app.expanded_folders.contains(path) {
                                                app.expanded_folders.remove(path);
                                            } else {
                                                app.expanded_folders.insert(path.clone());
                                            }
                                        } else {
                                            let _ = event_tx.try_send(AppEvent::PreviewRequested(
                                                app.focused_pane_index,
                                                path.clone(),
                                            ));
                                            app.sidebar_focus = false;
                                        }
                                    }
                                    SidebarTarget::Disk(name) => {
                                        if let Some(disk) =
                                            app.system_state.disks.iter().find(|d| d.name == *name)
                                        {
                                            if disk.is_mounted {
                                                let mp = PathBuf::from(&disk.name);
                                                 if let Some(fs) = app.current_file_state_mut() {
                                                     fs.current_path = mp.clone();
                                                     fs.selection.selected = Some(0);
                                                     fs.selection.anchor = Some(0);
                                                     fs.selection.clear_multi();
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

                    if row >= 1 {
                        // Breadcrumb Click Check
                        if let Some(fs) = app.current_file_state_mut() {
                            if let Some((_, path)) = fs.breadcrumb_bounds.iter().find(|(r, _)| {
                                r.contains(ratatui::layout::Position { x: column, y: row })
                            }) {
                                let target_path = path.clone();
                                let current_path = fs.current_path.clone();

                                // Smart Selection: If target is ancestor of current, select the child leading to current
                                if current_path.starts_with(&target_path)
                                    && current_path != target_path
                                {
                                    if let Ok(prefix) = current_path.strip_prefix(&target_path) {
                                        if let Some(component) = prefix.components().next() {
                                            let child_name = component.as_os_str();
                                            let pending = target_path.join(child_name);
                                            fs.pending_select_path = Some(pending);
                                        }
                                    }
                                }

                                fs.current_path = target_path.clone();
                                fs.selection.selected = Some(0);
                                fs.selection.anchor = Some(0);
                                fs.selection.clear_multi();
                                fs.search_filter.clear();
                                *fs.table_state.offset_mut() = 0;
                                crate::event_helpers::push_history(fs, target_path);
                                let _ = event_tx
                                    .try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                return true;
                            }
                        }

                        if row >= 3 {
                            let idx = crate::event_helpers::fs_mouse_index(row, app);
                            let mut sp = None;
                            let mut is_dir = false;
                            let is_shift = me.modifiers.contains(KeyModifiers::SHIFT) || me.modifiers.contains(KeyModifiers::ALT);
                            let is_ctrl = me.modifiers.contains(KeyModifiers::CONTROL);
                            let has_mods = is_shift || is_ctrl;
                            app.prevent_mouse_up_selection_cleanup = has_mods;
                            let selection_mode = app.selection_mode;

                            let mut set_selection_mode = false;
                            let current_icon_mode = app.icon_mode;
                            if let Some(fs) = app.current_file_state_mut() {
                                // Analyze click position for Smart Drag vs Selection
                                let mut hit_on_name_text = false;
                                for (rect, col) in &fs.column_bounds {
                                    if column >= rect.x && column < rect.x + rect.width {
                                        if *col == FileColumn::Name {
                                            if let Some(file) = fs.files.get(idx) {
                                                // Check visual width of name + icon
                                                let is_dir_item = fs.metadata.get(file).map(|m| m.is_dir).unwrap_or(false);
                                                let cat = crate::modules::files::get_file_category(file);
                                                let icon_str = Icon::get_for_path(file, cat, is_dir_item, current_icon_mode);
                                                let name_str = file.file_name().and_then(|n| n.to_str()).unwrap_or("..");
                                                
                                                let icon_w = icon_str.chars().map(get_visual_width).sum::<usize>() as u16;
                                                let name_w = name_str.width() as u16;
                                                
                                                // +1 for spacing, +2 reasonable buffer
                                                let content_width = icon_w.saturating_add(1).saturating_add(name_w).saturating_add(2);
                                                if column <= rect.x.saturating_add(content_width) {
                                                    hit_on_name_text = true;
                                                }
                                            }
                                        }
                                    }
                                }
                                
                                if !hit_on_name_text && !has_mods {
                                     // Clicked on empty space (or metadata column) -> Enable Range Selection Mode
                                     set_selection_mode = true;
                                }

                                if row >= 3 && idx < fs.files.len() {
                                    if has_mods {
                                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                            "Mouse Down: shift={} ctrl={}",
                                            is_shift, is_ctrl
                                        )));
                                    }

                                    let is_divider = fs.files.get(idx).map(|p| p.to_string_lossy() == "__DIVIDER__").unwrap_or(false);
                                    if is_divider {
                                        // Click divider to jump to the first global result
                                        if idx + 1 < fs.files.len() {
                                            fs.selection.handle_move(idx + 1, false);
                                            fs.table_state.select(fs.selection.selected);
                                        }
                                        return true;
                                    }
                                    if button == MouseButton::Left {
                                        let is_sticky = selection_mode && !is_shift;

                                        fs.selection.handle_click(idx, is_shift, is_ctrl, is_sticky);
                                        fs.table_state.select(fs.selection.selected);
                                    }
                                    
                                    if let Some(p) = fs.files.get(idx).cloned() {
                                        is_dir = fs.metadata.get(&p).map(|m| m.is_dir).unwrap_or(false);
                                        sp = Some(p);
                                    }
                                } else if row >= 3 && button == MouseButton::Left && !has_mods {
                                    fs.selection.clear();
                                    fs.table_state.select(None);
                                } else if row >= 3 && button == MouseButton::Right {
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

                            
                            if set_selection_mode {
                                app.selection_mode = true;
                                app.prevent_mouse_up_selection_cleanup = true;
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
                                                nfs.selection.selected = Some(0);
                                                nfs.selection.anchor = Some(0);
                                                nfs.selection.clear_multi();
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
                                            fs.selection.selected = Some(0);
                                            fs.selection.anchor = Some(0);
                                            fs.selection.clear_multi();
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
                        }
                    } else if row == h.saturating_sub(1) && column < 9 && !matches!(app.mode, AppMode::Editor | AppMode::Viewer) {
                        app.running = false;
                        return true;
                    }
                    return true;
                }
                MouseEventKind::Up(_) => {
                    // Forward Release to Editor in IDE mode
                    if app.current_view == CurrentView::Editor && column >= app.sidebar_width() && (matches!(app.mode, AppMode::Normal) || matches!(app.mode, AppMode::EditorSearch) || matches!(app.mode, AppMode::EditorReplace) || matches!(app.mode, AppMode::EditorGoToLine)) {
                        let sw = app.sidebar_width();
                        let cw = w.saturating_sub(sw);
                        let pc = app.panes.len();
                        let pw = if pc > 0 { cw / pc as u16 } else { cw };
                        let cp = (column.saturating_sub(sw) / pw) as usize;
                        if cp < pc {
                            if let Some(pane) = app.panes.get_mut(cp) {
                                if let Some(preview) = &mut pane.preview {
                                    if let Some(editor) = &mut preview.editor {
                                        let pane_area = ratatui::layout::Rect::new(
                                            sw + (cp as u16 * pw) + 1,
                                            3, 
                                            pw.saturating_sub(2),
                                            h.saturating_sub(4),
                                        );
                                        editor.handle_mouse_event(me, pane_area);
                                    }
                                }
                            }
                        }
                    }

                    if app.is_resizing_sidebar {
                        app.is_resizing_sidebar = false;
                        let _ = crate::config::save_state(app);
                        return true;
                    }
                    if app.is_dragging {
                        if let Some((source, target)) =
                            app.drag_source.take().zip(app.hovered_drop_target.take())
                        {
                            // 1. Resolve Sources (Single or Multi-select)
                            let mut sources = Vec::new();
                            for pane in &app.panes {
                                if let Some(fs) = pane.current_state() {
                                    // Check if the dragged source is part of this pane's multi-selection
                                    // We check indices in multi_select to see if any matches 'source'
                                    let idx_of_source = fs.files.iter().position(|p| *p == source);
                                    if let Some(idx_of_source) = idx_of_source {
                                        if fs.selection.multi.contains(&idx_of_source)
                                            || fs.selection.selected == Some(idx_of_source)
                                        {
                                            if !fs.selection.is_empty() {
                                                for &i in fs.selection.multi_selected_indices() {
                                                    if let Some(path) = fs.files.get(i) {
                                                        sources.push(path.clone());
                                                    }
                                                }
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                            if sources.is_empty() {
                                sources.push(source.clone());
                            }

                            match target {
                                DropTarget::ImportServers | DropTarget::RemotesHeader => {
                                    // Only makes sense for single TOML file
                                    if sources.len() == 1 {
                                        if sources[0]
                                            .extension()
                                            .map(|e| e == "toml")
                                            .unwrap_or(false)
                                        {
                                            let _ = app.import_servers(sources[0].clone());
                                            let _ = crate::config::save_state(app);
                                        }
                                    }
                                }
                                DropTarget::Favorites => {
                                    let mut changed = false;
                                    for s in sources {
                                        if s.is_dir() && !app.starred.contains(&s) {
                                            app.starred.push(s);
                                            changed = true;
                                        }
                                    }
                                    if changed {
                                        let _ = crate::config::save_state(app);
                                    }
                                }
                                DropTarget::Pane(t_idx) => {
                                    if let Some(dest_dir) = app
                                        .panes
                                        .get(t_idx)
                                        .and_then(|p| p.current_state())
                                        .map(|fs| fs.current_path.clone())
                                    {
                                        app.mode = AppMode::DragDropMenu {
                                            sources,
                                            target: dest_dir,
                                        };
                                    }
                                }
                                DropTarget::Folder(target_path) => {
                                    // Avoid dropping into itself
                                    if !sources.contains(&target_path) {
                                        app.mode = AppMode::DragDropMenu {
                                            sources,
                                            target: target_path,
                                        };
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
                                         fs.selection.selected = Some(0);
                                         fs.selection.anchor = Some(0);
                                         fs.selection.clear_multi();
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
                                                 fs.selection.selected = Some(0);
                                                 fs.selection.anchor = Some(0);
                                                 fs.selection.clear_multi();
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
                    } else {
                        // Handle simple click on file (MouseUp without Drag) to clear multi-select if needed
                        let prevent_cleanup = app.prevent_mouse_up_selection_cleanup;
                        let selection_mode = app.selection_mode;
                        if row >= 3 {
                            let idx = crate::event_helpers::fs_mouse_index(row, app);
                            if let Some(fs) = app.current_file_state_mut() {
                                if idx < fs.files.len() {
                                    if let MouseEventKind::Up(MouseButton::Left) = me.kind {
                                        let current_mods = me.modifiers.contains(KeyModifiers::SHIFT)
                                            || me.modifiers.contains(KeyModifiers::CONTROL)
                                            || me.modifiers.contains(KeyModifiers::ALT);
                                        if !prevent_cleanup && !selection_mode && !current_mods {
                                            fs.selection.clear();
                                            fs.selection.selected = Some(idx);
                                            fs.selection.anchor = Some(idx);
                                            fs.table_state.select(Some(idx));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    app.prevent_mouse_up_selection_cleanup = false;
                    app.is_dragging = false;
                    app.drag_start_pos = None;
                    app.drag_source = None;
                    app.hovered_drop_target = None;
                    app.selection_mode = false;
                    return true;
                }
                MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                    app.mouse_pos = (column, row);

                    // Forward Drag to Editor in IDE mode
                    if app.current_view == CurrentView::Editor && column >= app.sidebar_width() && (matches!(app.mode, AppMode::Normal) || matches!(app.mode, AppMode::EditorSearch) || matches!(app.mode, AppMode::EditorReplace) || matches!(app.mode, AppMode::EditorGoToLine)) {
                        let sw = app.sidebar_width();
                        let cw = w.saturating_sub(sw);
                        let pc = app.panes.len();
                        let pw = if pc > 0 { cw / pc as u16 } else { cw };
                        let cp = (column.saturating_sub(sw) / pw) as usize;
                        if cp < pc {
                            if let Some(pane) = app.panes.get_mut(cp) {
                                if let Some(preview) = &mut pane.preview {
                                    if let Some(editor) = &mut preview.editor {
                                        let pane_area = ratatui::layout::Rect::new(
                                            sw + (cp as u16 * pw) + 1,
                                            3, 
                                            pw.saturating_sub(2),
                                            h.saturating_sub(4),
                                        );
                                        editor.handle_mouse_event(me, pane_area);
                                    }
                                }
                            }
                        }
                    }

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
                            // Only start drag-and-drop if NOT extending selection with Shift or selection_mode
                            if !me.modifiers.contains(KeyModifiers::SHIFT) && !app.selection_mode {
                                app.is_dragging = true;
                            }
                        }
                    }

                    // Selection extension during drag
                    if !app.is_resizing_sidebar && row >= 3 && column >= app.sidebar_width() {
                        if (me.modifiers.contains(KeyModifiers::SHIFT) || me.modifiers.contains(KeyModifiers::ALT) || app.selection_mode)
                            && !app.is_dragging
                        {
                            let mut idx = crate::event_helpers::fs_mouse_index(row, app);
                            if let Some(fs) = app.current_file_state_mut() {
                                if !fs.files.is_empty() {
                                    idx = idx.min(fs.files.len().saturating_sub(1));
                                    let is_not_divider = fs.files.get(idx).map(|p| p.to_string_lossy() != "__DIVIDER__").unwrap_or(false);
                                    if is_not_divider {
                                        let anchor = fs
                                            .selection
                                            .anchor
                                            .unwrap_or(fs.selection.selected.unwrap_or(0));
                                        fs.selection.anchor = Some(anchor);
                                        fs.selection.clear_multi();
                                        for i in
                                            std::cmp::min(anchor, idx)..=std::cmp::max(anchor, idx)
                                        {
                                            fs.selection.add(i);
                                        }
                                        fs.selection.selected = Some(idx);
                                        fs.table_state.select(Some(idx));
                                    }
                                }
                            }
                        }
                    }

                    if app.is_dragging {
                        app.hovered_drop_target = None;
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
                                    if let Some(fs) =
                                        app.panes.get(hi).and_then(|p| p.current_state())
                                    {
                                        let mouse_row_offset = row.saturating_sub(3) as usize;
                                        let idx = fs.table_state.offset() + mouse_row_offset;
                                        if let Some(path) = fs.files.get(idx) {
                                            if path.is_dir() {
                                                app.hovered_drop_target =
                                                    Some(DropTarget::Folder(path.clone()));
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
                    } else if app.current_view == CurrentView::Editor {
                        let (w, h) = app.terminal_size;
                        let sw = app.sidebar_width();
                        let pc = app.panes.len();
                        let cw = w.saturating_sub(sw);
                        let pw = if pc > 0 { cw / pc as u16 } else { cw };
                        let focused_idx = app.focused_pane_index;

                        if let Some(pane) = app.panes.get_mut(focused_idx) {
                            if let Some(preview) = &mut pane.preview {
                                if let Some(editor) = &mut preview.editor {
                                    let pane_area = ratatui::layout::Rect::new(
                                        sw + (focused_idx as u16 * pw),
                                        1, pw, h.saturating_sub(1)
                                    );
                                    // Change step from 3 to 1 for smoother scroll
                                    let smooth_me = me;
                                    if let MouseEventKind::ScrollUp = smooth_me.kind {
                                        // TextEditor internal handle_mouse_event might have its own step, 
                                        // but usually it responds to the event itself.
                                        editor.handle_mouse_event(smooth_me, pane_area);
                                    }
                                }
                            }
                        }
                    } else if let Some(fs) = app.current_file_state_mut() {
                        let new_offset = fs.table_state.offset().saturating_sub(1);
                        *fs.table_state.offset_mut() = new_offset;
                    }
                    return true;
                }
                MouseEventKind::ScrollDown => {
                    if let AppMode::Settings = app.mode {
                        app.settings_scroll = app.settings_scroll.saturating_add(2);
                    } else if app.current_view == CurrentView::Editor {
                        let (w, h) = app.terminal_size;
                        let sw = app.sidebar_width();
                        let pc = app.panes.len();
                        let cw = w.saturating_sub(sw);
                        let pw = if pc > 0 { cw / pc as u16 } else { cw };
                        let focused_idx = app.focused_pane_index;

                        if let Some(pane) = app.panes.get_mut(focused_idx) {
                            if let Some(preview) = &mut pane.preview {
                                if let Some(editor) = &mut preview.editor {
                                    let pane_area = ratatui::layout::Rect::new(
                                        sw + (focused_idx as u16 * pw),
                                        1, pw, h.saturating_sub(1)
                                    );
                                    editor.handle_mouse_event(me, pane_area);
                                }
                            }
                        }
                    } else if let Some(fs) = app.current_file_state_mut() {
                        let max_offset = fs
                            .files
                            .len()
                            .saturating_sub(fs.view_height.saturating_sub(3));
                        let new_offset = (fs.table_state.offset() + 1).min(max_offset);
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
                        editor.insert_string(&text);
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
    terma::utils::delete_word_backwards(s);
}
