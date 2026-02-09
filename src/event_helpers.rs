use crate::app::{
    App, AppEvent, AppMode, CommandAction, CommandItem, ContextMenuAction, ContextMenuTarget,
    CurrentView, FileState,
};
use crate::config::save_state;
use std::path::PathBuf;
use tokio::sync::mpsc;

pub fn update_commands(app: &mut App) {
    let mut commands = vec![
        CommandItem {
            key: "q".to_string(),
            desc: "Quit".to_string(),
            action: CommandAction::Quit,
        },
        CommandItem {
            key: "z".to_string(),
            desc: "Toggle Zoom".to_string(),
            action: CommandAction::ToggleZoom,
        },
        CommandItem {
            key: "f".to_string(),
            desc: "File Manager".to_string(),
            action: CommandAction::SwitchView(CurrentView::Files),
        },
        CommandItem {
            key: "e".to_string(),
            desc: "Editor".to_string(),
            action: CommandAction::SwitchView(CurrentView::Editor),
        },
        CommandItem {
            key: "g".to_string(),
            desc: "Git".to_string(),
            action: CommandAction::SwitchView(CurrentView::Git),
        },
        CommandItem {
            key: "m".to_string(),
            desc: "Monitor".to_string(),
            action: CommandAction::SwitchView(CurrentView::Processes),
        },
        CommandItem {
            key: "a".to_string(),
            desc: "Add Remote".to_string(),
            action: CommandAction::AddRemote,
        },
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

                    // Toggle Add/Remove Favorites
                    if app.starred.contains(path) {
                        actions.push(ContextMenuAction::RemoveFromFavorites);
                    } else {
                        actions.push(ContextMenuAction::AddToFavorites);
                    }
                }
            }

            actions.extend(vec![
                ContextMenuAction::SetColor(None),
                ContextMenuAction::Separator,
                ContextMenuAction::Properties,
            ]);
            actions
        }
        ContextMenuTarget::Folder(idx) => {
            let mut actions = vec![
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
            ];

            if let Some(fs) = app.current_file_state() {
                if let Some(path) = fs.files.get(*idx) {
                    // Toggle Add/Remove Favorites
                    if app.starred.contains(path) {
                        actions.push(ContextMenuAction::RemoveFromFavorites);
                    } else {
                        actions.push(ContextMenuAction::AddToFavorites);
                    }
                }
            }

            actions.extend(vec![
                ContextMenuAction::Compress,
                ContextMenuAction::SetColor(None),
                ContextMenuAction::Separator,
                ContextMenuAction::Properties,
            ]);
            actions
        }
        ContextMenuTarget::SidebarFavorite(_) => vec![
            ContextMenuAction::Open,
            ContextMenuAction::OpenNewTab,
            ContextMenuAction::Separator,
            ContextMenuAction::RemoveFromFavorites,
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
                let path_opt = app
                    .current_file_state()
                    .and_then(|fs| fs.files.get(*idx).cloned());
                if let Some(path) = path_opt {
                    if path.is_dir() {
                        let path_clone = path.clone();
                        if let Some(fs_mut) = app.current_file_state_mut() {
                            fs_mut.current_path = path_clone;
                            let _ =
                                event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                        }
                    } else {
                        let _ = event_tx.try_send(AppEvent::PreviewRequested(
                            app.focused_pane_index,
                            path.clone(),
                        ));
                    }
                }
            }
        }
        ContextMenuAction::AddToFavorites => {
            crate::app::log_debug(&format!(
                "DEBUG: AddToFavorites action triggered with target {:?}",
                target
            ));
            if let ContextMenuTarget::Folder(idx) | ContextMenuTarget::File(idx) = target {
                let path_opt = app
                    .current_file_state()
                    .and_then(|fs| fs.files.get(*idx).cloned());
                if let Some(path) = path_opt {
                    crate::app::log_debug(&format!("DEBUG: Adding path to favorites: {:?}", path));
                    if !app.starred.contains(&path) {
                        app.starred.push(path);
                        if let Err(e) = save_state(app) {
                            crate::app::log_debug(&format!(
                                "DEBUG: Failed to save state after adding favorite: {}",
                                e
                            ));
                        } else {
                            crate::app::log_debug(
                                "DEBUG: State saved successfully after adding favorite",
                            );
                        }
                        // Refresh to update sidebar
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                    } else {
                        crate::app::log_debug("DEBUG: Path already in favorites");
                    }
                }
            }
        }
        ContextMenuAction::RemoveFromFavorites => {
            let mut removed = false;
            match target {
                ContextMenuTarget::SidebarFavorite(path) => {
                    let path_clone = path.clone();
                    app.starred.retain(|p| p != &path_clone);
                    removed = true;
                }
                ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) => {
                    if let Some(fs) = app.current_file_state() {
                        if let Some(path) = fs.files.get(*idx) {
                            let path_clone = path.clone();
                            if app.starred.contains(&path_clone) {
                                app.starred.retain(|p| p != &path_clone);
                                removed = true;
                            }
                        }
                    }
                }
                _ => {}
            }
            if removed {
                let _ = save_state(app);
                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
            }
        }
        ContextMenuAction::Rename => {
            if let ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) = target {
                let path_opt = app
                    .current_file_state()
                    .and_then(|fs| fs.files.get(*idx).cloned());
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
                let path_opt = app
                    .current_file_state()
                    .and_then(|fs| fs.files.get(*idx).cloned());
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
        ContextMenuAction::TerminalTab | ContextMenuAction::TerminalWindow => {
            let new_tab = matches!(action, ContextMenuAction::TerminalTab);
            let mut path_to_open = None;
            let mut remote = None;

            if let Some(fs) = app.current_file_state() {
                remote = fs.remote_session.clone();
            }

            match target {
                ContextMenuTarget::Folder(idx) => {
                    if let Some(fs) = app.current_file_state() {
                        path_to_open = fs.files.get(*idx).cloned();
                    }
                }
                ContextMenuTarget::EmptySpace => {
                    if let Some(fs) = app.current_file_state() {
                        path_to_open = Some(fs.current_path.clone());
                    }
                }
                ContextMenuTarget::ProjectTree(p) => {
                    path_to_open = Some(p.clone());
                }
                _ => {}
            }

            if let Some(path) = path_to_open {
                let _ = event_tx.try_send(AppEvent::SpawnTerminal {
                    path,
                    new_tab,
                    remote,
                    command: None,
                });
            }
        }
        ContextMenuAction::OpenNewTab => {
            if let ContextMenuTarget::Folder(idx) = target {
                if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                    if let Some(fs) = pane.current_state() {
                        if let Some(path) = fs.files.get(*idx).cloned() {
                            let mut new_fs = fs.clone();
                            new_fs.current_path = path;
                            new_fs.selection.clear();
                            let current_path_clone = new_fs.current_path.clone();
                            crate::event_helpers::push_history(&mut new_fs, current_path_clone);
                            pane.open_tab(new_fs);
                            let _ =
                                event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                        }
                    }
                }
            }
        }
        ContextMenuAction::NewFile | ContextMenuAction::NewFolder => {
            let mut target_dir = app.current_file_state().map(|fs| fs.current_path.clone());
            match target {
                ContextMenuTarget::Folder(idx) => {
                    if let Some(fs) = app.current_file_state() {
                        if let Some(p) = fs.files.get(*idx) {
                            target_dir = Some(p.clone());
                        }
                    }
                }
                ContextMenuTarget::File(idx) => {
                    if let Some(fs) = app.current_file_state() {
                        if let Some(p) = fs.files.get(*idx) {
                            target_dir = p.parent().map(|pp| pp.to_path_buf());
                        }
                    }
                }
                ContextMenuTarget::ProjectTree(path) => {
                    if path.is_dir() {
                        target_dir = Some(path.clone());
                    } else {
                        target_dir = path.parent().map(|pp| pp.to_path_buf());
                    }
                }
                ContextMenuTarget::EmptySpace => {}
                _ => {}
            }
            if let (Some(fs), Some(dir)) = (app.current_file_state_mut(), target_dir) {
                fs.current_path = dir;
            }
            app.mode = if matches!(action, ContextMenuAction::NewFolder) {
                AppMode::NewFolder
            } else {
                AppMode::NewFile
            };
            app.input.clear();
            app.rename_selected = false;
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
        offset.saturating_add(rel_row)
    } else {
        0
    }
}

pub fn get_open_with_suggestions(_app: &App, ext: &str) -> Vec<String> {
    terma::utils::get_open_with_suggestions(ext)
}

pub fn navigate_up(app: &mut App) {
    if let Some(fs) = app.current_file_state_mut() {
        if let Some(parent) = fs.current_path.parent() {
            // Store the folder we're leaving so we can select it after refresh
            let old_folder = fs.current_path.clone();
            let parent = parent.to_path_buf();
            fs.current_path = parent.clone();
            fs.pending_select_path = Some(old_folder);
            push_history(fs, parent);
        }
    }
}
