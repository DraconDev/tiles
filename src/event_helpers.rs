use crate::app::{
    App, AppEvent, AppMode, CommandAction, CommandItem, ContextMenuAction, ContextMenuTarget,
    CurrentView, FileState,
};
use base64::Engine;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::{Command, Stdio};
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
        ContextMenuTarget::SidebarFavorite(_path) => {
            let actions = vec![
                ContextMenuAction::Open,
                ContextMenuAction::OpenNewTab,
                ContextMenuAction::Separator,
                ContextMenuAction::NewFile,
                ContextMenuAction::NewFolder,
                ContextMenuAction::Separator,
                ContextMenuAction::TerminalTab,
                ContextMenuAction::TerminalWindow,
                ContextMenuAction::Separator,
                ContextMenuAction::RemoveFromFavorites,
                ContextMenuAction::Separator,
                ContextMenuAction::Properties,
            ];
            actions
        }
        ContextMenuTarget::EmptySpace => {
            let mut actions = vec![ContextMenuAction::NewFile, ContextMenuAction::NewFolder];
            if app.clipboard.is_some() {
                actions.push(ContextMenuAction::Paste);
            }
            actions.extend(vec![
                ContextMenuAction::Separator,
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
            if let ContextMenuTarget::Folder(idx) | ContextMenuTarget::File(idx) = target {
                let path_opt = app
                    .current_file_state()
                    .and_then(|fs| fs.files.get(*idx).cloned());
                if let Some(path) = path_opt {
                    if !app.starred.contains(&path) {
                        app.starred.push(path);
                        crate::config::save_state_quiet(app);
                        // Refresh to update sidebar
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
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
                crate::config::save_state_quiet(app);
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
        ContextMenuAction::CopyPath | ContextMenuAction::CopyName => {
            match copy_target_text(action, target, app) {
                Ok(text) => match copy_text_to_clipboard(&text) {
                    Ok(()) => {
                        let label = if matches!(action, ContextMenuAction::CopyName) {
                            "name"
                        } else {
                            "path"
                        };
                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                            "Copied {} to clipboard",
                            label
                        )));
                    }
                    Err(err) => {
                        let _ = event_tx
                            .try_send(AppEvent::StatusMsg(format!("Clipboard failed: {}", err)));
                    }
                },
                Err(err) => {
                    let _ = event_tx.try_send(AppEvent::StatusMsg(err));
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
                ContextMenuTarget::ProjectTree(path) | ContextMenuTarget::SidebarFavorite(path) => {
                    if path.is_dir() {
                        target_dir = Some(path.clone());
                    } else {
                        target_dir = path.parent().map(|pp| pp.to_path_buf());
                    }
                }
                ContextMenuTarget::SidebarRemote(idx) => {
                    if let Some(bookmark) = app.remote_bookmarks.get(*idx) {
                        target_dir = Some(bookmark.last_path.clone());
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
        ContextMenuAction::Cut => {
            if let ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) = target {
                let path_opt = app
                    .current_file_state()
                    .and_then(|fs| fs.files.get(*idx).cloned());
                if let Some(path) = path_opt {
                    app.clipboard = Some((path, crate::app::ClipboardOp::Cut));
                }
            }
        }
        ContextMenuAction::Copy => {
            if let ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) = target {
                let path_opt = app
                    .current_file_state()
                    .and_then(|fs| fs.files.get(*idx).cloned());
                if let Some(path) = path_opt {
                    app.clipboard = Some((path, crate::app::ClipboardOp::Copy));
                }
            }
        }
        ContextMenuAction::Paste => {
            if let Some((src, op)) = app.clipboard.clone() {
                let target_dir = match target {
                    ContextMenuTarget::Folder(idx) => {
                        app.current_file_state()
                            .and_then(|fs| fs.files.get(*idx).cloned())
                    }
                    ContextMenuTarget::SidebarFavorite(path) => Some(path.clone()),
                    ContextMenuTarget::EmptySpace => {
                        app.current_file_state().map(|fs| fs.current_path.clone())
                    }
                    _ => app.current_file_state().map(|fs| fs.current_path.clone()),
                };
                if let Some(dest_dir) = target_dir {
                    let dest = dest_dir.join(
                        src.file_name()
                            .unwrap_or_else(|| OsStr::new("root")),
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
        ContextMenuAction::Compress => {
            if let ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) = target {
                let path_opt = app
                    .current_file_state()
                    .and_then(|fs| fs.files.get(*idx).cloned());
                if let Some(path) = path_opt {
                    let name = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("archive");
                    let dest = path.parent()
                        .map(|p| p.join(format!("{}.zip", name)));
                    if let Some(dest_path) = dest {
                        let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                            "Compressing {}...",
                            path.display()
                        )));
                        let _ = event_tx.try_send(AppEvent::Copy(path.clone(), dest_path));
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
                    let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                        "ExtractHere not yet implemented for {}",
                        path.display()
                    )));
                }
            }
        }
        ContextMenuAction::Duplicate => {
            if let ContextMenuTarget::File(idx) = target {
                let path_opt = app
                    .current_file_state()
                    .and_then(|fs| fs.files.get(*idx).cloned());
                if let Some(path) = path_opt {
                    if let Some(parent) = path.parent() {
                        let stem = path.file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("file");
                        let ext = path.extension()
                            .and_then(|e| e.to_str())
                            .map(|s| format!(".{}", s))
                            .unwrap_or_default();
                        let new_name = format!("{}_copy{}", stem, ext);
                        let dest = parent.join(new_name);
                        let _ = event_tx.try_send(AppEvent::Copy(path, dest));
                    }
                }
            }
        }
        ContextMenuAction::SystemMonitor => {
            let _ = event_tx.try_send(AppEvent::SystemMonitor);
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
    const MAX_HISTORY: usize = 50;
    if fs.history.len() > MAX_HISTORY {
        let excess = fs.history.len() - MAX_HISTORY;
        fs.history.drain(0..excess);
        fs.history_index = fs.history_index.saturating_sub(excess);
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
    dracon_terminal_engine::utils::get_open_with_suggestions(ext)
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

pub fn open_path_input(app: &mut App) {
    let value = app
        .current_file_state()
        .map(|fs| fs.current_path.to_string_lossy().to_string())
        .unwrap_or_default();
    app.input.set_value(value);
    app.input.cursor_position = app.input.value.len();
    // Style input to match breadcrumb look
    app.input.style = ratatui::style::Style::default()
        .fg(crate::ui::theme::accent_secondary())
        .add_modifier(ratatui::style::Modifier::BOLD);
    app.input.cursor_style = ratatui::style::Style::default()
        .bg(crate::ui::theme::accent_secondary())
        .fg(ratatui::style::Color::Black);
    app.mode = AppMode::PathInput;
    // No input shield — mouse escape sequences from the click that opened
    // PathInput are already consumed by the time the user starts typing.
}

pub fn submit_path_input(app: &mut App, event_tx: &mpsc::Sender<AppEvent>) -> Result<(), String> {
    let t0 = std::time::Instant::now();
    let input = app.input.value.trim().to_string();
    if input.is_empty() {
        return Err("Path is empty".to_string());
    }

    let focused = app.focused_pane_index;
    let Some(fs) = app.current_file_state_mut() else {
        return Err("No active file pane".to_string());
    };

    let remote = fs.remote_session.is_some();
    let target = resolve_path_input(&input, &fs.current_path, remote);

    fs.current_path = target.clone();
    fs.pending_select_path = None;
    fs.selection.clear();
    fs.search_filter.clear();
    *fs.table_state.offset_mut() = 0;
    push_history(fs, target);

    let _ = event_tx.try_send(AppEvent::RefreshFiles(focused));
    crate::app::log_debug(&format!("submit_path_input: {:?}", t0.elapsed()));
    Ok(())
}

pub fn copy_text_to_clipboard(text: &str) -> Result<(), String> {
    let attempts: [(&str, &[&str]); 4] = [
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
        ("pbcopy", &[]),
    ];

    let mut last_err = None;
    for (cmd, args) in attempts {
        match Command::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(mut child) => {
                if let Some(stdin) = child.stdin.as_mut() {
                    use std::io::Write;
                    if stdin.write_all(text.as_bytes()).is_err() {
                        last_err = Some(format!("{} rejected clipboard data", cmd));
                        let _ = child.kill();
                        continue;
                    }
                }

                match child.wait() {
                    Ok(status) if status.success() => return Ok(()),
                    Ok(_) => last_err = Some(format!("{} exited unsuccessfully", cmd)),
                    Err(err) => last_err = Some(format!("{} failed: {}", cmd, err)),
                }
            }
            Err(err) => {
                if err.kind() != std::io::ErrorKind::NotFound {
                    last_err = Some(format!("{} failed: {}", cmd, err));
                }
            }
        }
    }

    copy_text_to_clipboard_via_osc52(text).map_err(|osc_err| {
        let fallback = last_err.unwrap_or_else(|| {
            "No clipboard helper found (tried wl-copy, xclip, xsel, pbcopy)".to_string()
        });
        format!("{}; OSC 52 fallback failed: {}", fallback, osc_err)
    })
}

pub fn copy_text_to_clipboard_async(text: String) {
    std::thread::spawn(move || {
        let _ = copy_text_to_clipboard(&text);
    });
}

fn copy_text_to_clipboard_via_osc52(text: &str) -> Result<(), String> {
    use std::io::Write;

    let term = std::env::var("TERM").unwrap_or_default();
    if term == "dumb" {
        return Err("terminal does not support clipboard escape sequences".to_string());
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    let sequence = format!("\u{1b}]52;c;{}\u{07}", encoded);

    let mut stdout = std::io::stdout();
    stdout
        .write_all(sequence.as_bytes())
        .map_err(|err| format!("write failed: {}", err))?;
    stdout
        .flush()
        .map_err(|err| format!("flush failed: {}", err))?;
    Ok(())
}

fn copy_target_text(
    action: &ContextMenuAction,
    target: &ContextMenuTarget,
    app: &App,
) -> Result<String, String> {
    let path = match target {
        ContextMenuTarget::File(idx) | ContextMenuTarget::Folder(idx) => app
            .current_file_state()
            .and_then(|fs| fs.files.get(*idx))
            .cloned()
            .ok_or_else(|| "No file selected".to_string())?,
        ContextMenuTarget::SidebarFavorite(path) | ContextMenuTarget::ProjectTree(path) => {
            path.clone()
        }
        _ => return Err("Nothing here supports path copy".to_string()),
    };

    if matches!(action, ContextMenuAction::CopyName) {
        Ok(path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string()))
    } else {
        Ok(path.to_string_lossy().to_string())
    }
}

fn resolve_path_input(input: &str, current_path: &std::path::Path, remote: bool) -> PathBuf {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return current_path.to_path_buf();
    }

    if !remote && trimmed == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }

    if !remote {
        if let Some(rest) = trimmed.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(rest);
            }
        }
    }

    let typed = PathBuf::from(trimmed);
    if typed.is_absolute() {
        typed
    } else {
        current_path.join(typed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::FileState;
    use std::path::PathBuf;

    fn make_fs(path: &str) -> FileState {
        FileState::new(
            PathBuf::from(path),
            None,
            false,
            vec![crate::state::FileColumn::Name],
            crate::state::FileColumn::Name,
            true,
        )
    }

    // ── resolve_path_input ──────────────────────────────────────

    #[test]
    fn resolve_empty_returns_current() {
        let current = PathBuf::from("/home/user");
        let result = resolve_path_input("", &current, false);
        assert_eq!(result, current);
    }

    #[test]
    fn resolve_whitespace_returns_current() {
        let current = PathBuf::from("/home/user");
        let result = resolve_path_input("   ", &current, false);
        assert_eq!(result, current);
    }

    #[test]
    fn resolve_absolute_path() {
        let current = PathBuf::from("/home/user");
        let result = resolve_path_input("/etc/config", &current, false);
        assert_eq!(result, PathBuf::from("/etc/config"));
    }

    #[test]
    fn resolve_relative_path() {
        let current = PathBuf::from("/home/user");
        let result = resolve_path_input("projects/tiles", &current, false);
        assert_eq!(result, PathBuf::from("/home/user/projects/tiles"));
    }

    #[test]
    fn resolve_parent_dotdot() {
        let current = PathBuf::from("/home/user/projects");
        // resolve_path_input does simple join, doesn't normalize
        let result = resolve_path_input("..", &current, false);
        assert_eq!(result, PathBuf::from("/home/user/projects/.."));
    }

    #[test]
    fn resolve_absolute_ignores_remote_flag() {
        let current = PathBuf::from("/home/user");
        let result = resolve_path_input("/var/log", &current, true);
        assert_eq!(result, PathBuf::from("/var/log"));
    }

    // ── push_history ────────────────────────────────────────────

    #[test]
    fn push_history_basic() {
        let mut fs = make_fs("/home");
        push_history(&mut fs, PathBuf::from("/home"));
        assert_eq!(fs.history.len(), 1);
        assert_eq!(fs.history_index, 0);
    }

    #[test]
    fn push_history_deduplicates_consecutive() {
        let mut fs = make_fs("/home");
        push_history(&mut fs, PathBuf::from("/home"));
        push_history(&mut fs, PathBuf::from("/home"));
        assert_eq!(fs.history.len(), 1);
    }

    #[test]
    fn push_history_allows_different_paths() {
        let mut fs = make_fs("/home");
        push_history(&mut fs, PathBuf::from("/home"));
        push_history(&mut fs, PathBuf::from("/etc"));
        push_history(&mut fs, PathBuf::from("/var"));
        assert_eq!(fs.history.len(), 3);
        assert_eq!(fs.history_index, 2);
    }

    #[test]
    fn push_history_caps_at_50() {
        let mut fs = make_fs("/");
        for i in 0..60 {
            push_history(&mut fs, PathBuf::from(format!("/dir{}", i)));
        }
        assert_eq!(fs.history.len(), 50);
        // Most recent entries should be preserved
        assert_eq!(fs.history.last().unwrap(), &PathBuf::from("/dir59"));
    }

    #[test]
    fn push_history_index_stays_valid_after_cap() {
        let mut fs = make_fs("/");
        for i in 0..55 {
            push_history(&mut fs, PathBuf::from(format!("/dir{}", i)));
        }
        assert!(fs.history_index < fs.history.len());
    }

    #[test]
    fn push_history_truncates_future_on_new_entry() {
        let mut fs = make_fs("/"); // history starts with ["/"]
        push_history(&mut fs, PathBuf::from("/a")); // ["/", "/a"]
        push_history(&mut fs, PathBuf::from("/b")); // ["/", "/a", "/b"]
        push_history(&mut fs, PathBuf::from("/c")); // ["/", "/a", "/b", "/c"]
                                                    // Simulate going back to "/b"
        fs.history_index = 2;
        // Push new entry should truncate "/c"
        push_history(&mut fs, PathBuf::from("/d"));
        assert_eq!(fs.history.len(), 4);
        assert_eq!(fs.history[0], PathBuf::from("/"));
        assert_eq!(fs.history[1], PathBuf::from("/a"));
        assert_eq!(fs.history[2], PathBuf::from("/b"));
        assert_eq!(fs.history[3], PathBuf::from("/d"));
    }

    // ── push_recent_folder ──────────────────────────────────────

    #[test]
    fn push_recent_folder_adds_to_front() {
        let mut app = crate::app::App::new(std::sync::Arc::new(std::sync::Mutex::new(Vec::new())));
        app.push_recent_folder(PathBuf::from("/home/user"));
        app.push_recent_folder(PathBuf::from("/etc"));
        assert_eq!(app.recent_folders[0], PathBuf::from("/etc"));
        assert_eq!(app.recent_folders[1], PathBuf::from("/home/user"));
    }

    #[test]
    fn push_recent_folder_deduplicates() {
        let mut app = crate::app::App::new(std::sync::Arc::new(std::sync::Mutex::new(Vec::new())));
        app.push_recent_folder(PathBuf::from("/home"));
        app.push_recent_folder(PathBuf::from("/etc"));
        app.push_recent_folder(PathBuf::from("/home")); // Move to front
        assert_eq!(app.recent_folders.len(), 2);
        assert_eq!(app.recent_folders[0], PathBuf::from("/home"));
    }

    #[test]
    fn push_recent_folder_caps_at_10() {
        let mut app = crate::app::App::new(std::sync::Arc::new(std::sync::Mutex::new(Vec::new())));
        for i in 0..15 {
            app.push_recent_folder(PathBuf::from(format!("/dir{}", i)));
        }
        assert_eq!(app.recent_folders.len(), 10);
        assert_eq!(app.recent_folders[0], PathBuf::from("/dir14"));
    }

    // ── breadcrumb bounds isolation ─────────────────────────────

    #[test]
    fn breadcrumb_header_bounds_only_one_row() {
        use ratatui::layout::Rect;
        // Simulate a 80x24 pane area
        let area = Rect::new(0, 0, 80, 24);
        let breadcrumb_bounds = Rect::new(area.x, area.y, area.width, 1);

        // Breadcrumb row (y=0) should be inside
        assert!(breadcrumb_bounds.contains(ratatui::layout::Position { x: 40, y: 0 }));

        // File rows (y=3+) should NOT be inside
        assert!(!breadcrumb_bounds.contains(ratatui::layout::Position { x: 40, y: 3 }));
        assert!(!breadcrumb_bounds.contains(ratatui::layout::Position { x: 40, y: 10 }));
        assert!(!breadcrumb_bounds.contains(ratatui::layout::Position { x: 40, y: 23 }));
    }

    #[test]
    fn breadcrumb_bounds_excludes_column_headers() {
        use ratatui::layout::Rect;
        let area = Rect::new(10, 5, 60, 20);
        let breadcrumb_bounds = Rect::new(area.x, area.y, area.width, 1);

        // Breadcrumb at y=5
        assert!(breadcrumb_bounds.contains(ratatui::layout::Position { x: 30, y: 5 }));
        // Column headers at y=6, y=7 - should NOT match
        assert!(!breadcrumb_bounds.contains(ratatui::layout::Position { x: 30, y: 6 }));
        assert!(!breadcrumb_bounds.contains(ratatui::layout::Position { x: 30, y: 7 }));
    }

    // ── resolve_path_input edge cases ───────────────────────────

    #[test]
    fn resolve_relative_with_trailing_slash() {
        let current = PathBuf::from("/home/user");
        let result = resolve_path_input("projects/", &current, false);
        assert_eq!(result, PathBuf::from("/home/user/projects/"));
    }

    #[test]
    fn resolve_double_dot_multiple() {
        let current = PathBuf::from("/a/b/c/d");
        let result = resolve_path_input("../../", &current, false);
        assert_eq!(result, PathBuf::from("/a/b/c/d/../../"));
    }

    #[test]
    fn resolve_single_dot() {
        let current = PathBuf::from("/home/user");
        let result = resolve_path_input(".", &current, false);
        assert_eq!(result, PathBuf::from("/home/user/."));
    }

    // ── history navigation ──────────────────────────────────────

    #[test]
    fn push_history_empty_fs() {
        let fs = make_fs("/root");
        assert_eq!(fs.history.len(), 1); // initialized with current_path
        assert_eq!(fs.history[0], PathBuf::from("/root"));
    }

    #[test]
    fn push_history_same_path_twice_no_dup() {
        let mut fs = make_fs("/home");
        push_history(&mut fs, PathBuf::from("/home"));
        assert_eq!(fs.history.len(), 1);
    }

    #[test]
    fn push_history_alternating_paths() {
        let mut fs = make_fs("/a");
        push_history(&mut fs, PathBuf::from("/b"));
        push_history(&mut fs, PathBuf::from("/a"));
        push_history(&mut fs, PathBuf::from("/b"));
        // Only consecutive dedup, so: ["/a" (init), "/b", "/a", "/b"]
        assert_eq!(fs.history.len(), 4);
    }
}
