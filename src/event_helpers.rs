use crate::app::{
    App, AppEvent, AppMode, CommandAction, CommandItem, ContextMenuAction, ContextMenuTarget,
    FileState,
};
use crate::config::save_state;
use std::path::PathBuf;
// use std::sync::{Arc, Mutex};
use tokio::process::Command as TokioCommand;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

pub fn fs_mouse_index(row: u16, app: &App) -> usize {
    if let Some(fs) = app.current_file_state() {
        // AppHeader(0), PaneBorder/Tabs(1), Breadcrumbs(2), FileHeader(3), Content(4+) -- WAIT, Rendering is Content at Y=3.
        // Row 3 (First File) -> Index 0. 3-3=0.
        let mouse_row_offset = row.saturating_sub(3) as usize; 
        fs.table_state.offset() + mouse_row_offset
    } else {
        0
    }
}

pub fn push_history(fs: &mut FileState, path: PathBuf) {
    if let Some(last) = fs.history.get(fs.history_index) {
        if last == &path {
            return;
        }
    }
    fs.history.truncate(fs.history_index + 1);
    fs.history.push(path);
    fs.history_index = fs.history.len() - 1;
}

pub fn navigate_up(app: &mut App) {
    let pane_idx = app.focused_pane_index;
    let mut to_restore = None;
    let mut old_path = None;
    let mut old_idx = 0;

    if let Some(fs) = app
        .panes
        .get_mut(pane_idx)
        .and_then(|p| p.current_state_mut())
    {
        if let Some(parent) = fs.current_path.parent() {
            old_path = Some(fs.current_path.clone());
            old_idx = fs.selection.selected.unwrap_or(0);
            
            let new_path = parent.to_path_buf();
            fs.current_path = new_path.clone();
            push_history(fs, new_path.clone());
            to_restore = Some(new_path);
        }
    }

    if let Some(ref p) = old_path {
        app.folder_selections.insert(p.clone(), old_idx);
    }

    if let Some(path) = to_restore {
        let restored_idx = app.folder_selections.get(&path).cloned().unwrap_or(0);
        if let Some(fs) = app
            .panes
            .get_mut(pane_idx)
            .and_then(|p| p.current_state_mut())
        {
            fs.selection.selected = Some(restored_idx);
            fs.selection.anchor = Some(restored_idx);
            fs.table_state.select(Some(restored_idx));
            *fs.table_state.offset_mut() = restored_idx.saturating_sub(fs.view_height / 2);
            fs.search_filter.clear();

            // Allow selecting the folder we just came from
            if let Some(old) = old_path {
                fs.pending_select_path = Some(old);
            }
        }
    }
}

pub fn navigate_back(app: &mut App) {
    let pane_idx = app.focused_pane_index;
    let mut to_restore = None;
    let mut old_path = None;
    let mut old_idx = 0;

    if let Some(fs) = app
        .panes
        .get_mut(pane_idx)
        .and_then(|p| p.current_state_mut())
    {
        if fs.history_index > 0 {
            old_path = Some(fs.current_path.clone());
            old_idx = fs.selection.selected.unwrap_or(0);
            fs.history_index -= 1;
            let new_path = fs.history[fs.history_index].clone();
            fs.current_path = new_path.clone();
            to_restore = Some(new_path);
        }
    }

    if let Some(p) = &old_path {
        app.folder_selections.insert(p.clone(), old_idx);
    }

    if let Some(path) = to_restore {
        let restored_idx = app.folder_selections.get(&path).cloned().unwrap_or(0);
        if let Some(fs) = app
            .panes
            .get_mut(pane_idx)
            .and_then(|p| p.current_state_mut())
        {
            fs.selection.selected = Some(restored_idx);
            fs.selection.anchor = Some(restored_idx);
            fs.table_state.select(Some(restored_idx));
            *fs.table_state.offset_mut() = restored_idx.saturating_sub(fs.view_height / 2);
            fs.search_filter.clear();

            // Allow selecting the folder we just came from
            if let Some(old) = old_path {
                fs.pending_select_path = Some(old);
            }
        }
    }
}

pub fn navigate_forward(app: &mut App) {
    let pane_idx = app.focused_pane_index;
    let mut to_restore = None;
    let mut old_path = None;
    let mut old_idx = 0;

    if let Some(fs) = app
        .panes
        .get_mut(pane_idx)
        .and_then(|p| p.current_state_mut())
    {
        if fs.history_index + 1 < fs.history.len() {
            old_path = Some(fs.current_path.clone());
            old_idx = fs.selection.selected.unwrap_or(0);
            fs.history_index += 1;
            let new_path = fs.history[fs.history_index].clone();
            fs.current_path = new_path.clone();
            to_restore = Some(new_path);
        }
    }

    if let Some(p) = old_path {
        app.folder_selections.insert(p, old_idx);
    }

    if let Some(path) = to_restore {
        let restored_idx = app.folder_selections.get(&path).cloned().unwrap_or(0);
        if let Some(fs) = app
            .panes
            .get_mut(pane_idx)
            .and_then(|p| p.current_state_mut())
        {
            fs.selection.selected = Some(restored_idx);
            fs.selection.anchor = Some(restored_idx);
            fs.table_state.select(Some(restored_idx));
            *fs.table_state.offset_mut() = restored_idx.saturating_sub(fs.view_height / 2);
            fs.search_filter.clear();
        }
    }
}

pub fn update_commands(app: &mut App) {
    let commands = vec![
        CommandItem {
            key: "quit".to_string(),
            desc: "Quit".to_string(),
            action: CommandAction::Quit,
        },
        CommandItem {
            key: "remote".to_string(),
            desc: "Add Remote Host".to_string(),
            action: CommandAction::AddRemote,
        },
        CommandItem {
            key: "palette".to_string(),
            desc: "Command Palette".to_string(),
            action: CommandAction::CommandPalette,
        },
        CommandItem {
            key: "zoom".to_string(),
            desc: "Toggle Zoom".to_string(),
            action: CommandAction::ToggleZoom,
        },
    ];
    let mut filtered = commands;
    for (bookmark_idx, bookmark) in app.remote_bookmarks.iter().enumerate() {
        filtered.push(CommandItem {
            key: format!("connect_{}", bookmark_idx),
            desc: format!("Connect to: {}", bookmark.name),
            action: CommandAction::ConnectToRemote(bookmark_idx),
        });
    }
    app.filtered_commands = filtered
        .into_iter()
        .filter(|cmd| {
            cmd.desc
                .to_lowercase()
                .contains(&app.input.value.to_lowercase())
        })
        .collect();
    app.command_index = app
        .command_index
        .min(app.filtered_commands.len().saturating_sub(1));
}

pub fn execute_command(action: CommandAction, app: &mut App, event_tx: mpsc::Sender<AppEvent>) {
    match action {
        CommandAction::Quit => {
            app.running = false;
        }
        CommandAction::ToggleZoom => app.toggle_split(), // Approximate if explicit zoom missing
        CommandAction::SwitchView(view) => app.current_view = view,
        CommandAction::AddRemote => {
            app.mode = AppMode::AddRemote(0);
            app.input.clear();
        }
        CommandAction::ConnectToRemote(idx) => {
            let _ = event_tx.try_send(AppEvent::ConnectToRemote(app.focused_pane_index, idx));
        }
        CommandAction::CommandPalette => {
            app.mode = AppMode::CommandPalette;
        }
        _ => {}
    }
}

pub fn get_context_menu_actions(target: &ContextMenuTarget, app: &App) -> Vec<ContextMenuAction> {
    match target {
        ContextMenuTarget::File(idx) => {
            let mut actions = vec![
                ContextMenuAction::Open,
                ContextMenuAction::OpenWith,
                ContextMenuAction::Separator,
                ContextMenuAction::Cut,
                ContextMenuAction::Copy,
                ContextMenuAction::CopyPath,
                ContextMenuAction::CopyName,
                ContextMenuAction::Separator,
                ContextMenuAction::Rename,
                ContextMenuAction::Delete,
                ContextMenuAction::Separator,
            ];

            if let Some(fs) = app.current_file_state() {
                if let Some(path) = fs.files.get(*idx) {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if matches!(ext.as_str(), "zip" | "tar" | "gz" | "7z" | "rar") {
                        actions.push(ContextMenuAction::ExtractHere);
                    } else {
                        actions.push(ContextMenuAction::Compress);
                    }
                }
            }

            // Check for drag support
            if terma::utils::command_exists("dragon") || terma::utils::command_exists("ripdrag") {
                actions.push(ContextMenuAction::Drag);
            }

            actions.extend(vec![
                ContextMenuAction::SetColor(None),
                ContextMenuAction::Separator,
                ContextMenuAction::Properties,
            ]);
            actions
        }
        ContextMenuTarget::Folder(_) => vec![
            // Group 1: Navigation
            ContextMenuAction::Open,
            ContextMenuAction::OpenNewTab,
            ContextMenuAction::TerminalTab,
            ContextMenuAction::TerminalWindow,
            ContextMenuAction::Separator,
            // Group 2: Basic Ops
            ContextMenuAction::Cut,
            ContextMenuAction::Copy,
            ContextMenuAction::CopyPath,
            ContextMenuAction::CopyName,
            ContextMenuAction::Separator,
            ContextMenuAction::Rename,
            ContextMenuAction::Delete,
            ContextMenuAction::Separator,
            // Group 3: Advanced
            ContextMenuAction::AddToFavorites,
            ContextMenuAction::Compress,
            ContextMenuAction::SetColor(None),
            ContextMenuAction::Separator,
            // Group 4: Metadata
            ContextMenuAction::Properties,
        ],
        ContextMenuTarget::EmptySpace => {
            let mut actions = vec![ContextMenuAction::NewFile, ContextMenuAction::NewFolder];

            if app.clipboard.is_some() {
                actions.push(ContextMenuAction::Paste);
            }

            actions.extend(vec![
                ContextMenuAction::Separator,
                ContextMenuAction::Refresh,
                ContextMenuAction::ToggleHidden,
                ContextMenuAction::Separator,
                ContextMenuAction::TerminalTab,
                ContextMenuAction::TerminalWindow,
                ContextMenuAction::SystemMonitor,
            ]);
            actions
        }
        ContextMenuTarget::SidebarFavorite(_) => vec![
            ContextMenuAction::Open,
            ContextMenuAction::RemoveFromFavorites,
            ContextMenuAction::Separator,
            ContextMenuAction::Properties,
        ],
        ContextMenuTarget::SidebarRemote(_) => vec![
            ContextMenuAction::ConnectRemote,
            ContextMenuAction::DeleteRemote,
            ContextMenuAction::Separator,
            ContextMenuAction::Properties,
        ],
        ContextMenuTarget::SidebarStorage(_) => vec![
            ContextMenuAction::Mount,
            ContextMenuAction::Unmount,
            ContextMenuAction::Separator,
            ContextMenuAction::Properties,
        ],
        ContextMenuTarget::ProjectTree(path) => {
            let mut actions = vec![
                ContextMenuAction::NewFile,
                ContextMenuAction::NewFolder,
                ContextMenuAction::Separator,
            ];
            if path.is_file() {
                actions.extend(vec![
                    ContextMenuAction::Rename,
                    ContextMenuAction::Delete,
                    ContextMenuAction::Separator,
                ]);
            } else {
                actions.extend(vec![
                    ContextMenuAction::TerminalTab,
                    ContextMenuAction::Separator,
                ]);
            }
            actions.push(ContextMenuAction::Properties);
            actions
        }
        _ => vec![],
    }
}

pub fn handle_context_menu_action(
    action: &ContextMenuAction,
    target: &ContextMenuTarget,
    app: &mut App,
    event_tx: mpsc::Sender<AppEvent>,
) {
    match action {
        ContextMenuAction::Open => {
            if let ContextMenuTarget::File(idx) = target {
                if let Some(fs) = app.current_file_state() {
                    if let Some(path) = fs.files.get(*idx) {
                        terma::utils::spawn_detached(
                            "xdg-open",
                            vec![path.to_string_lossy().to_string()],
                        );
                    }
                }
            } else if let ContextMenuTarget::Folder(idx) = target {
                if let Some(fs) = app.current_file_state_mut() {
                    if let Some(path) = fs.files.get(*idx).cloned() {
                        fs.current_path = path.clone();
                        fs.selection.selected = Some(0);
                        fs.selection.anchor = Some(0);
                        fs.selection.clear_multi();
                        fs.table_state.select(Some(0));
                        push_history(fs, path);
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                    }
                }
            } else if let ContextMenuTarget::SidebarFavorite(path) = target {
                if let Some(fs) = app.current_file_state_mut() {
                    fs.current_path = path.clone();
                    fs.selection.selected = Some(0);
                    fs.selection.anchor = Some(0);
                    fs.selection.clear_multi();
                    push_history(fs, path.clone());
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                }
            }
        }
        ContextMenuAction::OpenNewTab => {
            if let ContextMenuTarget::Folder(idx) = target {
                if let Some(p) = app.panes.get_mut(app.focused_pane_index) {
                    if let Some(fs) = p.current_state() {
                        if let Some(path) = fs.files.get(*idx) {
                            let mut nfs = fs.clone();
                            nfs.current_path = path.clone();
                            nfs.history = vec![path.clone()];
                            p.open_tab(nfs);
                            let _ =
                                event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                        }
                    }
                }
            }
        }
        ContextMenuAction::Edit => {
            if let ContextMenuTarget::File(idx) = target {
                if let Some(fs) = app.current_file_state() {
                    if let Some(path) = fs.files.get(*idx).cloned() {
                        let _ = event_tx
                            .try_send(AppEvent::PreviewRequested(app.focused_pane_index, path));
                    }
                }
            }
        }
        ContextMenuAction::Run => {
            if let ContextMenuTarget::File(idx) = target {
                if let Some(fs) = app.current_file_state() {
                    if let Some(path) = fs.files.get(*idx).cloned() {
                        let _ = event_tx.try_send(AppEvent::SpawnDetached {
                            cmd: path.to_string_lossy().to_string(),
                            args: vec![],
                        });
                    }
                }
            }
        }
        ContextMenuAction::RunTerminal => {
            if let ContextMenuTarget::File(idx) = target {
                if let Some(fs) = app.current_file_state() {
                    if let Some(path) = fs.files.get(*idx).cloned() {
                        let _ = event_tx.try_send(AppEvent::SpawnTerminal {
                            path: path
                                .parent()
                                .unwrap_or(std::path::Path::new("."))
                                .to_path_buf(),
                            new_tab: true,
                            remote: fs.remote_session.clone(),
                            command: Some(format!(
                                "./{}",
                                path.file_name().unwrap().to_string_lossy()
                            )),
                        });
                    }
                }
            }
        }
        ContextMenuAction::OpenWith => {
            if let ContextMenuTarget::File(idx) = target {
                if let Some(fs) = app.current_file_state() {
                    if let Some(path) = fs.files.get(*idx).cloned() {
                        app.mode = AppMode::OpenWith(path);
                        app.input.clear();
                        return;
                    }
                }
            }
        }
        ContextMenuAction::Delete => {
            if let ContextMenuTarget::ProjectTree(path) = target {
                app.mode = AppMode::DeleteFile(path.clone());
                app.input.set_value("y".to_string());
                return;
            }
            app.mode = AppMode::Delete;
            return;
        }
        ContextMenuAction::Rename => {
            if let ContextMenuTarget::ProjectTree(path) = target {
                if let Some(name) = path.file_name() {
                    let name_str = name.to_string_lossy().to_string();
                    app.input.set_value(name_str.clone());
                    if let Some(dot_idx) = name_str.rfind('.') {
                        if dot_idx > 0 {
                            app.input.cursor_position = dot_idx;
                        } else {
                            app.input.cursor_position = name_str.len();
                        }
                    } else {
                        app.input.cursor_position = name_str.len();
                    }
                    app.rename_selected = true;
                }
            } else if let ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) = target {
                if let Some(fs) = app.current_file_state() {
                    if let Some(path) = fs.files.get(*idx) {
                        if let Some(name) = path.file_name() {
                            let name_str = name.to_string_lossy().to_string();
                            app.input.set_value(name_str.clone());
                            if let Some(dot_idx) = name_str.rfind('.') {
                                if dot_idx > 0 {
                                    app.input.cursor_position = dot_idx;
                                } else {
                                    app.input.cursor_position = name_str.len();
                                }
                            } else {
                                app.input.cursor_position = name_str.len();
                            }
                            app.rename_selected = true;
                        }
                    }
                }
            }
            app.mode = AppMode::Rename;
            return;
        }
        ContextMenuAction::Properties => {
            app.mode = AppMode::Properties;
            return;
        }
        ContextMenuAction::SystemMonitor => {
            let _ = event_tx.try_send(AppEvent::SystemMonitor);
        }
        ContextMenuAction::Refresh => {
            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
        }
        ContextMenuAction::NewFile => {
            if let ContextMenuTarget::ProjectTree(path) = target {
                // If path is a file, use parent. If dir, use itself.
                let base = if path.is_dir() { path.clone() } else { path.parent().unwrap_or(path).to_path_buf() };
                app.mode = AppMode::NewFile;
                // We need a way to tell the app where to create it. 
                // Currently NewFile uses current_file_state.current_path.
                // Let's assume for now it uses the focused pane's path if we don't change core logic.
                // Better: Update current_path of focused pane to the tree base if needed? 
                // User wants to create in THAT folder.
                if let Some(fs) = app.current_file_state_mut() {
                    fs.current_path = base;
                }
            } else {
                app.mode = AppMode::NewFile;
            }
            app.input.clear();
            return;
        }
        ContextMenuAction::NewFolder => {
            if let ContextMenuTarget::ProjectTree(path) = target {
                let base = if path.is_dir() { path.clone() } else { path.parent().unwrap_or(path).to_path_buf() };
                app.mode = AppMode::NewFolder;
                if let Some(fs) = app.current_file_state_mut() {
                    fs.current_path = base;
                }
            } else {
                app.mode = AppMode::NewFolder;
            }
            app.input.clear();
            return;
        }
        ContextMenuAction::ToggleHidden => {
            if let Some(fs) = app.current_file_state_mut() {
                fs.show_hidden = !fs.show_hidden;
                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
            }
        }
        ContextMenuAction::RemoveFromFavorites => {
            if let ContextMenuTarget::SidebarFavorite(path) = target {
                app.starred.retain(|p| p != path);
                let _ = save_state(app);
            }
        }
        ContextMenuAction::AddToFavorites => {
            if let ContextMenuTarget::Folder(idx) = target {
                if let Some(fs) = app.current_file_state() {
                    if let Some(path) = fs.files.get(*idx) {
                        if !app.starred.contains(path) {
                            app.starred.push(path.clone());
                            let _ = save_state(app);
                        }
                    }
                }
            }
        }
        ContextMenuAction::Copy => {
            if let ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) = target {
                if let Some(fs) = app.current_file_state() {
                    if let Some(path) = fs.files.get(*idx) {
                        app.clipboard = Some((path.clone(), crate::app::ClipboardOp::Copy));
                    }
                }
            }
        }
        ContextMenuAction::CopyPath => {
            if let ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) = target {
                if let Some(fs) = app.current_file_state() {
                    if let Some(path) = fs.files.get(*idx) {
                        let path_str = path.to_string_lossy().to_string();
                        let mut stdout = std::io::stdout();
                        let _ = terma::visuals::osc::copy_to_clipboard(&mut stdout, &path_str);
                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                            "Copied path to clipboard: {}",
                            path_str
                        )));
                    }
                }
            }
        }
        ContextMenuAction::CopyName => {
            if let ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) = target {
                if let Some(fs) = app.current_file_state() {
                    if let Some(path) = fs.files.get(*idx) {
                        if let Some(name) = path.file_name() {
                            let name_str = name.to_string_lossy().to_string();
                            let mut stdout = std::io::stdout();
                            let _ = terma::visuals::osc::copy_to_clipboard(&mut stdout, &name_str);
                            let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                "Copied name to clipboard: {}",
                                name_str
                            )));
                        }
                    }
                }
            }
        }
        ContextMenuAction::Cut => {
            if let ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) = target {
                if let Some(fs) = app.current_file_state() {
                    if let Some(path) = fs.files.get(*idx) {
                        app.clipboard = Some((path.clone(), crate::app::ClipboardOp::Cut));
                    }
                }
            }
        }
        ContextMenuAction::Paste => {
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
        }
        ContextMenuAction::Mount => {
            if let ContextMenuTarget::SidebarStorage(idx) = target {
                if let Some(disk) = app.system_state.disks.get(*idx) {
                    if !disk.is_mounted {
                        let dev = disk.device.clone();
                        let tx = event_tx.clone();
                        let p_idx = app.focused_pane_index;
                        tokio::spawn(async move {
                            if let Ok(out) = std::process::Command::new("udisksctl")
                                .arg("mount")
                                .arg("-b")
                                .arg(&dev)
                                .output()
                            {
                                if String::from_utf8_lossy(&out.stdout).contains("Mounted") {
                                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                                    let _ = tx.send(AppEvent::RefreshFiles(p_idx)).await;
                                }
                            }
                        });
                    }
                }
            }
        }
        ContextMenuAction::Unmount => {
            if let ContextMenuTarget::SidebarStorage(idx) = target {
                if let Some(disk) = app.system_state.disks.get(*idx) {
                    if disk.is_mounted {
                        let dev = disk.device.clone();
                        let tx = event_tx.clone();
                        let p_idx = app.focused_pane_index;
                        tokio::spawn(async move {
                            if let Ok(_) = std::process::Command::new("udisksctl")
                                .arg("unmount")
                                .arg("-b")
                                .arg(&dev)
                                .output()
                            {
                                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                                let _ = tx.send(AppEvent::RefreshFiles(p_idx)).await;
                            }
                        });
                    }
                }
            }
        }
        ContextMenuAction::ConnectRemote => {
            if let ContextMenuTarget::SidebarRemote(idx) = target {
                let _ = event_tx.try_send(AppEvent::StatusMsg("Connecting...".to_string()));
                execute_command(CommandAction::ConnectToRemote(*idx), app, event_tx.clone());
            }
        }
        ContextMenuAction::DeleteRemote => {
            if let ContextMenuTarget::SidebarRemote(idx) = target {
                if *idx < app.remote_bookmarks.len() {
                    app.remote_bookmarks.remove(*idx);
                    let _ = save_state(app);
                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                }
            }
        }
        ContextMenuAction::TerminalWindow => {
            let working_dir = if let ContextMenuTarget::Folder(idx) = target {
                app.current_file_state()
                    .and_then(|fs| fs.files.get(*idx).cloned())
            } else if let ContextMenuTarget::EmptySpace = target {
                app.current_file_state().map(|fs| fs.current_path.clone())
            } else {
                None
            };

            if let Some(path) = working_dir {
                let _ = event_tx.try_send(AppEvent::SpawnTerminal {
                    path,
                    new_tab: false,
                    remote: app
                        .current_file_state()
                        .and_then(|fs| fs.remote_session.clone()),
                    command: None,
                });
            }
        }
        ContextMenuAction::TerminalTab => {
            let working_dir = if let ContextMenuTarget::Folder(idx) = target {
                app.current_file_state()
                    .and_then(|fs| fs.files.get(*idx).cloned())
            } else if let ContextMenuTarget::EmptySpace = target {
                app.current_file_state().map(|fs| fs.current_path.clone())
            } else {
                None
            };

            if let Some(path) = working_dir {
                let _ = event_tx.try_send(AppEvent::SpawnTerminal {
                    path,
                    new_tab: true,
                    remote: app
                        .current_file_state()
                        .and_then(|fs| fs.remote_session.clone()),
                    command: None,
                });
            }
        }
        ContextMenuAction::Compress => {
            if let ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) = target {
                let path_opt = app
                    .current_file_state()
                    .and_then(|fs| fs.files.get(*idx).cloned());
                if let Some(path) = path_opt {
                    if let Some(name) = path.file_name() {
                        let tx = event_tx.clone();
                        let p_idx = app.focused_pane_index;
                        let name_str = name.to_string_lossy().into_owned();
                        let task_id = uuid::Uuid::new_v4();
                        let filename = name.to_os_string();
                        let parent = path
                            .parent()
                            .unwrap_or(std::path::Path::new("."))
                            .to_path_buf();

                        tokio::spawn(async move {
                            crate::app::log_debug(&format!(
                                "Starting compression of {} to zip",
                                name_str
                            ));
                            let _ = tx.try_send(AppEvent::TaskProgress(
                                task_id,
                                0.0,
                                format!("Compressing {}...", name_str),
                            ));

                            let has_ark = std::process::Command::new("which")
                                .arg("ark")
                                .output()
                                .map(|o| o.status.success())
                                .unwrap_or(false);
                            let has_file_roller = std::process::Command::new("which")
                                .arg("file-roller")
                                .output()
                                .map(|o| o.status.success())
                                .unwrap_or(false);
                            let has_engrampa = std::process::Command::new("which")
                                .arg("engrampa")
                                .output()
                                .map(|o| o.status.success())
                                .unwrap_or(false);
                            let has_zip = std::process::Command::new("which")
                                .arg("zip")
                                .output()
                                .map(|o| o.status.success())
                                .unwrap_or(false);

                            crate::app::log_debug(&format!(
                                "Tools found: ark={}, file-roller={}, engrampa={}, zip={}",
                                has_ark, has_file_roller, has_engrampa, has_zip
                            ));

                            let mut child_cmd = if has_ark {
                                crate::app::log_debug("Using ark for compression");
                                let mut zip_name = filename.clone();
                                zip_name.push(".zip");
                                let mut c = TokioCommand::new("ark");
                                c.arg("-b")
                                    .arg("-t")
                                    .arg(&zip_name)
                                    .arg(&filename)
                                    .current_dir(&parent);
                                c
                            } else if has_file_roller {
                                crate::app::log_debug("Using file-roller for compression");
                                let mut zip_name = filename.clone();
                                zip_name.push(".zip");
                                let mut c = TokioCommand::new("file-roller");
                                c.arg("--add-to")
                                    .arg(&zip_name)
                                    .arg(&filename)
                                    .current_dir(&parent);
                                c
                            } else if has_engrampa {
                                crate::app::log_debug("Using engrampa for compression");
                                let mut zip_name = filename.clone();
                                zip_name.push(".zip");
                                let mut c = TokioCommand::new("engrampa");
                                c.arg("--add-to")
                                    .arg(&zip_name)
                                    .arg(&filename)
                                    .current_dir(&parent);
                                c
                            } else if has_zip {
                                crate::app::log_debug("Using zip for compression");
                                let mut zip_name = filename.clone();
                                zip_name.push(".zip");
                                let mut c = TokioCommand::new("zip");
                                c.arg("-r")
                                    .arg(&zip_name)
                                    .arg(&filename)
                                    .current_dir(&parent);
                                c
                            } else {
                                crate::app::log_debug("Falling back to tar for compression");
                                let mut tar_name = filename.clone();
                                tar_name.push(".tar.gz");
                                let mut c = TokioCommand::new("tar");
                                c.arg("-czf")
                                    .arg(&tar_name)
                                    .arg(&filename)
                                    .current_dir(&parent);
                                c
                            };

                            let mut child = match child_cmd
                                .stdout(std::process::Stdio::piped())
                                .stderr(std::process::Stdio::piped())
                                .spawn()
                            {
                                Ok(c) => c,
                                Err(e) => {
                                    let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                        "Failed to spawn tool: {}",
                                        e
                                    )));
                                    let _ = tx.try_send(AppEvent::TaskFinished(task_id));
                                    return;
                                }
                            };

                            let mut current_progress = 0.0;
                            let res = loop {
                                tokio::select! {
                                    status = child.wait() => {
                                        break status;
                                    }
                                    _ = sleep(Duration::from_millis(500)) => {
                                        if current_progress < 0.9 {
                                            current_progress += 0.05;
                                            let _ = tx.try_send(AppEvent::TaskProgress(task_id, current_progress, format!("Compressing {}... ({:.0}%)", name_str, current_progress * 100.0)));
                                        }
                                    }
                                }
                            };

                            match res {
                                Ok(status) if status.success() => {
                                    let _ = tx.try_send(AppEvent::TaskProgress(
                                        task_id,
                                        1.0,
                                        "Done".to_string(),
                                    ));
                                    sleep(Duration::from_millis(800)).await;
                                    let _ = tx.try_send(AppEvent::TaskFinished(task_id));
                                    let _ = tx.try_send(AppEvent::RefreshFiles(p_idx));
                                }
                                Ok(status) => {
                                    let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                        "Compression failed with status: {}",
                                        status
                                    )));
                                    let _ = tx.try_send(AppEvent::TaskFinished(task_id));
                                }
                                Err(e) => {
                                    let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                        "Error waiting for tool: {}",
                                        e
                                    )));
                                    let _ = tx.try_send(AppEvent::TaskFinished(task_id));
                                }
                            }
                        });
                    }
                }
            }
        }
        ContextMenuAction::ExtractHere => {
            if let ContextMenuTarget::File(idx) = target {
                let path_opt = app
                    .current_file_state()
                    .and_then(|fs| fs.files.get(*idx).cloned());
                if let Some(path) = path_opt {
                    let parent = path
                        .parent()
                        .unwrap_or(std::path::Path::new("."))
                        .to_path_buf();
                    let tx = event_tx.clone();
                    let p_idx = app.focused_pane_index;
                    let task_id = uuid::Uuid::new_v4();

                    tokio::spawn(async move {
                        let filename = path.file_name().unwrap().to_string_lossy().into_owned();
                        crate::app::log_debug(&format!("Starting extraction of {}", filename));
                        let _ = tx.try_send(AppEvent::TaskProgress(
                            task_id,
                            0.0,
                            format!("Extracting {}...", filename),
                        ));

                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();

                        let has_ark = std::process::Command::new("which")
                            .arg("ark")
                            .output()
                            .map(|o| o.status.success())
                            .unwrap_or(false);
                        let has_file_roller = std::process::Command::new("which")
                            .arg("file-roller")
                            .output()
                            .map(|o| o.status.success())
                            .unwrap_or(false);
                        let has_engrampa = std::process::Command::new("which")
                            .arg("engrampa")
                            .output()
                            .map(|o| o.status.success())
                            .unwrap_or(false);
                        let has_unzip = std::process::Command::new("which")
                            .arg("unzip")
                            .output()
                            .map(|o| o.status.success())
                            .unwrap_or(false);
                        let has_7z = std::process::Command::new("which")
                            .arg("7z")
                            .output()
                            .map(|o| o.status.success())
                            .unwrap_or(false);
                        let has_tar = std::process::Command::new("which")
                            .arg("tar")
                            .output()
                            .map(|o| o.status.success())
                            .unwrap_or(false);

                        crate::app::log_debug(&format!("Tools found: ark={}, file-roller={}, engrampa={}, unzip={}, 7z={}, tar={}", has_ark, has_file_roller, has_engrampa, has_unzip, has_7z, has_tar));

                        let mut child_cmd = if has_ark {
                            crate::app::log_debug("Using ark for extraction");
                            let mut c = TokioCommand::new("ark");
                            c.arg("-b")
                                .arg("-o")
                                .arg(parent.to_string_lossy().to_string())
                                .arg(path.to_string_lossy().to_string());
                            c
                        } else if has_file_roller {
                            crate::app::log_debug("Using file-roller for extraction");
                            let mut c = TokioCommand::new("file-roller");
                            c.arg("--extract-to")
                                .arg(parent.to_string_lossy().to_string())
                                .arg(path.to_string_lossy().to_string());
                            c
                        } else if has_engrampa {
                            crate::app::log_debug("Using engrampa for extraction");
                            let mut c = TokioCommand::new("engrampa");
                            c.arg("--extract-to")
                                .arg(parent.to_string_lossy().to_string())
                                .arg(path.to_string_lossy().to_string());
                            c
                        } else if ext == "zip" && has_unzip {
                            crate::app::log_debug("Using unzip for extraction");
                            let mut c = TokioCommand::new("unzip");
                            c.arg(path.to_string_lossy().to_string())
                                .arg("-d")
                                .arg(parent.to_string_lossy().to_string());
                            c
                        } else if (ext == "tar"
                            || ext == "gz"
                            || ext == "xz"
                            || ext == "bz2"
                            || ext == "tgz")
                            && has_tar
                        {
                            crate::app::log_debug("Using tar for extraction");
                            let mut c = TokioCommand::new("tar");
                            c.arg("-xf")
                                .arg(path.to_string_lossy().to_string())
                                .arg("-C")
                                .arg(parent.to_string_lossy().to_string());
                            c
                        } else if has_7z {
                            crate::app::log_debug("Using 7z for extraction");
                            let mut c = TokioCommand::new("7z");
                            c.arg("x")
                                .arg(path.to_string_lossy().to_string())
                                .arg(format!("-o{}", parent.to_string_lossy()));
                            c
                        } else if has_tar {
                            crate::app::log_debug("Using tar (fallback) for extraction");
                            let mut c = TokioCommand::new("tar");
                            c.arg("-xf")
                                .arg(path.to_string_lossy().to_string())
                                .arg("-C")
                                .arg(parent.to_string_lossy().to_string());
                            c
                        } else {
                            crate::app::log_debug("No extraction tool found");
                            let _ = tx.try_send(AppEvent::StatusMsg("Error: No suitable extraction tool found (ark/file-roller/zip/tar/7z)".to_string()));
                            let _ = tx.try_send(AppEvent::TaskFinished(task_id));
                            return;
                        };

                        let mut child = match child_cmd
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .spawn()
                        {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                    "Failed to spawn tool: {}",
                                    e
                                )));
                                let _ = tx.try_send(AppEvent::TaskFinished(task_id));
                                return;
                            }
                        };

                        let mut current_progress = 0.0;
                        let res = loop {
                            tokio::select! {
                                status = child.wait() => {
                                    break status;
                                }
                                _ = sleep(Duration::from_millis(500)) => {
                                    if current_progress < 0.9 {
                                        current_progress += 0.05;
                                        let _ = tx.try_send(AppEvent::TaskProgress(task_id, current_progress, format!("Extracting {}... ({:.0}%)", filename, current_progress * 100.0)));
                                    }
                                }
                            }
                        };

                        match res {
                            Ok(status) if status.success() => {
                                let _ = tx.try_send(AppEvent::TaskProgress(
                                    task_id,
                                    1.0,
                                    "Extracted".to_string(),
                                ));
                                sleep(Duration::from_millis(800)).await;
                                let _ = tx.try_send(AppEvent::TaskFinished(task_id));
                                let _ = tx.try_send(AppEvent::RefreshFiles(p_idx));
                            }
                            Ok(status) => {
                                let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                    "Extraction failed with status: {}",
                                    status
                                )));
                                let _ = tx.try_send(AppEvent::TaskFinished(task_id));
                            }
                            Err(e) => {
                                let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                    "Error waiting for tool: {}",
                                    e
                                )));
                                let _ = tx.try_send(AppEvent::TaskFinished(task_id));
                            }
                        }
                    });
                }
            }
        }
        ContextMenuAction::GitInit => {
            let working_dir = match target {
                ContextMenuTarget::Folder(idx) => app
                    .current_file_state()
                    .and_then(|fs| fs.files.get(*idx).cloned()),
                ContextMenuTarget::EmptySpace => {
                    app.current_file_state().map(|fs| fs.current_path.clone())
                }
                _ => None,
            };
            if let Some(path) = working_dir {
                let tx = event_tx.clone();
                let p_idx = app.focused_pane_index;
                tokio::spawn(async move {
                    let res = std::process::Command::new("git")
                        .arg("init")
                        .current_dir(&path)
                        .output();
                    match res {
                        Ok(out) if out.status.success() => {
                            let _ = tx.try_send(AppEvent::StatusMsg(
                                "Initialized Git repository".to_string(),
                            ));
                            let _ = tx.try_send(AppEvent::RefreshFiles(p_idx));
                        }
                        Ok(out) => {
                            let err = String::from_utf8_lossy(&out.stderr);
                            let _ = tx
                                .try_send(AppEvent::StatusMsg(format!("Git init failed: {}", err)));
                        }
                        Err(e) => {
                            let _ = tx
                                .try_send(AppEvent::StatusMsg(format!("Error running git: {}", e)));
                        }
                    }
                });
            }
        }
        ContextMenuAction::Drag => {
            if let ContextMenuTarget::File(idx) = target {
                if let Some(fs) = app.current_file_state() {
                    // Collect selected files or just the target
                    let mut paths = Vec::new();
                    if fs.selection.multi.contains(idx) {
                        for &i in fs.selection.multi_selected_indices() {
                            if let Some(p) = fs.files.get(i) {
                                paths.push(p.clone());
                            }
                        }
                    } else if let Some(p) = fs.files.get(*idx) {
                        paths.push(p.clone());
                    }

                    if !paths.is_empty() {
                        let tool = if terma::utils::command_exists("ripdrag") {
                            "ripdrag"
                        } else {
                            "dragon"
                        };
                        
                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!("Launching {}...", tool)));
                        
                        let args: Vec<String> = paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
                        terma::utils::spawn_detached(tool, args);
                    }
                }
            }
        }
        ContextMenuAction::GitStatus => {
            let working_dir = match target {
                ContextMenuTarget::Folder(idx) => app
                    .current_file_state()
                    .and_then(|fs| fs.files.get(*idx).cloned()),
                ContextMenuTarget::EmptySpace => {
                    app.current_file_state().map(|fs| fs.current_path.clone())
                }
                _ => None,
            };
            if let Some(path) = working_dir {
                let tx = event_tx.clone();
                tokio::spawn(async move {
                    let res = std::process::Command::new("git")
                        .arg("status")
                        .arg("--short")
                        .current_dir(&path)
                        .output();
                    match res {
                        Ok(out) if out.status.success() => {
                            let status = String::from_utf8_lossy(&out.stdout);
                            if status.is_empty() {
                                let _ = tx.try_send(AppEvent::StatusMsg("Git: Clean".to_string()));
                            } else {
                                let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                    "Git: {}",
                                    status.trim().replace('\n', ", ")
                                )));
                            }
                        }
                        Ok(_) => {
                            let _ = tx
                                .try_send(AppEvent::StatusMsg("Not a git repository".to_string()));
                        }
                        Err(e) => {
                            let _ = tx.try_send(AppEvent::StatusMsg(format!(
                                "Error checking git: {}",
                                e
                            )));
                        }
                    }
                });
            }
        }
        ContextMenuAction::SetColor(_) => {
            app.mode = AppMode::Highlight;
            return;
        }
        _ => {}
    }
    app.mode = AppMode::Normal;
}

pub fn get_open_with_suggestions(app: &App, ext: &str) -> Vec<String> {
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
