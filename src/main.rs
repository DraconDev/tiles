use std::time::Duration;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;


// Terma Imports
use terma::integration::ratatui::TermaBackend;
use terma::input::event::{Event, KeyCode, MouseButton, MouseEventKind, KeyModifiers};

// Ratatui Imports
use ratatui::Terminal;

// App Imports
use crate::app::{App, AppMode, CurrentView, CommandItem, AppEvent};


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
                            AppEvent::Tick => {
                                // Tick is now only for general timing if needed
                            }
                            AppEvent::SystemUpdated(data) => {
                                if let Ok(mut app) = app_bg.lock() {
                                    app.system_state.cpu_usage = data.cpu_usage;
                                    app.system_state.mem_usage = data.mem_usage;
                                    app.system_state.total_mem = data.total_mem;
                                    app.system_state.disks = data.disks;
                                    app.system_state.processes = data.processes;
                                }
                            }
                            AppEvent::Raw(raw) => {
                                // println!("DEBUG: Receiver: Got Raw Event: {:?}", raw);
                                let mut app_guard = app_bg.lock().unwrap();
                                let app_tx = event_tx_bg.clone();
                                handle_event(raw, &mut app_guard, app_tx);
                                
                                // Check if selection changed and if it's an image
                                if let Some(fs) = app_guard.current_file_state() {
                                    if let Some(idx) = fs.selected_index {
                                        if let Some(path) = fs.files.get(idx) {
                                            let _ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                                        }
                            }
                        }
                    }

                            AppEvent::RefreshFiles(idx) => {
                                let (path, show_hidden, filter, session) = {
                                    if let Ok(app) = app_bg.lock() {
                                        if let Some(fs) = app.file_tabs.get(idx) {
                                            (fs.current_path.clone(), fs.show_hidden, fs.search_filter.clone(), fs.remote_session.as_ref().map(|rs| rs.session.clone()))
                                        } else { continue; }
                                    } else { continue; }
                                };
                                let tx = event_tx_bg.clone();
                                tokio::spawn(async move {
                                    let mut temp_state = crate::app::FileState {
                                        current_path: path, remote_session: None, selected_index: None,
                                        table_state: ratatui::widgets::TableState::default(), files: Vec::new(),
                                        metadata: std::collections::HashMap::new(), show_hidden, git_status: std::collections::HashMap::new(),
                                        clipboard: None, search_filter: filter, starred: std::collections::HashSet::new(),
                                        columns: Vec::new(), history: Vec::new(), history_index: 0,
                                        view_height: 0,
                                    };
                                    if let Some(s_mutex) = session {
                                        if let Ok(s) = s_mutex.lock() { crate::modules::files::update_files(&mut temp_state, Some(&s)); }
                                    } else { crate::modules::files::update_files(&mut temp_state, None); }
                                    let _ = tx.send(AppEvent::FilesUpdated(idx, temp_state.files, temp_state.metadata, temp_state.git_status)).await;
                                });
                            }
                            AppEvent::FilesUpdated(idx, files, meta, git) => {
                                if let Ok(mut app) = app_bg.lock() {
                                    if let Some(fs) = app.file_tabs.get_mut(idx) {
                                        fs.files = files; fs.metadata = meta; fs.git_status = git;
                                    }
                                }
                            }
                            AppEvent::CreateFile(filename) => {
                                if let Ok(mut app) = app_bg.lock() {
                                    if let Some(fs) = app.current_file_state() {
                                        let path = fs.current_path.join(filename);
                                        if !path.exists() {
                                            if let Ok(_) = std::fs::File::create(&path) {
                                                // Success
                                            }
                                        }
                                    }
                                    let _ = event_tx_bg.try_send(AppEvent::RefreshFiles(app.tab_index));
                                }
                            }
                            AppEvent::CreateFolder(foldername) => {
                                if let Ok(mut app) = app_bg.lock() {
                                    if let Some(fs) = app.current_file_state() {
                                        let path = fs.current_path.join(&foldername);
                                        let _ = std::fs::write("/home/dracon/debug_tiles.log", format!("Attempting to create folder: {:?} in {:?}\n", foldername, fs.current_path));
                                        if !path.exists() {
                                            match std::fs::create_dir(&path) {
                                                Ok(_) => {
                                                     let _ = std::fs::write("/home/dracon/debug_tiles_success.log", format!("Created: {:?}\n", path));
                                                }
                                                Err(e) => {
                                                     let _ = std::fs::write("/home/dracon/debug_tiles_error.log", format!("Error creating {:?}: {}\n", path, e));
                                                }
                                            }
                                        } else {
                                             let _ = std::fs::write("/home/dracon/debug_tiles_error.log", format!("Path exists: {:?}\n", path));
                                        }
                                    }
                                    let _ = event_tx_bg.try_send(AppEvent::RefreshFiles(app.tab_index));
                                }
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
    if fs.current_path == path { return; }
    fs.history.truncate(fs.history_index + 1);
    fs.history.push(path);
    fs.history_index = fs.history.len() - 1;
}

fn navigate_back(fs: &mut crate::app::FileState) {
    if fs.history_index > 0 {
        fs.history_index -= 1;
        fs.current_path = fs.history[fs.history_index].clone();
        fs.selected_index = Some(0);
        fs.search_filter.clear();
    }
}

fn navigate_forward(fs: &mut crate::app::FileState) {
    if fs.history_index + 1 < fs.history.len() {
        fs.history_index += 1;
        fs.current_path = fs.history[fs.history_index].clone();
        fs.selected_index = Some(0);
        fs.search_filter.clear();
    }
}



fn handle_event(evt: Event, app: &mut App, event_tx: mpsc::Sender<AppEvent>) {
    match evt {
        Event::Mouse(me) => {
            let column = me.column;
            let row = me.row;
            let mut is_double_click = false;
            if let MouseEventKind::Down(button) = me.kind {
                if button == MouseButton::Left {
                    let now = std::time::Instant::now();
                    if let Some((last_time, last_row, last_col)) = app.last_click {
                        if now.duration_since(last_time) < Duration::from_millis(500) && last_row == row && last_col == column {
                            is_double_click = true;
                        }
                    }
                    app.last_click = Some((now, row, column));
                }
            }

            match me.kind {
                MouseEventKind::Down(button) | MouseEventKind::Up(button) if button == MouseButton::Back || button == MouseButton::Forward => {
                    if let Some(fs) = app.current_file_state_mut() {
                        if button == MouseButton::Back { navigate_back(fs); }
                        else { navigate_forward(fs); }
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index));
                    }
                    return;
                }
                MouseEventKind::Down(button) => {
                    if let AppMode::ContextMenu { x, y, item_index } = app.mode {
                        let menu_width = 20;
                        let menu_height = 5;
                        if column >= x && column < x + menu_width && row >= y && row < y + menu_height {
                            let menu_row = row.saturating_sub(y + 1) as usize;
                            if item_index.is_some() {
                                match menu_row {
                                    0 => app.mode = AppMode::Rename,
                                    1 => { if let Some(fs) = app.current_file_state_mut() { if let Some(idx) = item_index { if let Some(path) = fs.files.get(idx).cloned() { if !fs.starred.insert(path.clone()) { fs.starred.remove(&path); } } } } app.mode = AppMode::Normal; },
                                    2 => app.mode = AppMode::Delete,
                                    _ => app.mode = AppMode::Normal,
                                }
                            } else {
                                match menu_row {
                                    0 => { app.mode = AppMode::NewFolder; app.input.clear(); },
                                    1 => { app.mode = AppMode::NewFile; app.input.clear(); },
                                    2 => { let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index)); app.mode = AppMode::Normal; },
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
                                        let new_fs = crate::app::FileState {
                                            current_path: path.clone(), remote_session: fs.remote_session.clone(), 
                                            selected_index: Some(0), table_state: ratatui::widgets::TableState::default(),
                                            files: Vec::new(), metadata: std::collections::HashMap::new(), 
                                            show_hidden: fs.show_hidden, git_status: std::collections::HashMap::new(),
                                            clipboard: None, search_filter: String::new(), starred: fs.starred.clone(),
                                            columns: fs.columns.clone(), history: vec![path], history_index: 0,
                                            view_height: 0,
                                        };
                                        app.file_tabs.push(new_fs);
                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.file_tabs.len() - 1));
                                    }
                                }
                            }
                        }
                        return;
                    }
                    if button == MouseButton::Left {
                        if row == 0 {
                            if column < 11 { app.current_view = CurrentView::Files; }
                            else if column < 22 { app.current_view = CurrentView::System; }
                        } else {
                            let sidebar_width = 16;
                            if column < sidebar_width {
                                app.sidebar_focus = true;
                                let sidebar_row = row.saturating_sub(2) as usize;
                                let num_remotes = app.remote_bookmarks.len().max(1);
                                match sidebar_row {
                                    1..=4 => {
                                        app.sidebar_index = sidebar_row;
                                        if let Some(p) = match app.sidebar_index { 1 => dirs::home_dir(), 2 => dirs::download_dir(), 3 => dirs::document_dir(), 4 => dirs::picture_dir(), _ => None } {
                                            if let Some(fs) = app.current_file_state_mut() {
                                                fs.current_path = p.clone(); fs.selected_index = Some(0); fs.search_filter.clear();
                                                *fs.table_state.offset_mut() = 0;
                                                push_history(fs, p);
                                            }
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index));
                                            app.sidebar_focus = false;
                                        }
                                    },
                                    r if r >= 7 && r < 7 + num_remotes => {
                                        let bookmark_idx = r - 7;
                                        if bookmark_idx < app.remote_bookmarks.len() {
                                            app.sidebar_index = r;
                                            execute_command(crate::app::CommandAction::ConnectToRemote(bookmark_idx), app, event_tx.clone());
                                        }
                                    },
                                    r if r >= 9 + num_remotes => {
                                        let storage_idx = r - (9 + num_remotes);
                                        app.sidebar_index = r;
                                        let path = match storage_idx {
                                            0 => Some(std::path::PathBuf::from("/")),
                                            1 => Some(std::path::PathBuf::from("/run/media")),
                                            _ => None,
                                        };
                                        if let Some(p) = path {
                                            if let Some(fs) = app.current_file_state_mut() {
                                                fs.current_path = p.clone(); fs.selected_index = Some(0); fs.search_filter.clear();
                                                *fs.table_state.offset_mut() = 0;
                                                push_history(fs, p);
                                            }
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index));
                                            app.sidebar_focus = false;
                                        }
                                    }
                                    _ => {}
                                }
                                app.sidebar_focus = false;
                                if app.current_view == CurrentView::Files {
                                    // Row 2 is the content of the Path Bar (breadcrumbs)
                                    if row == 2 {
                                        if let Some(fs) = app.current_file_state_mut() {
                                            if fs.search_filter.is_empty() {
                                                let text_start = sidebar_width + 7; // Sidebar + "Path: " (6) + border
                                                if column >= text_start {
                                                    let mut click_offset = column - text_start;
                                                    let components: Vec<_> = fs.current_path.components().collect();
                                                    let mut current_pos = 0;
                                                    let mut target_path = std::path::PathBuf::new();
                                                    
                                                    for (i, comp) in components.iter().enumerate() {
                                                        let part = match comp {
                                                            std::path::Component::RootDir => "/".to_string(),
                                                            std::path::Component::Normal(s) => s.to_string_lossy().to_string(),
                                                            _ => continue,
                                                        };
                                                        let part_len = part.len() as u16;
                                                        target_path.push(comp);
                                                        
                                                        if click_offset >= current_pos && click_offset < current_pos + part_len {
                                                            // Clicked on this component
                                                            fs.current_path = target_path;
                                                            fs.selected_index = Some(0);
                                                            *fs.table_state.offset_mut() = 0;
                                                            push_history(fs, fs.current_path.clone());
                                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index));
                                                            return;
                                                        }
                                                        
                                                        current_pos += part_len;
                                                        if i < components.len() - 1 && i > 0 {
                                                            current_pos += 1; // for "/"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    let index = fs_mouse_index(row, app);
                                    if let Some(fs) = app.current_file_state_mut() {
                                        if index < fs.files.len() {
                                            fs.selected_index = Some(index);
                                            fs.table_state.select(Some(index));
                                            if let Some(path) = fs.files.get(index).cloned() {
                                            if path.is_dir() && is_double_click {
                                                fs.current_path = path.clone();
                                                fs.selected_index = Some(0);
                                                fs.search_filter.clear();
                                                *fs.table_state.offset_mut() = 0;
                                                push_history(fs, path);
                                                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index));
                                            }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                MouseEventKind::ScrollUp => {
                    match app.current_view {
                        CurrentView::Files => {
                            if let Some(fs) = app.current_file_state_mut() {
                                let new_offset = fs.table_state.offset().saturating_sub(3);
                                *fs.table_state.offset_mut() = new_offset;
                            }
                        }
                        CurrentView::System => {
                            let new_offset = app.system_state.process_list_state.offset().saturating_sub(3);
                            *app.system_state.process_list_state.offset_mut() = new_offset;
                        }
                    }
                }
                MouseEventKind::ScrollDown => {
                    match app.current_view {
                        CurrentView::Files => {
                            if let Some(fs) = app.current_file_state_mut() {
                                let capacity = fs.view_height.saturating_sub(4);
                                let effective_capacity = capacity.saturating_sub(2); // Margin
                                let max_offset = fs.files.len().saturating_sub(effective_capacity);
                                let new_offset = (fs.table_state.offset() + 3).min(max_offset);
                                *fs.table_state.offset_mut() = new_offset;
                            }
                        }
                        CurrentView::System => {
                            let new_offset = app.system_state.process_list_state.offset().saturating_add(3);
                            *app.system_state.process_list_state.offset_mut() = new_offset;
                        }
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
                        KeyCode::Enter => { let path = std::path::PathBuf::from(&app.input); if path.exists() { if let Some(fs) = app.current_file_state_mut() { fs.current_path = path.clone(); fs.selected_index = Some(0); *fs.table_state.offset_mut() = 0; push_history(fs, path); } let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index)); } app.mode = AppMode::Normal; }
                        _ => {}
                    }
                }
                AppMode::NewFile => {
                    match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Enter => { let name = app.input.clone(); let _ = event_tx.try_send(AppEvent::CreateFile(name)); app.mode = AppMode::Normal; }
                        _ => {}
                    }
                }
                AppMode::NewFolder => {
                    match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Enter => { let name = app.input.clone(); let _ = event_tx.try_send(AppEvent::CreateFolder(name)); app.mode = AppMode::Normal; }
                        _ => {}
                    }
                }
                _ => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        match key.code {
                            KeyCode::Char('q') => app.running = false,
                            KeyCode::Char('.') => { app.mode = AppMode::CommandPalette; update_commands(app); }
                            KeyCode::Char('f') => app.current_view = CurrentView::Files,
                            KeyCode::Char('p') => app.current_view = CurrentView::System,
                            _ => {}
                        }
                        return;
                    }
                    match key.code {
                        KeyCode::Down => { app.move_down(); }
                        KeyCode::Up => { app.move_up(); }
                        KeyCode::Left => { if key.modifiers.contains(KeyModifiers::ALT) { if let Some(fs) = app.current_file_state_mut() { navigate_back(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index)); } } else { app.move_left(); } }
                        KeyCode::Right => { if key.modifiers.contains(KeyModifiers::ALT) { if let Some(fs) = app.current_file_state_mut() { navigate_forward(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index)); } } else { app.move_right(); } }
                        KeyCode::Enter => { if let Some(fs) = app.current_file_state_mut() { if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx).cloned() { if path.is_dir() { fs.current_path = path.clone(); fs.selected_index = Some(0); fs.search_filter.clear(); push_history(fs, path); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index)); } } } } }
                        KeyCode::Char('N') => { app.mode = AppMode::NewFolder; app.input.clear(); }
                        KeyCode::Char('n') => { app.mode = AppMode::NewFile; app.input.clear(); }
                        // Nautilus-style search
                        KeyCode::Char(c) if key.modifiers.is_empty() => {
                            if let Some(fs) = app.current_file_state_mut() {
                                fs.search_filter.push(c);
                                fs.selected_index = Some(0);
                                *fs.table_state.offset_mut() = 0;
                                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index));
                            }
                        }
                        KeyCode::Backspace => {
                            if let Some(fs) = app.current_file_state_mut() {
                                if !fs.search_filter.is_empty() {
                                    fs.search_filter.pop();
                                    fs.selected_index = Some(0);
                                    *fs.table_state.offset_mut() = 0;
                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index));
                                } else {
                                    // Go UP one level if search is empty
                                    if let Some(parent) = fs.current_path.parent() {
                                        let p = parent.to_path_buf();
                                        fs.current_path = p.clone();
                                        fs.selected_index = Some(0);
                                        *fs.table_state.offset_mut() = 0;
                                        push_history(fs, p);
                                        let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index));
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
                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index));
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
    let mouse_row_offset = row.saturating_sub(5) as usize;
    if let Some(fs) = app.current_file_state() { fs.table_state.offset() + mouse_row_offset }
    else { 0 }
}

fn update_commands(app: &mut App) {
    let commands = vec![
        CommandItem { label: "Quit".to_string(), action: crate::app::CommandAction::Quit },
        CommandItem { label: "View: Files".to_string(), action: crate::app::CommandAction::SwitchView(CurrentView::Files) },
        CommandItem { label: "View: System".to_string(), action: crate::app::CommandAction::SwitchView(CurrentView::System) },

        CommandItem { label: "Add Remote Host".to_string(), action: crate::app::CommandAction::AddRemote },
    ];
    let mut filtered = commands;
    for bookmark_idx in 0..app.remote_bookmarks.len() {
        let bookmark = &app.remote_bookmarks[bookmark_idx];
        filtered.push(CommandItem { label: format!("Connect to: {}", bookmark.name), action: crate::app::CommandAction::ConnectToRemote(bookmark_idx) });
    }
    app.filtered_commands = filtered.into_iter().filter(|cmd| cmd.label.to_lowercase().contains(&app.input.to_lowercase())).collect();
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
                let addr = format!("{}:{}", bookmark.host, bookmark.port);
                if let Ok(_tcp) = std::net::TcpStream::connect(&addr) {
                     // Simplified SSH logic for restoration
                }
            }
        },
    }
}
