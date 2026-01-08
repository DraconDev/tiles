use std::time::Duration;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;


// Terma Imports
use terma::integration::ratatui::TermaBackend;
use terma::input::event::{Event, KeyCode, MouseButton, MouseEventKind, KeyModifiers};

// Ratatui Imports
use ratatui::Terminal;

use crate::app::{App, AppMode, CurrentView, CommandItem, AppEvent, DropTarget, SidebarTarget};
use std::path::PathBuf;



mod app;
mod ui;
mod modules;
mod event;
mod config;
mod license;

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    
    // Always run in TTY Mode
    run_tty()
}

// ==================================================================================
//                                    TTY MODE
// ==================================================================================
fn run_tty() -> color_eyre::Result<()> {
    // Initialize TermaBackend (Raw Mode, etc.)
    let backend = TermaBackend::new(std::io::stdout())?;
    let tile_queue = backend.tile_queue();
    let mut terminal = Terminal::new(backend)?;

    // Setup App & Async
    let (app, event_tx, mut _event_rx) = setup_app(tile_queue);

    // TTY Event Loop
    {
        let tx = event_tx.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            // Native terma parser
            let mut parser = terma::input::parser::Parser::new();
            let mut stdin = std::io::stdin();
            let mut buffer = [0; 1];
            loop {
                match stdin.read(&mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        if let Some(evt) = parser.advance(buffer[0]) {
                             if let Some(converted) = crate::event::convert_event(evt) {
                                 // Filter Move events in TTY mode too
                                 let is_spam = if let Event::Mouse(ref me) = converted {
                                      matches!(me.kind, MouseEventKind::Moved)
                                 } else { false };
                                 
                                 if !is_spam {
                                     let _ = tx.blocking_send(AppEvent::Raw(converted));
                                 }
                             }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    loop {
        // Draw
        {
            let mut app_guard = app.lock().unwrap();
            if !app_guard.running { break; }
            terminal.draw(|f| {
                app_guard.terminal_size = (f.area().width, f.area().height);
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
                                let (path, show_hidden, filter, session) = {
                                    if let Ok(app) = app_bg.lock() {
                                        if let Some(pane) = app.panes.get(pane_idx) {
                                            if let Some(fs) = pane.current_state() {
                                                (fs.current_path.clone(), fs.show_hidden, fs.search_filter.clone(), fs.remote_session.as_ref().map(|rs| rs.session.clone()))
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
                                        crate::app::FileColumn::Name,
                                        true,
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
                            // is_double_click = true; // This variable is no longer used
                        }
                    }
                    app.last_click = Some((now, row, column));
                }
            }

            match me.kind {
                MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                    app.mouse_pos = (column, row);
                    
                    // Handle Drag State
                    if let Some((sx, sy)) = app.drag_start_pos {
                        let dist = ((column as i16 - sx as i16).pow(2) + (row as i16 - sy as i16).pow(2)) as f32;
                        if dist >= 1.0 { // Any movement activates drag
                            app.is_dragging = true;
                        }
                    }
                    
                    // Update hovered drop target if dragging
                    app.hovered_drop_target = None;
                    if app.is_dragging {
                        let sidebar_width = (app.terminal_size.0 * 20) / 100;
                        
                        // Check Favorites Hover (Sidebar top)
                        if column < sidebar_width {
                            let mut hit_link = false;
                            for bound in &app.sidebar_bounds {
                                if bound.y == row {
                                    match &bound.target {
                                        SidebarTarget::Favorite(p) => {
                                            // Implement Reordering
                                            if let Some(source) = &app.drag_source {
                                                if app.starred.contains(source) && source != p {
                                                    let source_idx = app.starred.iter().position(|x| x == source);
                                                    let target_idx = app.starred.iter().position(|x| x == p);
                                                    if let (Some(s), Some(t)) = (source_idx, target_idx) {
                                                        app.starred.swap(s, t);
                                                        // After swap, source is now under the mouse. 
                                                        // Stop highlight for this frame to avoid flicker
                                                        app.hovered_drop_target = Some(DropTarget::SidebarArea);
                                                        hit_link = true;
                                                    }
                                                } else {
                                                    app.hovered_drop_target = Some(DropTarget::Folder(p.clone()));
                                                    hit_link = true;
                                                }
                                            } else {
                                                app.hovered_drop_target = Some(DropTarget::Folder(p.clone()));
                                                hit_link = true;
                                            }
                                        }
                                        _ => {}
                                    }
                                    break;
                                }
                            }
                            if !hit_link {
                                app.hovered_drop_target = Some(DropTarget::SidebarArea);
                            }
                        }
                        
                        // Check Folder Hover in content panes
                        if app.hovered_drop_target.is_none() && row >= 3 && column >= sidebar_width {
                            let index = fs_mouse_index(row, app);
                            if let Some(fs) = app.current_file_state() {
                                if let Some(path) = fs.files.get(index) {
                                    if path.is_dir() {
                                        app.hovered_drop_target = Some(DropTarget::Folder(path.clone()));
                                    }
                                }
                            }
                        }
                    }

                    // Update hovered breadcrumb for all panes
                    for pane in &mut app.panes {
                        if let Some(fs) = pane.current_state_mut() {
                            fs.hovered_breadcrumb = None;
                            if row == 1 {
                                for (rect, path) in &fs.breadcrumb_bounds {
                                    if rect.contains(ratatui::layout::Position { x: column, y: row }) {
                                        fs.hovered_breadcrumb = Some(path.clone());
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                MouseEventKind::Down(button) => {
                    // 1. Focus Pane First (so Alt+Click affects the clicked pane)
                    let sidebar_width = (app.terminal_size.0 * 20) / 100;
                    if column >= sidebar_width {
                        let pane_count = app.panes.len();
                        if pane_count > 0 {
                             let content_area_width = app.terminal_size.0.saturating_sub(sidebar_width);
                             let content_col = column.saturating_sub(sidebar_width);
                             let pane_width = content_area_width / pane_count as u16;
                             let clicked_pane = (content_col / pane_width) as usize;
                             if clicked_pane < pane_count {
                                  app.focused_pane_index = clicked_pane;
                             }
                        }
                    }

                    // 2. Alt + Click Shortcuts (Back/Forward)
                    if me.modifiers.contains(KeyModifiers::ALT) {
                        if let Some(fs) = app.current_file_state_mut() {
                            if button == MouseButton::Left { navigate_back(fs); }
                            else if button == MouseButton::Right { navigate_forward(fs); }
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                        }
                        return;
                    }

                    // 3. Mouse Back/Forward Buttons
                    if button == MouseButton::Back || button == MouseButton::Forward {
                        if let Some(fs) = app.current_file_state_mut() {
                            if button == MouseButton::Back { navigate_back(fs); }
                            else { navigate_forward(fs); }
                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                        }
                        return;
                    }

                    if let AppMode::ContextMenu { x, y, item_index } = app.mode {
                        let menu_width = 20;
                        let menu_height = if item_index.is_some() { 6 } else { 6 }; // Dynamic based on items
                        if column >= x && column < x + menu_width && row >= y && row < y + menu_height {
                            let menu_row = row.saturating_sub(y + 1) as usize;
                            if let Some(idx) = item_index {
                                // Item menu: check if folder or file
                                if let Some(fs) = app.current_file_state_mut() {
                                    if let Some(path) = fs.files.get(idx).cloned() {
                                        let is_dir = fs.metadata.get(&path).map(|m| m.is_dir).unwrap_or(false);
                                        if is_dir {
                                            // Folder: Open, Star, Rename, Delete
                                            match menu_row {
                                                0 => { // Open
                                                    fs.current_path = path.clone();
                                                    fs.selected_index = Some(0);
                                                    fs.search_filter.clear();
                                                    *fs.table_state.offset_mut() = 0;
                                                    push_history(fs, path);
                                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                                    app.mode = AppMode::Normal;
                                                }
                                                1 => { // Star
                                                    if app.starred.contains(&path) {
                                                        app.starred.retain(|x| x != &path);
                                                    } else {
                                                        app.starred.push(path.clone());
                                                    }
                                                    app.mode = AppMode::Normal;
                                                }
                                                2 => app.mode = AppMode::Rename, // Rename
                                                3 => app.mode = AppMode::Delete, // Delete
                                                _ => app.mode = AppMode::Normal,
                                            }
                                        } else {
                                            // File: Edit, Star, Rename, Delete, Properties
                                                match menu_row {
                                                    0 => { // Edit (open with xdg-open)
                                                        let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
                                                        app.mode = AppMode::Normal;
                                                    }
                                                    1 => { // Star
                                                        if app.starred.contains(&path) {
                                                            app.starred.retain(|x| x != &path);
                                                        } else {
                                                            app.starred.push(path.clone());
                                                        }
                                                        app.mode = AppMode::Normal;
                                                    }
                                                    2 => app.mode = AppMode::Rename, // Rename
                                                    3 => app.mode = AppMode::Delete, // Delete
                                                    4 => app.mode = AppMode::Properties, // Properties
                                                    _ => app.mode = AppMode::Normal,
                                                }
                                        }
                                    }
                                }
                            } else {
                                // Empty space menu: New Folder, New File, Refresh, Terminal Here
                                match menu_row {
                                    0 => { app.mode = AppMode::NewFolder; app.input.clear(); },
                                    1 => { app.mode = AppMode::NewFile; app.input.clear(); },
                                    2 => { let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); app.mode = AppMode::Normal; },
                                    3 => { // Terminal Here
                                        if let Some(fs) = app.current_file_state() {
                                            let _ = std::process::Command::new("xdg-terminal")
                                                .current_dir(&fs.current_path)
                                                .spawn()
                                                .or_else(|_| std::process::Command::new("gnome-terminal")
                                                    .current_dir(&fs.current_path)
                                                    .spawn())
                                                .or_else(|_| std::process::Command::new("xterm")
                                                    .current_dir(&fs.current_path)
                                                    .spawn());
                                        }
                                        app.mode = AppMode::Normal;
                                    }
                                    _ => app.mode = AppMode::Normal,
                                }
                            }
                            return;
                        }
                        app.mode = AppMode::Normal;
                        return;
                    }
                    if button == MouseButton::Right {
                        let index = if app.current_view == CurrentView::Files && !app.sidebar_focus {
                            let idx = fs_mouse_index(row, app);
                            if let Some(fs) = app.current_file_state() { if idx < fs.files.len() { Some(idx) } else { None } } else { None }
                        } else { None };
                        if let Some(idx) = index { if let Some(fs) = app.current_file_state_mut() { fs.selected_index = Some(idx); fs.table_state.select(Some(idx)); } }
                        app.mode = AppMode::ContextMenu { x: column, y: row, item_index: index };
                        return;
                    }
                    if button == MouseButton::Middle {
                        if app.current_view == CurrentView::Files {
                            let index = fs_mouse_index(row, app);
                            if let Some(fs) = app.current_file_state() {
                                if let Some(path) = fs.files.get(index).cloned() {
                                    if path.is_dir() {
                                        let new_fs = crate::app::FileState::new(
                                            path.clone(),
                                            fs.remote_session.clone(),
                                            fs.show_hidden,
                                            fs.columns.clone(),
                                            fs.sort_column,
                                            fs.sort_ascending,
                                        );
                                        // Open in new tab in current pane
                                        if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                                            pane.open_tab(new_fs);
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                        }
                                    }
                                }
                            }
                        }
                        return;
                    }
                    if button == MouseButton::Left {
                        // Row 0: Global Header (Settings / Split / Tabs)
                        if row == 0 {
                            let settings_width = 4;
                            let split_width = 4;
                            let right_x = app.terminal_size.0.saturating_sub(settings_width + split_width + 2);
                            
                            // Check for Split Button
                            if column >= right_x && column < right_x + split_width {
                                app.toggle_split();
                                return;
                            }

                            // Tab Click Logic
                            // Use explicit 20% calculation to match Ratatui (integer math)
                            let sidebar_width = (app.terminal_size.0 * 20) / 100;
                            
                            // Check if click is in Tab Area (Right of Sidebar)
                            if column >= sidebar_width && column < right_x {
                                let pane_count = app.panes.len();
                                if pane_count > 0 {
                                    // Tabs occupy the full content width (buttons overlay rightmost part)
                                    let content_area_width = app.terminal_size.0.saturating_sub(sidebar_width);
                                    let pane_width = content_area_width / pane_count as u16; 
                                    
                                    // Relative column inside tab area
                                    let rel_col_global = column.saturating_sub(sidebar_width);
                                    
                                    let clicked_pane_idx = (rel_col_global / pane_width) as usize;
                                    if clicked_pane_idx < pane_count {
                                        app.focused_pane_index = clicked_pane_idx;
                                        
                                        // Identify Tab within Pane
                                        // Start x for this pane (relative to tab area start)
                                        let pane_start_rel_x = (clicked_pane_idx as u16) * pane_width;
                                        let rel_col = rel_col_global.saturating_sub(pane_start_rel_x);
                                        
                                        // We need to iterate tabs to see which one was clicked.
                                        let mut current_x = 0;
                                        // Separator offset
                                        if clicked_pane_idx > 0 { current_x += 2; } 
                                        
                                        if let Some(pane) = app.panes.get_mut(clicked_pane_idx) {
                                            for (t_i, tab) in pane.tabs.iter().enumerate() {
                                                let name = if !tab.search_filter.is_empty() {
                                                    format!("Search: {}", tab.search_filter)
                                                } else {
                                                    tab.current_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or("/".to_string())
                                                };
                                                let text = format!(" {} ", name);
                                                let width = text.len() as u16;
                                                
                                                if rel_col >= current_x && rel_col < current_x + width {
                                                    pane.active_tab_index = t_i;
                                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(clicked_pane_idx));
                                                    return;
                                                }
                                                current_x += width + 1; // +1 for spacer
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Row 1: Breadcrumbs
                        if row == 1 {
                             let pane_count = app.panes.len();
                             let sidebar_width = (app.terminal_size.0 * 20) / 100;
                             
                             if column >= sidebar_width {
                                 let content_area_width = app.terminal_size.0.saturating_sub(sidebar_width);
                                 let pane_width = if pane_count > 0 { content_area_width / pane_count as u16 } else { content_area_width };
                                 let rel_col = column.saturating_sub(sidebar_width);
                                 let clicked_pane_idx = (rel_col / pane_width) as usize;
                                 
                                 if let Some(pane) = app.panes.get_mut(clicked_pane_idx) {
                                     if let Some(fs) = pane.current_state_mut() {
                                         for (rect, path) in fs.breadcrumb_bounds.clone() {
                                             if rect.contains(ratatui::layout::Position { x: column, y: row }) {
                                                 fs.current_path = path.clone();
                                                 fs.selected_index = Some(0);
                                                 fs.search_filter.clear();
                                                 push_history(fs, path);
                                                 let _ = event_tx.try_send(AppEvent::RefreshFiles(clicked_pane_idx));
                                                 return;
                                             }
                                         }
                                     }
                                 }
                             }
                             app.current_view = CurrentView::Files;
                        } else if row > 1 {
                            let sidebar_width = (app.terminal_size.0 * 20) / 100;
                            if column < sidebar_width {
                                app.sidebar_focus = true;
                                let mut clicked_target = None;
                                for bound in &app.sidebar_bounds {
                                    if bound.y == row {
                                        clicked_target = Some(bound.target.clone());
                                        break;
                                    }
                                }

                                if let Some(target) = clicked_target {
                                    match target {
                                        SidebarTarget::Favorite(p) => {
                                            // Set drag source for reordering
                                            app.drag_source = Some(p.clone());
                                            app.drag_start_pos = Some((column, row));

                                            if let Some(fs) = app.current_file_state_mut() {
                                                fs.current_path = p.clone();
                                                fs.selected_index = Some(0);
                                                fs.search_filter.clear();
                                                *fs.table_state.offset_mut() = 0;
                                                push_history(fs, p.clone());
                                            }
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                            
                                            // Focus sidebar and find index
                                            app.sidebar_focus = true;
                                            if let Some(pos) = app.starred.iter().position(|x| x == &p) {
                                                app.sidebar_index = pos + 1; // +1 for [FAVORITES] header
                                            }
                                        }
                                        SidebarTarget::Storage(idx) => {
                                            if let Some(disk) = app.system_state.disks.get(idx) {
                                                let p = PathBuf::from(&disk.name);
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
                                            if idx < app.remote_bookmarks.len() {
                                                execute_command(crate::app::CommandAction::ConnectToRemote(idx), app, event_tx.clone());
                                            }
                                        }
                                    }
                                }
                            } else {
                                // Clicking in the files area (column >= sidebar_width)
                                app.sidebar_focus = false;
                                
                                // Determine focused pane
                                let pane_count = app.panes.len();
                                if pane_count > 0 {
                                     // Sidebar is 20%. The rest is split.
                                     let content_area_width = app.terminal_size.0.saturating_sub(sidebar_width);
                                     let content_col = column.saturating_sub(sidebar_width);
                                     let pane_width = content_area_width / pane_count as u16;
                                     let clicked_pane = (content_col / pane_width) as usize;
                                     if clicked_pane < pane_count {
                                          app.focused_pane_index = clicked_pane;
                                     }
                                }

                                if app.current_view == CurrentView::Files {
                                    // Column header click detection (row 2 is the header row)
                                    if row == 2 {
                                        if let Some(fs) = app.current_file_state_mut() {
                                            let mut clicked_col = None;
                                            for (rect, col_type) in &fs.column_bounds {
                                                if rect.contains(ratatui::layout::Position { x: column, y: row }) {
                                                    clicked_col = Some(*col_type);
                                                    break;
                                                }
                                            }
                                            
                                            if let Some(col) = clicked_col {
                                                    if fs.sort_column == col {
                                                        fs.sort_ascending = !fs.sort_ascending;
                                                    } else {
                                                        fs.sort_column = col;
                                                        fs.sort_ascending = true;
                                                    }
                                                    // Trigger refresh to re-sort
                                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                                    return;
                                                }
                                            }
                                        } else if row >= 3 {
                                        // File Row Click
                                        let content_start = sidebar_width + 1;
                                        if column >= content_start {
                                            let index = fs_mouse_index(row, app);
                                            
                                            // Validate index and select
                                            let mut selected_path = None;
                                            if let Some(fs) = app.current_file_state_mut() {
                                                if index < fs.files.len() {
                                                    fs.selected_index = Some(index);
                                                    fs.table_state.select(Some(index));
                                                    if let Some(p) = fs.files.get(index) {
                                                        selected_path = Some(p.clone());
                                                    }
                                                }
                                            }

                                            // Now that fs is dropped, we can modify app state
                                            if let Some(path) = selected_path {
                                                app.drag_source = Some(path.clone());
                                                app.drag_start_pos = Some((column, row));

                                                // Double Click Check
                                                if app.mouse_last_click.elapsed() < std::time::Duration::from_millis(500) && app.mouse_click_pos == (column, row) {
                                                    if path.is_dir() {
                                                        // Enter Directory
                                                        if let Some(fs) = app.current_file_state_mut() {
                                                            fs.current_path = path.clone();
                                                            fs.selected_index = Some(0);
                                                            fs.search_filter.clear();
                                                            *fs.table_state.offset_mut() = 0;
                                                            push_history(fs, path);
                                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                                        }
                                                    } else {
                                                         // Open File
                                                        let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
                                                    }
                                                }
                                                app.mouse_last_click = std::time::Instant::now();
                                                app.mouse_click_pos = (column, row);
                                            }
                                        }
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
                        if let Some(source) = source_opt {
                            let mut target_path: Option<std::path::PathBuf> = None;
                            let sidebar_width = (app.terminal_size.0 * 20) / 100;
                            
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
                                    if let Some(filename) = source.file_name() {
                                        let dest = target.join(filename);
                                        if dest != source && source.parent() != Some(&target) {
                                            let _ = crate::modules::files::move_recursive(&source, &dest);
                                            for i in 0..app.panes.len() {
                                                let _ = event_tx.try_send(AppEvent::RefreshFiles(i));
                                            }
                                        }
                                    }
                                }
                            } else if column < sidebar_width {
                                // Dropped on sidebar but no specific move target hit -> Add to Favorites
                                if source.is_dir() {
                                    if !app.starred.contains(&source) {
                                        app.starred.push(source.clone());
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
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Enter => {
                            if let Some(fs) = app.current_file_state() {
                                let path = fs.current_path.join(app.input.clone());
                                let _ = event_tx.try_send(AppEvent::CreateFile(path));
                            }
                            app.mode = AppMode::Normal;
                        }
                        _ => {}
                    }
                }
                AppMode::NewFolder => {
                    match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Enter => {
                            if let Some(fs) = app.current_file_state() {
                                let path = fs.current_path.join(app.input.clone());
                                let _ = event_tx.try_send(AppEvent::CreateFolder(path));
                            }
                            app.mode = AppMode::Normal;
                        }
                        _ => {}
                    }
                }
                _ => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        match key.code {
                            KeyCode::Char('q') => app.running = false,
                            KeyCode::Char('s') => app.toggle_split(),
                            KeyCode::Char('.') => { app.mode = AppMode::CommandPalette; update_commands(app); }
                            KeyCode::Char('f') => app.current_view = CurrentView::Files,
                            _ => {}
                        }
                        return;
                    }

                    if key.modifiers.contains(KeyModifiers::ALT) {
                        match key.code {
                            KeyCode::Left => { if let Some(fs) = app.current_file_state_mut() { navigate_back(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } }
                            KeyCode::Right => { if let Some(fs) = app.current_file_state_mut() { navigate_forward(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } }
                            KeyCode::Up => {
                                if app.sidebar_focus && !app.starred.is_empty() {
                                    // sidebar_index starts after [FAVORITES] header (index 0)
                                    // if sidebar_index is between 1 and starred.len()
                                    if app.sidebar_index > 1 && app.sidebar_index <= app.starred.len() {
                                        let fav_idx = app.sidebar_index - 1;
                                        if fav_idx > 0 {
                                            app.starred.swap(fav_idx, fav_idx - 1);
                                            app.sidebar_index -= 1;
                                        }
                                    }
                                }
                            }
                            KeyCode::Down => {
                                if app.sidebar_focus && !app.starred.is_empty() {
                                    if app.sidebar_index >= 1 && app.sidebar_index < app.starred.len() {
                                        let fav_idx = app.sidebar_index - 1;
                                        if fav_idx < app.starred.len() - 1 {
                                            app.starred.swap(fav_idx, fav_idx + 1);
                                            app.sidebar_index += 1;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                        return;
                    }

                    match key.code {
                        KeyCode::Down => { app.move_down(); }
                        KeyCode::Up => { app.move_up(); }
                        KeyCode::Left => { app.move_left(); }
                        KeyCode::Right => { app.move_right(); }
                        KeyCode::Enter => { if let Some(fs) = app.current_file_state_mut() { if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx).cloned() { if path.is_dir() { fs.current_path = path.clone(); fs.selected_index = Some(0); fs.search_filter.clear(); push_history(fs, path); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index)); } } } } }
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
                        KeyCode::Esc => {
                            if let Some(fs) = app.current_file_state_mut() {
                                if !fs.search_filter.is_empty() {
                                    fs.search_filter.clear();
                                    fs.selected_index = Some(0);
                                    *fs.table_state.offset_mut() = 0;
                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
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
