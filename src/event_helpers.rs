use std::path::PathBuf;
use tokio::sync::mpsc;
use crate::app::{App, AppEvent, AppMode, CurrentView, ContextMenuAction, ContextMenuTarget, CommandAction, FileColumn, SelectionState, CommitInfo, GitStatus, FileState, CommandItem};
use crate::config::save_state;

pub fn update_commands(app: &mut App) {
    let mut commands = vec![
        CommandItem { key: "q".to_string(), desc: "Quit".to_string(), action: CommandAction::Quit },
        CommandItem { key: "z".to_string(), desc: "Toggle Zoom".to_string(), action: CommandAction::ToggleZoom },
        CommandItem { key: "f".to_string(), desc: "File Manager".to_string(), action: CommandAction::SwitchView(CurrentView::Files) },
        CommandItem { key: "e".to_string(), desc: "Editor".to_string(), action: CommandAction::SwitchView(CurrentView::Editor) },
        CommandItem { key: "g".to_string(), desc: "Git".to_string(), action: CommandAction::SwitchView(CurrentView::Git) },
        CommandItem { key: "m".to_string(), desc: "Monitor".to_string(), action: CommandAction::SwitchView(CurrentView::Processes) },
        CommandItem { key: "a".to_string(), desc: "Add Remote".to_string(), action: CommandAction::AddRemote },
    ];

    for (i, bookmark) in app.remote_bookmarks.iter().enumerate() {
        commands.push(CommandItem {
            key: format!("r{}", i),
            desc: format!("Connect to {}", bookmark.name),
            action: CommandAction::ConnectToRemote(i),
        });
    }

    app.filtered_commands = commands
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
        CommandAction::ToggleZoom => app.toggle_split(),
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

            actions.extend(vec![
                ContextMenuAction::AddToFavorites,
                ContextMenuAction::SetColor(None),
                ContextMenuAction::Separator,
                ContextMenuAction::Properties,
            ]);
            actions
        }
        ContextMenuTarget::Folder(_) => vec![
            ContextMenuAction::Open,
            ContextMenuAction::OpenNewTab,
            ContextMenuAction::TerminalTab,
            ContextMenuAction::TerminalWindow,
            ContextMenuAction::Separator,
            ContextMenuAction::Cut,
            ContextMenuAction::Copy,
            ContextMenuAction::CopyPath,
            ContextMenuAction::CopyName,
            ContextMenuAction::Separator,
            ContextMenuAction::Rename,
            ContextMenuAction::Delete,
            ContextMenuAction::Separator,
            ContextMenuAction::AddToFavorites,
            ContextMenuAction::Compress,
            ContextMenuAction::SetColor(None),
            ContextMenuAction::Separator,
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
        ContextMenuTarget::Process(_) => vec![
            ContextMenuAction::Delete, // Kill
            ContextMenuAction::Properties,
        ],
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
            if let ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) = target {
                let path_opt = app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned());
                if let Some(path) = path_opt {
                    if path.is_dir() {
                        let path_clone = path.clone();
                        if let Some(fs_mut) = app.current_file_state_mut() {
                            fs_mut.current_path = path_clone;
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                        }
                    } else {
                        let _ = event_tx.try_send(AppEvent::PreviewRequested(app.focused_pane_index, path.clone()));
                    }
                }
            }
        }
        ContextMenuAction::AddToFavorites => {
            crate::app::log_debug(&format!("DEBUG: AddToFavorites action triggered with target {:?}", target));
            if let ContextMenuTarget::Folder(idx) | ContextMenuTarget::File(idx) = target {
                let path_opt = app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned());
                if let Some(path) = path_opt {
                    crate::app::log_debug(&format!("DEBUG: Adding path to favorites: {:?}", path));
                    if !app.starred.contains(&path) {
                        app.starred.push(path);
                        if let Err(e) = save_state(app) {
                            crate::app::log_debug(&format!("DEBUG: Failed to save state after adding favorite: {}", e));
                        } else {
                            crate::app::log_debug("DEBUG: State saved successfully after adding favorite");
                        }
                    } else {
                        crate::app::log_debug("DEBUG: Path already in favorites");
                    }
                }
            }
        }
        ContextMenuAction::RemoveFromFavorites => {
            if let ContextMenuTarget::SidebarFavorite(path) = target {
                let path_clone = path.clone();
                app.starred.retain(|p| p != &path_clone);
                let _ = save_state(app);
            }
        }
        ContextMenuAction::Rename => {
            if let ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) = target {
                let path_opt = app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned());
                if let Some(path) = path_opt {
                    if let Some(name) = path.file_name() {
                        let name_str = name.to_string_lossy().to_string();
                        app.mode = AppMode::Rename;
                        app.input.set_value(name_str);
                    }
                }
            }
        }
        ContextMenuAction::Delete => {
            if let ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) = target {
                let path_opt = app.current_file_state().and_then(|fs| fs.files.get(*idx).cloned());
                if let Some(path) = path_opt {
                    let _ = event_tx.try_send(AppEvent::Delete(path.clone()));
                }
            }
        }
        ContextMenuAction::Refresh => {
            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
        }
        ContextMenuAction::ToggleHidden => {
            if let Some(fs) = app.current_file_state_mut() {
                fs.show_hidden = !fs.show_hidden;
                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
            }
        }
        _ => {}
    }
}

pub fn navigate_back(app: &mut App) {
    if let Some(fs) = app.current_file_state_mut() {
        if fs.history_index > 0 {
            fs.history_index -= 1;
            fs.current_path = fs.history[fs.history_index].clone();
        }
    }
}

pub fn navigate_forward(app: &mut App) {
    if let Some(fs) = app.current_file_state_mut() {
        if fs.history_index + 1 < fs.history.len() {
            fs.history_index += 1;
            fs.current_path = fs.history[fs.history_index].clone();
        }
    }
}

pub fn push_history(fs: &mut FileState, path: PathBuf) {
    if fs.history_index + 1 < fs.history.len() {
        fs.history.truncate(fs.history_index + 1);
    }
    if fs.history.last() != Some(&path) {
        fs.history.push(path);
        fs.history_index = fs.history.len() - 1;
    }
}

pub fn fs_mouse_index(row: u16, app: &App) -> usize {
    if let Some(fs) = app.current_file_state() {
        let offset = fs.table_state.offset();
        let rel_row = row.saturating_sub(3) as usize;
        offset + rel_row
    } else { 0 }
}

pub fn get_open_with_suggestions(_app: &App, ext: &str) -> Vec<String> {
    terma::utils::get_open_with_suggestions(ext)
}

pub fn navigate_up(app: &mut App) {
    if let Some(fs) = app.current_file_state_mut() {
        if let Some(parent) = fs.current_path.parent() {
            let parent = parent.to_path_buf();
            fs.current_path = parent.clone();
            push_history(fs, parent);
        }
    }
}
