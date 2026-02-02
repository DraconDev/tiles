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
mod events;

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
    events::handle_event(evt, app, event_tx, panes_needing_refresh)
}

fn delete_word_backwards(s: &mut String) {
