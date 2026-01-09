use std::time::Duration;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;


// Terma Imports
use terma::integration::ratatui::TermaBackend;
use terma::input::event::{Event, KeyCode, MouseButton, MouseEventKind, KeyModifiers};

// Ratatui Imports
use ratatui::Terminal;

use crate::app::{App, AppMode, CurrentView, CommandItem, AppEvent, DropTarget, SidebarTarget, ContextMenuTarget};
use std::path::PathBuf;



mod app;
mod ui;
mod modules;
mod event;
mod config;
mod license;

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    std::panic::set_hook(Box::new(|panic_info| {
        let msg = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };
        let location = panic_info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_else(|| "unknown location".to_string());
        crate::app::log_debug(&format!("PANIC at {}: {}", location, msg));
    }));
    
    // Always run in TTY Mode
    run_tty()
}

// ==================================================================================
//                                    TTY MODE
// ==================================================================================
fn run_tty() -> color_eyre::Result<()> {
    crate::app::log_debug("run_tty start");
    // Initialize TermaBackend (Raw Mode, etc.)
    let backend = TermaBackend::new(std::io::stdout())?;
    crate::app::log_debug("TermaBackend created");
    let tile_queue = backend.tile_queue();
    let mut terminal = Terminal::new(backend)?;

    // Setup App & Async
    let (app, event_tx, mut _event_rx) = setup_app(tile_queue);
    crate::app::log_debug("App state and event loop initialized");

    // TTY Event Loop
    {
        let tx = event_tx.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            use std::os::fd::AsRawFd;
            
            // Native terma parser
            let mut parser = terma::input::parser::Parser::new();
            let mut stdin = std::io::stdin();
            let fd = stdin.as_raw_fd();
            let mut buffer = [0; 1024];
            
            loop {
                // Poll for input with timeout (20ms) to detect Esc key
                let polled = unsafe { terma::backend::tty::poll_input(std::os::fd::BorrowedFd::borrow_raw(fd), 20) };
                match polled {
                    Ok(true) => {
                        // Data available
                         match stdin.read(&mut buffer) {
                            Ok(0) => break, // EOF
                            Ok(n) => {
                                for i in 0..n {
                                    if let Some(evt) = parser.advance(buffer[i]) {
                                         if let Some(converted) = crate::event::convert_event(evt) {
                                             let is_spam = if let Event::Mouse(ref me) = converted {
                                                  matches!(me.kind, MouseEventKind::Moved)
                                             } else { false };
                                             
                                             if !is_spam {
                                                 let _ = tx.blocking_send(AppEvent::Raw(converted));
                                             }
                                         }
                                    }
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    Ok(false) => {
                        // Timeout - check if parser has pending bare Esc
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

    crate::app::log_debug("Entering main loop");
    loop {
        // Draw
        {
            let mut app_guard = app.lock().unwrap();
            if !app_guard.running { 
                let _ = crate::config::save_state(&app_guard);
                break; 
            }
            app_guard.terminal_size = (terminal.size()?.width, terminal.size()?.height);
            terminal.draw(|f| {
                ui::draw(f, &mut app_guard);
            })?;
        }
        std::thread::sleep(Duration::from_millis(16)); // ~60 FPS cap
    }

    Ok(())
}


// ==================================================================================
//                                  SHARED SETUP
// ==================================================================================
fn setup_app(tile_queue: Arc<Mutex<Vec<terma::compositor::engine::TilePlacement>>>) -> (
    Arc<Mutex<App>>,
    mpsc::Sender<AppEvent>,
    mpsc::Receiver<AppEvent>,

) {
    let app = Arc::new(Mutex::new(App::new(tile_queue)));    
    let (_event_tx, event_rx) = mpsc::channel(1000); 
    // Logic Loop Channel (Input)
    let (logic_tx, mut logic_rx) = mpsc::channel(1000);
    
    
    let app_bg = app.clone();
    let event_tx_bg: mpsc::Sender<AppEvent> = logic_tx.clone();
    
    // WE need to separate:
    // 1. External Events (Window/TTY) -> logic_tx
    // 2. Logic Loop consuming logic_rx
    
    // The callers (run_window) need a Sender. We return logic_tx as 'event_tx'.
    

    
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Tick
            let tick_tx = event_tx_bg.clone();
            tokio::spawn(async move {
                loop {
                    let _ = tick_tx.send(AppEvent::Tick).await;
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            });

            // Init Files
            let _ = event_tx_bg.send(AppEvent::RefreshFiles(0)).await;
            
            // System Updates Thread
            let sys_tx = event_tx_bg.clone();
            std::thread::spawn(move || {
                let mut sys_mod = crate::modules::system::SystemModule::new();
                loop {
                    let data = sys_mod.get_data();
                    let _ = sys_tx.blocking_send(AppEvent::SystemUpdated(data));
                    std::thread::sleep(Duration::from_millis(1000));
                }
            });
            
            // LOGIC LOOP
            loop {
                tokio::select! {
                    Some(evt) = logic_rx.recv() => {
                        match evt {
                            AppEvent::Tick => {}
                            AppEvent::SystemUpdated(data) => {
                                if let Ok(mut app) = app_bg.lock() {
                                    app.system_state.cpu_usage = data.cpu_usage;
                                    app.system_state.mem_usage = data.mem_usage;
                                    app.system_state.total_mem = data.total_mem;
                                    app.system_state.disks = data.disks;
                                }
                            }
                            AppEvent::Raw(raw) => {
                                let mut app_guard = app_bg.lock().unwrap();
                                let app_tx = event_tx_bg.clone();
                                handle_event(raw, &mut app_guard, app_tx);
                            }
                            AppEvent::Delete(path) => {
                                let _ = std::fs::remove_file(&path).or_else(|_| std::fs::remove_dir_all(&path));
                                let app = app_bg.lock().unwrap();
                                let _ = event_tx_bg.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                            }
                            AppEvent::Rename(old, new) => {
                                let _ = std::fs::rename(old, new);
                                let app = app_bg.lock().unwrap();
                                let _ = event_tx_bg.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                            }
                            AppEvent::RefreshFiles(pane_idx) => {
                                let (path, show_hidden, filter, session, sort_column, sort_ascending) = {
                                    if let Ok(app) = app_bg.lock() {
                                        if let Some(pane) = app.panes.get(pane_idx) {
                                            if let Some(fs) = pane.current_state() {
                                                (
                                                    fs.current_path.clone(),
                                                    fs.show_hidden,
                                                    fs.search_filter.clone(),
                                                    fs.remote_session.as_ref().map(|rs| rs.session.clone()),
                                                    fs.sort_column,
                                                    fs.sort_ascending
                                                )
                                            } else { continue; }
                                        } else { continue; }
                                    } else { continue; }
                                };
                                let tx = event_tx_bg.clone();
                                tokio::spawn(async move {
                                    let mut temp_state = crate::app::FileState::new(
                                        path,
                                        None,
                                        show_hidden,
                                        vec![crate::app::FileColumn::Name, crate::app::FileColumn::Size, crate::app::FileColumn::Modified],
                                        sort_column,
                                        sort_ascending,
                                    );
                                    temp_state.search_filter = filter;
                                    if let Some(s_mutex) = session {
                                        if let Ok(s) = s_mutex.lock() { crate::modules::files::update_files(&mut temp_state, Some(&s)); }
                                    } else { crate::modules::files::update_files(&mut temp_state, None); }
                                    let _ = tx.send(AppEvent::FilesUpdated(pane_idx, temp_state.files, temp_state.metadata, temp_state.git_status, temp_state.git_branch)).await;
                                });
                            }
                            AppEvent::FilesUpdated(pane_idx, files, meta, git, branch) => {
                                if let Ok(mut app) = app_bg.lock() {
                                    if let Some(pane) = app.panes.get_mut(pane_idx) {
                                        if let Some(fs) = pane.current_state_mut() {
                                            fs.files = files; fs.metadata = meta; fs.git_status = git; fs.git_branch = branch;
                                        }
                                    }
                                }
                            }
                            AppEvent::CreateFile(path) => {
                                let _ = std::fs::File::create(&path);
                                let app = app_bg.lock().unwrap();
                                let _ = event_tx_bg.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                            }
                            AppEvent::CreateFolder(path) => {
                                let _ = std::fs::create_dir(&path);
                                let app = app_bg.lock().unwrap();
                                let _ = event_tx_bg.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                            }
                        }
                    }
                }
            }
        });
    });

    (app, logic_tx, event_rx)
}

fn push_history(fs: &mut crate::app::FileState, path: std::path::PathBuf) {
    if let Some(last) = fs.history.get(fs.history_index) {
        if last == &path { return; }
    }
    fs.history.truncate(fs.history_index + 1);
    fs.history.push(path);
    fs.history_index = fs.history.len() - 1;
}

fn navigate_back(fs: &mut crate::app::FileState) {
    if fs.history_index > 0 {
        fs.history_index -= 1;
        fs.current_path = fs.history[fs.history_index].clone();
        fs.selected_index = Some(0);
        fs.table_state.select(Some(0));
        *fs.table_state.offset_mut() = 0;
        fs.search_filter.clear();
    }
}

fn navigate_forward(fs: &mut crate::app::FileState) {
    if fs.history_index + 1 < fs.history.len() {
        fs.history_index += 1;
        fs.current_path = fs.history[fs.history_index].clone();
        fs.selected_index = Some(0);
        fs.table_state.select(Some(0));
        *fs.table_state.offset_mut() = 0;
        fs.search_filter.clear();
    }
}



fn handle_event(evt: Event, app: &mut App, event_tx: mpsc::Sender<AppEvent>) {
    match evt {
        Event::Mouse(me) => {
            let column = me.column;
            let row = me.row;
            if let MouseEventKind::Down(button) = me.kind {
                if button == MouseButton::Left {
                    let now = std::time::Instant::now();
                    if let Some((last_time, last_row, last_col)) = app.last_click {
                        if now.duration_since(last_time) < Duration::from_millis(500) && last_row == row && last_col == column {
                        }
                    }
                    app.last_click = Some((now, row, column));
                }
            }

            match me.kind {
                MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                    app.mouse_pos = (column, row);
                    if let Some((sx, sy)) = app.drag_start_pos {
                        let dist = ((column as i16 - sx as i16).pow(2) + (row as i16 - sy as i16).pow(2)) as f32;
                        if dist >= 1.0 { app.is_dragging = true; }
                    }
                    app.hovered_drop_target = None;
                    if app.is_dragging {
                        let sidebar_width = app.sidebar_width();
                        if column < sidebar_width {
                            let mut hit_link = false;
                            for bound in &app.sidebar_bounds {
                                if bound.y == row {
                                    match &bound.target {
                                        SidebarTarget::Favorite(p) => {
                                            if let Some(source) = &app.drag_source {
                                                if app.starred.contains(source) && source != p {
                                                    let source_idx = app.starred.iter().position(|x| x == source);
                                                    let target_idx = app.starred.iter().position(|x| x == p);
                                                    if let (Some(s), Some(t)) = (source_idx, target_idx) {
                                                        app.starred.swap(s, t);
                                                        app.hovered_drop_target = Some(DropTarget::Folder(source.clone()));
                                                        hit_link = true;
                                                    }
                                                } else { app.hovered_drop_target = Some(DropTarget::Folder(p.clone())); hit_link = true; }
                                            } else { app.hovered_drop_target = Some(DropTarget::Folder(p.clone())); hit_link = true; }
                                        }
                                        _ => {}
                                    }
                                    break;
                                }
                            }
                            if !hit_link { app.hovered_drop_target = Some(DropTarget::SidebarArea); }
                        }
                        if app.hovered_drop_target.is_none() && row >= 3 && column >= sidebar_width {
                            let index = fs_mouse_index(row, app);
                            if let Some(fs) = app.current_file_state() {
                                if let Some(path) = fs.files.get(index) { if path.is_dir() { app.hovered_drop_target = Some(DropTarget::Folder(path.clone())); } }
                            }
                        }
                    }
                    for pane in &mut app.panes {
                        if let Some(fs) = pane.current_state_mut() {
                            fs.hovered_breadcrumb = None;
                            if row == 1 {
                                for (rect, path) in &fs.breadcrumb_bounds {
                                    if rect.contains(ratatui::layout::Position { x: column, y: row }) { fs.hovered_breadcrumb = Some(path.clone()); break; }
                                }
                            }
                        }
                    }
                }
                MouseEventKind::Down(button) => {
                    let sidebar_width = app.sidebar_width();
                    let (w, h) = app.terminal_size;

                    // 0. Global Header Handling (Row 0)
                    if row == 0 {
                        // Settings Button (0..10)
                        if column < 10 {
                            app.mode = AppMode::Settings;
                            return;
                        }
                        // Split Button (far right, width 3)
                        if column >= w.saturating_sub(3) {
                            app.toggle_split();
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                            return;
                        }
                    }

                    // 1. Modal / Exclusive Handling

                    if app.mode == AppMode::Settings {
                        if w > 0 && h > 0 {
                            let area_w = (w as f32 * 0.8) as u16;
                            let area_h = (h as f32 * 0.8) as u16;
                            let area_x = (w - area_w) / 2;
                            let area_y = (h - area_h) / 2;
                            if column >= area_x && column < area_x + area_w && row >= area_y && row < area_y + area_h {
                                let inner = ratatui::layout::Rect::new(area_x + 1, area_y + 1, area_w.saturating_sub(2), area_h.saturating_sub(2));
                                if column < inner.x + 15 {
                                    let rel_y = row.saturating_sub(inner.y);
                                    match rel_y {
                                        0 => app.settings_section = crate::app::SettingsSection::Columns,
                                        1 => app.settings_section = crate::app::SettingsSection::Tabs,
                                        2 => app.settings_section = crate::app::SettingsSection::General,
                                        _ => {}
                                    }
                                } else {
                                    match app.settings_section {
                                        crate::app::SettingsSection::Columns => {
                                            if row >= inner.y && row < inner.y + 3 {
                                                let content_x = column.saturating_sub(inner.x + 16);
                                                let tab_width = 12; 
                                                match content_x / tab_width {
                                                    0 => app.settings_target = crate::app::SettingsTarget::AllPanes,
                                                    1 => app.settings_target = crate::app::SettingsTarget::Pane(0),
                                                    2 => if app.panes.len() > 1 { app.settings_target = crate::app::SettingsTarget::Pane(1); },
                                                    _ => {}
                                                }
                                            } else if row >= inner.y + 4 {
                                                let rel_y = row.saturating_sub(inner.y + 4);
                                                match rel_y {
                                                    0 => { app.toggle_column(crate::app::FileColumn::Name); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); },
                                                    1 => { app.toggle_column(crate::app::FileColumn::Size); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); },
                                                    2 => { app.toggle_column(crate::app::FileColumn::Modified); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); },
                                                    3 => { app.toggle_column(crate::app::FileColumn::Created); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); },
                                                    4 => { app.toggle_column(crate::app::FileColumn::Permissions); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); },
                                                    5 => { app.toggle_column(crate::app::FileColumn::Extension); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); },
                                                    _ => {}
                                                }
                                            }
                                        }
                                        crate::app::SettingsSection::General => {
                                            let rel_y = row.saturating_sub(inner.y);
                                            match rel_y {
                                                0 => app.default_show_hidden = !app.default_show_hidden,
                                                1 => app.confirm_delete = !app.confirm_delete,
                                                _ => {}
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                return;
                            } else { app.mode = AppMode::Normal; return; }
                        }
                    }

                    if let AppMode::ContextMenu { x, y, target } = app.mode.clone() {
                        let menu_width = 20;
                        let menu_height = match target {
                            ContextMenuTarget::File(_) => 6, ContextMenuTarget::Folder(_) => 5, ContextMenuTarget::EmptySpace => 5,
                            ContextMenuTarget::SidebarFavorite(_) => 3, ContextMenuTarget::SidebarRemote(_) => 3, ContextMenuTarget::SidebarStorage(_) => 2,
                        };
                        if column >= x && column < x + menu_width && row >= y && row < y + menu_height {
                            let menu_row = row.saturating_sub(y + 1) as usize;
                            match target {
                                ContextMenuTarget::File(idx) => {
                                    if let Some(fs) = app.current_file_state_mut() {
                                        if let Some(path) = fs.files.get(idx).cloned() {
                                            match menu_row {
                                                0 => { let _ = std::process::Command::new("xdg-open").arg(&path).spawn(); app.mode = AppMode::Normal; }
                                                1 => { if app.starred.contains(&path) { app.starred.retain(|x| x != &path); } else { app.starred.push(path.clone()); } app.mode = AppMode::Normal; }
                                                2 => app.mode = AppMode::Rename, 3 => app.mode = AppMode::Delete, 4 => app.mode = AppMode::Properties, _ => app.mode = AppMode::Normal,
                                            }
                                        }
                                    }
                                }
                                ContextMenuTarget::Folder(idx) => {
                                    if let Some(fs) = app.current_file_state_mut() {
                                        if let Some(path) = fs.files.get(idx).cloned() {
                                            match menu_row {
                                                0 => { fs.current_path = path.clone(); fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); app.mode = AppMode::Normal; }
                                                1 => { if app.starred.contains(&path) { app.starred.retain(|x| x != &path); } else { app.starred.push(path.clone()); } app.mode = AppMode::Normal; }
                                                2 => app.mode = AppMode::Rename, 3 => app.mode = AppMode::Delete, _ => app.mode = AppMode::Normal,
                                            }
                                        }
                                    }
                                }
                                ContextMenuTarget::EmptySpace => {
                                    match menu_row {
                                        0 => { app.mode = AppMode::NewFolder; app.input.clear(); }, 1 => { app.mode = AppMode::NewFile; app.input.clear(); },
                                        2 => { let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); app.mode = AppMode::Normal; },
                                        3 => { if let Some(fs) = app.current_file_state() { let terminals = ["kgx", "alacritty", "kitty", "wezterm", "gnome-terminal", "konsole", "xterm", "xdg-terminal"]; for t in terminals { if std::process::Command::new("which").arg(t).stdout(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false) { if std::process::Command::new(t).current_dir(&fs.current_path).spawn().is_ok() { break; } } } } app.mode = AppMode::Normal; }
                                        _ => app.mode = AppMode::Normal,
                                    }
                                }
                                ContextMenuTarget::SidebarFavorite(path) => {
                                    let p_fav = path.clone();
                                    match menu_row {
                                        0 => { app.starred.retain(|x| x != &p_fav); app.mode = AppMode::Normal; }
                                        1 => { if let Some(pane) = app.panes.get_mut(app.focused_pane_index) { let mut new_fs = pane.tabs[pane.active_tab_index].clone(); new_fs.current_path = p_fav.clone(); new_fs.selected_index = Some(0); new_fs.search_filter.clear(); *new_fs.table_state.offset_mut() = 0; new_fs.history = vec![p_fav]; new_fs.history_index = 0; pane.open_tab(new_fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } app.mode = AppMode::Normal; }
                                        _ => app.mode = AppMode::Normal,
                                    }
                                }
                                ContextMenuTarget::SidebarRemote(idx) => {
                                    match menu_row {
                                        0 => { execute_command(crate::app::CommandAction::ConnectToRemote(idx), app, event_tx.clone()); app.mode = AppMode::Normal; }
                                        1 => { app.remote_bookmarks.remove(idx); app.mode = AppMode::Normal; }
                                        _ => app.mode = AppMode::Normal,
                                    }
                                }
                                ContextMenuTarget::SidebarStorage(idx) => { match menu_row { 0 | 1 => app.mode = AppMode::Normal, _ => app.mode = AppMode::Normal, } }
                            }
                            return;
                        } else { app.mode = AppMode::Normal; return; }
                    }

                    if app.mode != AppMode::Normal { return; }

                    if button == MouseButton::Back || button == MouseButton::Forward {
                        if let Some(fs) = app.current_file_state_mut() { if button == MouseButton::Back { navigate_back(fs); } else { navigate_forward(fs); } let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                        return;
                    }

                    if column >= sidebar_width {
                        let pane_count = app.panes.len();
                        if pane_count > 0 {
                             let content_area_width = app.terminal_size.0.saturating_sub(sidebar_width);
                             let content_col = column.saturating_sub(sidebar_width);
                             let pane_width = content_area_width / pane_count as u16;
                             let clicked_pane = (content_col / pane_width) as usize;
                             if clicked_pane < pane_count { app.focused_pane_index = clicked_pane; }
                        }
                    }

                    if app.current_view == CurrentView::Files {
                        let mut clicked_pane_index = None;
                        let mut clicked_col = None;
                        for (i, pane) in app.panes.iter().enumerate() {
                            if let Some(fs) = pane.current_state() {
                                for (rect, col_type) in &fs.column_bounds {
                                    if rect.contains(ratatui::layout::Position { x: column, y: row }) && row == rect.y { clicked_pane_index = Some(i); clicked_col = Some(*col_type); break; }
                                }
                            }
                            if clicked_pane_index.is_some() { break; }
                        }
                        if let (Some(pane_idx), Some(col)) = (clicked_pane_index, clicked_col) {
                            app.focused_pane_index = pane_idx;
                            if let Some(pane) = app.panes.get_mut(pane_idx) {
                                if let Some(fs) = pane.current_state_mut() {
                                    if fs.sort_column == col { fs.sort_ascending = !fs.sort_ascending; }
                                    else { fs.sort_column = col; fs.sort_ascending = true; }
                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(pane_idx)); return;
                                }
                            }
                        }

                        if row == 1 {
                             let pane_count = app.panes.len();
                             if column >= sidebar_width {
                                 let content_area_width = app.terminal_size.0.saturating_sub(sidebar_width);
                                 let pane_width = if pane_count > 0 { content_area_width / pane_count as u16 } else { content_area_width };
                                 let rel_col = column.saturating_sub(sidebar_width);
                                 let clicked_pane_idx = (rel_col / pane_width) as usize;
                                 if let Some(pane) = app.panes.get_mut(clicked_pane_idx) {
                                     if let Some(fs) = pane.current_state_mut() {
                                         for (rect, path) in fs.breadcrumb_bounds.clone() {
                                             if rect.contains(ratatui::layout::Position { x: column, y: row }) {
                                                 fs.current_path = path.clone(); fs.selected_index = Some(0); fs.search_filter.clear(); push_history(fs, path);
                                                 let _ = event_tx.try_send(AppEvent::RefreshFiles(clicked_pane_idx)); return;
                                             }
                                         }
                                     }
                                 }
                             }
                        } else if row > 1 {
                            if column < sidebar_width {
                                app.sidebar_focus = true;
                                 let mut clicked_bound = None;
                                 for bound in &app.sidebar_bounds { if bound.y == row { clicked_bound = Some(bound.clone()); break; } }
                                 if let Some(bound) = clicked_bound {
                                     app.sidebar_index = bound.index;
                                     match bound.target {
                                         SidebarTarget::Favorite(p) => {
                                             if let Some(fs) = app.current_file_state_mut() { fs.current_path = p.clone(); fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, p); }
                                             let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); app.sidebar_focus = false;
                                         }
                                         SidebarTarget::Storage(idx) => {
                                             if let Some(disk) = app.system_state.disks.get(idx) {
                                                 let p = std::path::PathBuf::from(&disk.name);
                                                 if let Some(fs) = app.current_file_state_mut() { fs.current_path = p.clone(); fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, p); }
                                                 let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); app.sidebar_focus = false;
                                             }
                                         }
                                         _ => {}
                                     }
                                 }
                            } else if row >= 3 {
                                let content_start = sidebar_width + 1;
                                if column >= content_start {
                                    let index = fs_mouse_index(row, app);
                                    let mut selected_path = None;
                                    if let Some(fs) = app.current_file_state_mut() {
                                        if index < fs.files.len() {
                                            fs.selected_index = Some(index); fs.table_state.select(Some(index));
                                            if let Some(p) = fs.files.get(index) { selected_path = Some(p.clone()); }
                                        }
                                    }
                                    if let Some(path) = selected_path {
                                        app.drag_source = Some(path.clone()); app.drag_start_pos = Some((column, row));
                                        if app.mouse_last_click.elapsed() < std::time::Duration::from_millis(500) && app.mouse_click_pos == (column, row) {
                                            if path.is_dir() {
                                                if let Some(fs) = app.current_file_state_mut() {
                                                    fs.current_path = path.clone(); fs.selected_index = Some(0); fs.search_filter.clear(); *fs.table_state.offset_mut() = 0; push_history(fs, path);
                                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                                }
                                            } else { let _ = std::process::Command::new("xdg-open").arg(&path).spawn(); }
                                        }
                                        app.mouse_last_click = std::time::Instant::now(); app.mouse_click_pos = (column, row);
                                    }
                                }
                            }
                        }
                    }
                }
                MouseEventKind::Up(_) => {
                    let was_dragging = app.is_dragging;
                    app.is_dragging = false;
                    app.drag_start_pos = None;
                    let source_opt = app.drag_source.clone();
                    app.drag_source = None;

                    if was_dragging {
                        let mut paths_to_act = Vec::new();

                        // Identify which files are being dragged
                        if let Some(fs) = app.current_file_state() {
                            if !fs.multi_select.is_empty() {
                                for &idx in &fs.multi_select {
                                    if let Some(p) = fs.files.get(idx) {
                                        paths_to_act.push(p.clone());
                                    }
                                }
                            }
                        }
                        
                        // If no multi-selection, use the single dragged item
                        if paths_to_act.is_empty() {
                            if let Some(source) = source_opt {
                                paths_to_act.push(source);
                            }
                        }

                        if !paths_to_act.is_empty() {
                            let mut target_path: Option<std::path::PathBuf> = None;
                            let sidebar_width = app.sidebar_width();
                            
                            // Check drop on Breadcrumb (Row 1)
                            if row == 1 && column >= sidebar_width {
                                if let Some(fs) = app.current_file_state() {
                                    for (rect, path) in &fs.breadcrumb_bounds {
                                        if rect.contains(ratatui::layout::Position { x: column, y: row }) {
                                            target_path = Some(path.clone());
                                            break;
                                        }
                                    }
                                }
                            }
                            
                            // Check drop on Sidebar
                            if target_path.is_none() && column < sidebar_width {
                                for bound in &app.sidebar_bounds {
                                    if bound.y == row {
                                        match &bound.target {
                                            SidebarTarget::Favorite(p) => target_path = Some(p.clone()),
                                            SidebarTarget::Storage(idx) => {
                                                if let Some(disk) = app.system_state.disks.get(*idx) {
                                                    target_path = Some(std::path::PathBuf::from(&disk.name));
                                                }
                                            }
                                            _ => {}
                                        }
                                        break;
                                    }
                                }
                            }
                            
                            // Check drop on Folder in file list
                            if target_path.is_none() && row >= 3 && column >= sidebar_width {
                                let index = fs_mouse_index(row, app);
                                if let Some(fs) = app.current_file_state() {
                                    if let Some(path) = fs.files.get(index) {
                                        if path.is_dir() {
                                            target_path = Some(path.clone());
                                        }
                                    }
                                }
                            }
                            
                            if let Some(target) = target_path {
                                if target.is_dir() {
                                    for source in paths_to_act {
                                        if let Some(filename) = source.file_name() {
                                            let dest = target.join(filename);
                                            if dest != source && source.parent() != Some(&target) {
                                                // TODO: Check if SHIFT is held for Copy instead of Move
                                                // Since we don't have access to modifiers in MouseUp event easily here 
                                                // (MouseEvent doesn't carry them in all backends, but terma's might)
                                                // Let's assume Move for now as per current logic.
                                                let _ = crate::modules::files::move_recursive(&source, &dest);
                                            }
                                        }
                                    }
                                    for i in 0..app.panes.len() {
                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(i));
                                    }
                                }
                            } else if column < sidebar_width {
                                // Dropped on sidebar but no specific move target hit -> Add to Favorites
                                for source in paths_to_act {
                                    if source.is_dir() {
                                        if !app.starred.contains(&source) {
                                            app.starred.push(source.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    app.is_dragging = false;
                    app.drag_source = None;
                    app.drag_start_pos = None;
                }
                MouseEventKind::ScrollUp => {
                    if let Some(fs) = app.current_file_state_mut() {
                        let new_offset = fs.table_state.offset().saturating_sub(3);
                        *fs.table_state.offset_mut() = new_offset;
                    }
                }
                MouseEventKind::ScrollDown => {
                    if let Some(fs) = app.current_file_state_mut() {
                        let capacity = fs.view_height.saturating_sub(4);
                        let effective_capacity = capacity.saturating_sub(2); // Margin
                        let max_offset = fs.files.len().saturating_sub(effective_capacity);
                        let new_offset = (fs.table_state.offset() + 3).min(max_offset);
                        *fs.table_state.offset_mut() = new_offset;
                    }
                }
                _ => {}
            }
        }
        Event::Key(key) => {
            crate::app::log_debug(&format!("DEBUG KEY: code={:?} modifiers={:?}", key.code, key.modifiers));
            crate::app::log_debug(&format!("Key Event: {:?} Modifiers: {:?}", key.code, key.modifiers));
            match app.mode {
                AppMode::CommandPalette => {
                    match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Char(c) => { app.input.push(c); update_commands(app); }
                        KeyCode::Backspace => { app.input.pop(); update_commands(app); }
                        KeyCode::Enter => { if let Some(cmd) = app.filtered_commands.get(app.command_index).cloned() { execute_command(cmd.action, app, event_tx.clone()); } app.mode = AppMode::Normal; app.input.clear(); }
                        _ => {}
                    }
                }
                AppMode::AddRemote => {
                     if key.code == KeyCode::Esc {
                         app.mode = AppMode::Normal;
                     }
                }
                AppMode::Location => {

                    match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Enter => { let path = std::path::PathBuf::from(&app.input); if path.exists() { if let Some(fs) = app.current_file_state_mut() { fs.current_path = path.clone(); fs.selected_index = Some(0); *fs.table_state.offset_mut() = 0; push_history(fs, path); } let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } app.mode = AppMode::Normal; }
                        _ => {}
                    }
                }
                AppMode::NewFile => {
                    match key.code {
                        KeyCode::Esc => { crate::app::log_debug("Esc in NewFile"); app.mode = AppMode::Normal; app.input.clear(); }
                        KeyCode::Char(c) => {
                            // Filter out potential escape sequence garbage (e.g. from mouse events)
                            if !c.is_control() && c != '\x1b' && c != '[' {
                                app.input.push(c);
                            }
                        }
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Enter => {
                            if let Some(fs) = app.current_file_state() {
                                let name = app.input.trim();
                                if !name.is_empty() {
                                    let path = fs.current_path.join(name);
                                    let _ = event_tx.try_send(AppEvent::CreateFile(path));
                                }
                            }
                            app.mode = AppMode::Normal;
                        }
                        _ => {}
                    }
                }
                AppMode::NewFolder => {
                    match key.code {
                        KeyCode::Esc => { crate::app::log_debug("Esc in NewFolder"); app.mode = AppMode::Normal; app.input.clear(); }
                        KeyCode::Char(c) => {
                            if !c.is_control() && c != '\x1b' && c != '[' {
                                app.input.push(c);
                            }
                        }
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Enter => {
                            if let Some(fs) = app.current_file_state() {
                                let name = app.input.trim();
                                if !name.is_empty() {
                                    let path = fs.current_path.join(name);
                                    let _ = event_tx.try_send(AppEvent::CreateFolder(path));
                                }
                            }
                            app.mode = AppMode::Normal;
                        }
                        _ => {}
                    }
                }
                AppMode::Settings => {
                    match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Char('0') => app.settings_target = crate::app::SettingsTarget::AllPanes,
                        KeyCode::Char('1') => app.settings_target = crate::app::SettingsTarget::Pane(0),
                        KeyCode::Char('2') => {
                            if app.panes.len() > 1 {
                                app.settings_target = crate::app::SettingsTarget::Pane(1);
                            }
                        }
                        KeyCode::Left | KeyCode::BackTab => {
                            app.settings_section = match app.settings_section {
                                crate::app::SettingsSection::Columns => crate::app::SettingsSection::General,
                                crate::app::SettingsSection::Tabs => crate::app::SettingsSection::Columns,
                                crate::app::SettingsSection::General => crate::app::SettingsSection::Tabs,
                            };
                        }
                        KeyCode::Right | KeyCode::Tab => {
                            app.settings_section = match app.settings_section {
                                crate::app::SettingsSection::Columns => crate::app::SettingsSection::Tabs,
                                crate::app::SettingsSection::Tabs => crate::app::SettingsSection::General,
                                crate::app::SettingsSection::General => crate::app::SettingsSection::Columns,
                            };
                        }
                        KeyCode::Char('n') => { app.toggle_column(crate::app::FileColumn::Name); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                        KeyCode::Char('s') => { app.toggle_column(crate::app::FileColumn::Size); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                        KeyCode::Char('m') => { app.toggle_column(crate::app::FileColumn::Modified); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                        KeyCode::Char('c') => { app.toggle_column(crate::app::FileColumn::Created); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                        KeyCode::Char('p') => { app.toggle_column(crate::app::FileColumn::Permissions); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                        KeyCode::Char('e') => { app.toggle_column(crate::app::FileColumn::Extension); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); }
                        KeyCode::Char('h') if app.settings_section == crate::app::SettingsSection::General => {
                            app.default_show_hidden = !app.default_show_hidden;
                        }
                        KeyCode::Char('d') if app.settings_section == crate::app::SettingsSection::General => {
                            app.confirm_delete = !app.confirm_delete;
                        }
                        _ => {}
                    }
                }
                AppMode::ContextMenu { .. } => {
                    if key.code == KeyCode::Esc {
                        app.mode = AppMode::Normal;
                    }
                }
                _ => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        match key.code {
                            KeyCode::Char('q') => {
                                app.running = false;
                            }
                            KeyCode::Char('s') => {
                                app.toggle_split();
                                let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                                let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                            }
                            KeyCode::Char('h') => {
                                let pane_idx = app.toggle_hidden();
                                let _ = event_tx.try_send(AppEvent::RefreshFiles(pane_idx));
                            }
                            KeyCode::Char('t') | KeyCode::Char('.') => {
                                if let Some(fs) = app.current_file_state() {
                                    let mut terminals = Vec::new();
                                    if let Ok(env_t) = std::env::var("TERMINAL") {
                                        terminals.push(env_t);
                                    }
                                    terminals.extend(vec![
                                        "kgx".to_string(),
                                        "gnome-terminal".to_string(),
                                        "konsole".to_string(),
                                        "xdg-terminal-exec".to_string(),
                                        "x-terminal-emulator".to_string(),
                                        "alacritty".to_string(),
                                        "kitty".to_string(),
                                        "xfce4-terminal".to_string(),
                                        "xterm".to_string(),
                                        "xdg-terminal".to_string(),
                                    ]);

                                    let mut spawned = false;
                                    for t in terminals {
                                        let exists = std::process::Command::new("which")
                                            .arg(&t)
                                            .stdout(std::process::Stdio::null())
                                            .stderr(std::process::Stdio::null())
                                            .status()
                                            .map(|s| s.success())
                                            .unwrap_or(false);
                                            
                                        if exists {
                                            crate::app::log_debug(&format!("Trying to spawn terminal: {}", t));
                                            let mut cmd = std::process::Command::new(&t);
                                            cmd.current_dir(&fs.current_path);
                                            
                                            match t.as_str() {
                                                "gnome-terminal" => { cmd.arg("--window"); }
                                                "konsole" => { cmd.arg("--new-window"); }
                                                _ => {}
                                            }

                                            if let Ok(_) = cmd.spawn() {
                                                crate::app::log_debug(&format!("Spawned {} successfully", t));
                                                spawned = true;
                                                break;
                                            }
                                        }
                                    }
                                    
                                    if !spawned {
                                        crate::app::log_debug("No suitable terminal emulator found in PATH.");
                                    }
                                }
                            }
                            KeyCode::Char(' ') => { app.input.clear(); app.mode = AppMode::CommandPalette; update_commands(app); }
                            _ => {}
                        }
                        return;
                    }

                    if key.modifiers.contains(KeyModifiers::ALT) {
                        let mut handled_reorder = false;
                        match key.code {
                            KeyCode::Left => { if let Some(fs) = app.current_file_state_mut() { navigate_back(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } }
                            KeyCode::Right => { if let Some(fs) = app.current_file_state_mut() { navigate_forward(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } }
                            KeyCode::Up => {
                                if app.sidebar_focus {
                                    // REORDER FAVORITES
                                    if !app.starred.is_empty() {
                                        // Bound logic: finding which favorite corresponds to sidebar_index
                                        if let Some(bound) = app.sidebar_bounds.iter().find(|b| b.index == app.sidebar_index) {
                                            if let SidebarTarget::Favorite(ref p) = bound.target {
                                                if let Some(fav_idx) = app.starred.iter().position(|x| x == p) {
                                                    if fav_idx > 0 {
                                                        app.starred.swap(fav_idx, fav_idx - 1);
                                                        app.sidebar_index -= 1;
                                                        handled_reorder = true;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            KeyCode::Down => {
                                if app.sidebar_focus {
                                    // REORDER FAVORITES
                                    if !app.starred.is_empty() {
                                        if let Some(bound) = app.sidebar_bounds.iter().find(|b| b.index == app.sidebar_index) {
                                            if let SidebarTarget::Favorite(ref p) = bound.target {
                                                if let Some(fav_idx) = app.starred.iter().position(|x| x == p) {
                                                    if fav_idx < app.starred.len() - 1 {
                                                        app.starred.swap(fav_idx, fav_idx + 1);
                                                        app.sidebar_index += 1;
                                                        handled_reorder = true;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                        if app.sidebar_focus && !handled_reorder {
                            let sidebar_items_count = app.sidebar_bounds.len();
                            match key.code {
                                KeyCode::Down => {
                                    if app.sidebar_index < sidebar_items_count.saturating_sub(1) {
                                        app.sidebar_index += 1;
                                    }
                                }
                                KeyCode::Up => {
                                    if app.sidebar_index > 0 {
                                        app.sidebar_index -= 1;
                                    }
                                }
                                _ => {}
                            }
                        }
                        return;
                    }

                    match key.code {
                        KeyCode::Esc => {
                            if let Some(fs) = app.current_file_state_mut() {
                                // Clear multi-selection
                                fs.multi_select.clear();
                                fs.selection_anchor = None;
                                
                                // Clear search
                                if !fs.search_filter.is_empty() {
                                    fs.search_filter.clear();
                                    fs.selected_index = Some(0);
                                    *fs.table_state.offset_mut() = 0;
                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                }
                            }
                        }
                                                KeyCode::Down => { app.move_down(key.modifiers.contains(KeyModifiers::SHIFT)); }
                                                KeyCode::Up => { app.move_up(key.modifiers.contains(KeyModifiers::SHIFT)); }
                                                KeyCode::Left => {
                                                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                                                        crate::app::log_debug("Triggering copy_to_other_pane (Left)");
                                                        app.copy_to_other_pane();
                                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                                                    } else if key.modifiers.contains(KeyModifiers::CONTROL) {
                                                        crate::app::log_debug("Triggering move_to_other_pane (Left)");
                                                        app.move_to_other_pane();
                                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                                                    } else {
                                                        app.move_left();
                                                    }
                                                }
                                                KeyCode::Right => {
                                                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                                                        crate::app::log_debug("Triggering copy_to_other_pane (Right)");
                                                        app.copy_to_other_pane();
                                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                                                    } else if key.modifiers.contains(KeyModifiers::CONTROL) {
                                                        crate::app::log_debug("Triggering move_to_other_pane (Right)");
                                                        app.move_to_other_pane();
                                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                                                    } else {
                                                        app.move_right();
                                                    }
                                                }
                                                KeyCode::Enter => {
                                                    if app.sidebar_focus {
                                                        if let Some(bound) = app.sidebar_bounds.iter().find(|b| b.index == app.sidebar_index).cloned() {
                                                            match &bound.target {
                                                                SidebarTarget::Favorite(p) => {
                                                                    if let Some(fs) = app.current_file_state_mut() {
                                                                        fs.current_path = p.clone();
                                                                        fs.selected_index = Some(0);
                                                                        fs.search_filter.clear();
                                                                        *fs.table_state.offset_mut() = 0;
                                                                        push_history(fs, p.clone());
                                                                    }
                                                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                                                    app.sidebar_focus = false;
                                                                }
                                                                SidebarTarget::Storage(idx) => {
                                                                    if let Some(disk) = app.system_state.disks.get(*idx) {
                                                                        let p = std::path::PathBuf::from(&disk.name);
                                                                        if let Some(fs) = app.current_file_state_mut() {
                                                                            fs.current_path = p.clone();
                                                                            fs.selected_index = Some(0);
                                                                            fs.search_filter.clear();
                                                                            *fs.table_state.offset_mut() = 0;
                                                                            push_history(fs, p);
                                                                        }
                                                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                                                        app.sidebar_focus = false;
                                                                    }
                                                                }
                                                                SidebarTarget::Remote(idx) => {
                                                                    execute_command(crate::app::CommandAction::ConnectToRemote(*idx), app, event_tx.clone());
                                                                }
                                                                _ => {}
                                                            }
                                                        }
                                                        return;
                                                    }
                                                    if let Some(fs) = app.current_file_state_mut() { if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx).cloned() { if path.is_dir() { fs.current_path = path.clone(); fs.selected_index = Some(0); fs.search_filter.clear(); push_history(fs, path); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } } } 
                                                }
                                                KeyCode::Char('N') => { app.mode = AppMode::NewFolder; app.input.clear(); }
                                                KeyCode::Char('n') => { app.mode = AppMode::NewFile; app.input.clear(); }
                                                KeyCode::Char(' ') => {
                                                    if let Some(fs) = app.current_file_state() {
                                                        if let Some(idx) = fs.selected_index {
                                                            if let Some(path) = fs.files.get(idx).cloned() {
                                                                if app.starred.contains(&path) {
                                                                    app.starred.retain(|x| x != &path);
                                                                } else {
                                                                    app.starred.push(path.clone());
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                        // Nautilus-style search
                        KeyCode::Char(c) if key.modifiers.is_empty() => {
                            if let Some(fs) = app.current_file_state_mut() {
                                fs.search_filter.push(c);
                                fs.selected_index = Some(0);
                                *fs.table_state.offset_mut() = 0;
                                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                            }
                        }
                        KeyCode::Backspace => {
                            if let Some(fs) = app.current_file_state_mut() {
                                if !fs.search_filter.is_empty() {
                                    fs.search_filter.pop();
                                    fs.selected_index = Some(0);
                                    *fs.table_state.offset_mut() = 0;
                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                } else {
                                    // Go UP one level if search is empty
                                    if let Some(parent) = fs.current_path.parent() {
                                        let p = parent.to_path_buf();
                                        fs.current_path = p.clone();
                                        fs.selected_index = Some(0);
                                        *fs.table_state.offset_mut() = 0;
                                        push_history(fs, p);
                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
}

fn fs_mouse_index(row: u16, app: &App) -> usize {
    let mouse_row_offset = row.saturating_sub(3) as usize;
    if let Some(fs) = app.current_file_state() { fs.table_state.offset() + mouse_row_offset }
    else { 0 }
}

fn update_commands(app: &mut App) {
    let commands = vec![
        CommandItem { key: "quit".to_string(), desc: "Quit".to_string(), action: crate::app::CommandAction::Quit },
        CommandItem { key: "remote".to_string(), desc: "Add Remote Host".to_string(), action: crate::app::CommandAction::AddRemote },
    ];
    let mut filtered = commands;
    for bookmark_idx in 0..app.remote_bookmarks.len() {
        let bookmark = &app.remote_bookmarks[bookmark_idx];
        filtered.push(CommandItem { key: format!("connect_{}", bookmark_idx), desc: format!("Connect to: {}", bookmark.name), action: crate::app::CommandAction::ConnectToRemote(bookmark_idx) });
    }
    app.filtered_commands = filtered.into_iter().filter(|cmd| cmd.desc.to_lowercase().contains(&app.input.to_lowercase())).collect();
    app.command_index = app.command_index.min(app.filtered_commands.len().saturating_sub(1));
}

fn execute_command(action: crate::app::CommandAction, app: &mut App, _event_tx: mpsc::Sender<AppEvent>) {
    match action {
        crate::app::CommandAction::Quit => { app.running = false; },
        crate::app::CommandAction::ToggleZoom => app.toggle_zoom(),
        crate::app::CommandAction::SwitchView(view) => app.current_view = view,
        crate::app::CommandAction::AddRemote => { app.mode = AppMode::AddRemote; app.input.clear(); },
        crate::app::CommandAction::ConnectToRemote(idx) => {
            if let Some(bookmark) = app.remote_bookmarks.get(idx).cloned() {
                let _addr = format!("{}:{}", bookmark.host, bookmark.port);
            }
        },
        crate::app::CommandAction::CommandPalette => { app.mode = AppMode::CommandPalette; },
    }
}
