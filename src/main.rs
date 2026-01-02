use std::time::Duration;
use std::io::IsTerminal;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;


// Terma Imports
use terma::integration::window::TermaWindow;
use terma::integration::ratatui::{TermaBackend, RatatuiCompositorBackend};
use terma::input::event::{Event, KeyCode, MouseButton, MouseEventKind, KeyModifiers};

use terma::visuals::loader::ImageLoader;
use terma::visuals::shapes::{ShapeGenerator, Color as ShapeColor};

// Ratatui Imports
use ratatui::Terminal;

// App Imports
use crate::app::{App, AppMode, CurrentView, CommandItem, AppEvent, UiCommand};
use crate::modules::docker::DockerModule;

mod app;
mod ui;
mod modules;
mod event;
mod config;
mod license;

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    
    // Check if we should run in TTY mode (SSH / Integrated Terminal)
    // or Window mode (Desktop App).
    let run_in_tty = std::io::stdout().is_terminal() && std::env::var("TILES_FORCE_WINDOW").is_err();

    if run_in_tty {
        // TTY MODE: Inside VS Code or SSH
        run_tty()
    } else {
        // WINDOW MODE: Standalone Window
        run_window()
    }
}

// ==================================================================================
//                                  WINDOW MODE
// ==================================================================================
fn run_window() -> color_eyre::Result<()> {
    // Load Font
    let font_data = include_bytes!("../../terma/assets/font.ttf");
    let mut window = TermaWindow::new(font_data, 24.0)
        .map_err(|e| color_eyre::eyre::eyre!("{}", e))?;
    
    // Generate Assets
    let sidebar_bg = ShapeGenerator::gradient_vertical(
        300, 800, 
        ShapeColor::new(10, 10, 15, 255),
        ShapeColor::new(25, 25, 40, 255)
    );
    window.add_image_asset(2001, sidebar_bg, 300, 800);
    // Add Placement for Sidebar
    window.add_tile_asset(2001, vec![], 300, 800); // Hack: using add_tile_asset to trigger placement if it supports it? 
    // Wait, TermaWindow logic distinguishes add_image_asset (register) vs placements.
    // I need to add a placement. TermaWindow exposes tile_queue.
    if let Ok(mut queue) = window.tile_queue().lock() {
        use terma::compositor::engine::TilePlacement;
        queue.push(TilePlacement {
             asset_id: 2001,
             x: 0,
             y: 0,
             z_index: 0, // Behind TUI
             cols: None, // Use native size
             rows: None,
             is_image: true,
             placement_id: None,
        });
        // Header
         queue.push(TilePlacement {
             asset_id: 3001,
             x: 0,
             y: 0,
             z_index: 1, 
             cols: Some(150), // Stretch width
             rows: None,
             is_image: true,
             placement_id: None,
        });
    }

    let header_bg = ShapeGenerator::gradient_horizontal(
        100, 16,
        ShapeColor::new(0, 255, 200, 50),
        ShapeColor::new(0, 0, 0, 0)
    );
    window.add_image_asset(3001, header_bg, 100, 16);

    let tile_queue = window.tile_queue();
    
    // Setup App & Async
    let (app, event_tx, mut _event_rx, _ui_tx, mut ui_rx, _docker) = setup_app(tile_queue);

    // Main Window Loop
    window.run(move |compositor, event| {
        if let Some(evt) = event {
            // Optimization: Ignore MouseMoved events to prevent channel flooding
            let is_spam = if let Event::Mouse(ref me) = evt {
                matches!(me.kind, MouseEventKind::Moved)
            } else { false };

            if !is_spam {
                // Debug Print
                if let Event::Mouse(_) = evt {
                    // println!("DEBUG: Sending Mouse Event: {:?}", evt);
                }
                if let Err(_) = event_tx.try_send(AppEvent::Raw(evt)) {
                     println!("DEBUG: Channel Full! Dropping event.");
                }
            }
        }

        // Process UI Commands
        while let Ok(cmd) = ui_rx.try_recv() {
            match cmd {
                UiCommand::RegisterImage(id, data, w, h) => {
                    compositor.add_image_asset(id, data, w, h);
                    let _ = event_tx.blocking_send(AppEvent::ImageReady(id, Vec::new(), 0, 0));
                }
            }
        }

        // Render
        let mut app_guard = app.lock().unwrap();
        // Window Mode uses RatatuiCompositorBackend wrapper
        let backend = RatatuiCompositorBackend { compositor };
        let mut terminal = Terminal::new(backend).unwrap();
        
        let _ = terminal.draw(|f| {
            ui::draw(f, &mut app_guard);
        });
    }).map_err(|e| color_eyre::eyre::eyre!("{}", e))?;

    Ok(())
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
    let (app, event_tx, mut _event_rx, _ui_tx, _ui_rx, _docker) = setup_app(tile_queue);

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
                if stdin.read(&mut buffer).is_ok() {
                    if let Some(evt) = parser.advance(buffer[0]) {
                         if let Some(converted) = crate::event::convert_event(evt) {
                             let _ = tx.blocking_send(AppEvent::Raw(converted));
                         }
                    }
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
    mpsc::Sender<UiCommand>,
    mpsc::Receiver<UiCommand>,
    Option<Arc<DockerModule>>
) {
    let app = Arc::new(Mutex::new(App::new(tile_queue)));    
    let (_event_tx, event_rx) = mpsc::channel(1000); 
    // Logic Loop Channel (Input)
    let (logic_tx, mut logic_rx) = mpsc::channel(1000);
    
    let (ui_tx, ui_rx) = mpsc::channel::<UiCommand>(10);
    let (docker_tx, mut docker_rx) = mpsc::channel(10);
    let docker_module = DockerModule::new().ok().map(Arc::new);

    let app_bg = app.clone();
    let docker_bg = docker_module.clone();
    let event_tx_bg = logic_tx.clone(); // Logic loop sends to itself via this? No, event_tx sends to logic_rx.
    
    // WE need to separate:
    // 1. External Events (Window/TTY) -> logic_tx
    // 2. Logic Loop consuming logic_rx
    
    // The callers (run_window) need a Sender. We return logic_tx as 'event_tx'.
    
    let ui_tx_bg = ui_tx.clone();
    let docker_tx_bg = docker_tx.clone();
    
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Docker Monitor
            if let Some(docker) = docker_bg.clone() {
                let tx = docker_tx_bg;
                tokio::spawn(async move {
                    loop {
                        if let Ok(containers) = docker.get_containers().await {
                            let _ = tx.send(containers).await;
                        }
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                });
            }

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
            
            // LOGIC LOOP
            loop {
                tokio::select! {
                     Some(containers) = docker_rx.recv() => {
                        if let Ok(mut app) = app_bg.lock() { app.docker_state.containers = containers; }
                    }
                    Some(evt) = logic_rx.recv() => {
                         match evt {
                            AppEvent::Tick => {
                                if let Ok(mut app) = app_bg.lock() {
                                    let mut system_state = std::mem::replace(&mut app.system_state, crate::app::SystemState {
                                        cpu_usage: 0.0, mem_usage: 0.0, total_mem: 0.0, disks: Vec::new(), processes: Vec::new(), selected_process_index: 0
                                    });
                                    app.system_module.update(&mut system_state);
                                    app.system_state = system_state;
                                }
                            }
                            AppEvent::Raw(raw) => {
                                let mut app_guard = app_bg.lock().unwrap();
                                let app_tx = event_tx_bg.clone();
                                handle_event(raw, &mut app_guard, &docker_bg, app_tx);
                                
                                // Check if selection changed and if it's an image
                                if let Some(fs) = app_guard.current_file_state() {
                                    if let Some(idx) = fs.selected_index {
                                        if let Some(path) = fs.files.get(idx) {
                                            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                                            if ext == "png" || ext == "jpg" || ext == "jpeg" {
                                                let _ = event_tx_bg.send(AppEvent::LoadImage(path.clone())).await;
                                            } else {
                                                app_guard.current_preview = None;
                                            }
                                        }
                                    }
                                }
                            }
                            AppEvent::LoadImage(path) => {
                                let ui_tx = ui_tx_bg.clone();
                                tokio::spawn(async move {
                                    // Use a stable ID for preview (e.g. 9999)
                                    if let Some(asset) = ImageLoader::load_and_resize(&path, 9999, 400, 400).ok() {
                                        let _ = ui_tx.send(UiCommand::RegisterImage(asset.id, asset.data, asset.width, asset.height)).await;
                                    }
                                });
                            }
                            AppEvent::ImageReady(id, _, _, _) => {
                                if let Ok(mut app) = app_bg.lock() {
                                    app.current_preview = Some(id);
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
                                        // Trigger preview if an image is selected
                                        if let Some(sel_idx) = fs.selected_index {
                                            if let Some(path) = fs.files.get(sel_idx) {
                                                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                                                if ext == "png" || ext == "jpg" || ext == "jpeg" {
                                                    let _ = event_tx_bg.send(AppEvent::LoadImage(path.clone())).await;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
    });

    (app, logic_tx, event_rx, ui_tx, ui_rx, docker_module)
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

fn update_docker_filter(app: &mut App) {
    if let Some(fs) = app.current_file_state() {
        if let Some(idx) = fs.selected_index {
            if let Some(path) = fs.files.get(idx) {
                if path.is_dir() {
                    if path.join("Dockerfile").exists() || path.join("docker-compose.yml").exists() || path.join("docker-compose.yaml").exists() {
                        app.docker_state.filter = Some(path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string());
                        return;
                    }
                }
            }
        }
    }
    app.docker_state.filter = None;
}

fn handle_event(evt: Event, app: &mut App, docker_module: &Option<Arc<DockerModule>>, event_tx: mpsc::Sender<AppEvent>) {
    match evt {
        Event::Mouse(me) => {
            let column = me.column;
            let row = me.row;
            match me.kind {
                MouseEventKind::Down(button) => {
                    if button == MouseButton::Back {
                        if let Some(fs) = app.current_file_state_mut() { navigate_back(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index)); }
                        return;
                    }
                    if button == MouseButton::Forward {
                        if let Some(fs) = app.current_file_state_mut() { navigate_forward(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index)); }
                        return;
                    }
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
                                    1 => { app.mode = AppMode::Normal; },
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
                            else if column < 33 { app.current_view = CurrentView::Docker; }
                        } else {
                            let sidebar_width = 16;
                            if column < sidebar_width {
                                app.sidebar_focus = true;
                                let sidebar_row = row.saturating_sub(2) as usize;
                                match sidebar_row {
                                    1..=4 => {
                                        app.sidebar_index = sidebar_row - 1;
                                        if let Some(p) = match app.sidebar_index { 0 => dirs::home_dir(), 1 => dirs::download_dir(), 2 => dirs::document_dir(), 3 => dirs::picture_dir(), _ => None } {
                                            if let Some(fs) = app.current_file_state_mut() {
                                                fs.current_path = p.clone(); fs.selected_index = Some(0); fs.search_filter.clear();
                                                *fs.table_state.offset_mut() = 0;
                                                push_history(fs, p);
                                            }
                                            let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index));
                                            app.sidebar_focus = false;
                                        }
                                    },
                                    r if r >= 7 => {
                                        let bookmark_idx = r - 7;
                                        if bookmark_idx < app.remote_bookmarks.len() {
                                            app.sidebar_index = r - 1;
                                            execute_command(crate::app::CommandAction::ConnectToRemote(bookmark_idx), app, docker_module, event_tx.clone());
                                        }
                                    },
                                    _ => {}
                                }
                            } else {
                                app.sidebar_focus = false;
                                if app.current_view == CurrentView::Files {
                                    let index = fs_mouse_index(row, app);
                                    if let Some(fs) = app.current_file_state_mut() {
                                        if index < fs.files.len() {
                                            fs.selected_index = Some(index);
                                            fs.table_state.select(Some(index));
                                            if let Some(path) = fs.files.get(index).cloned() {
                                                if path.is_dir() {
                                                    // Double click logic omitted for brevity in MVP, single click select
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
                    if app.current_view == CurrentView::Files { if let Some(fs) = app.current_file_state_mut() { let new_offset = fs.table_state.offset().saturating_sub(1); *fs.table_state.offset_mut() = new_offset; } } 
                    else { app.move_up(); update_docker_filter(app); }
                }
                MouseEventKind::ScrollDown => {
                    if app.current_view == CurrentView::Files { if let Some(fs) = app.current_file_state_mut() { let capacity = fs.view_height.saturating_sub(2); let effective_capacity = capacity.saturating_sub(3); let max_offset = fs.files.len().saturating_sub(effective_capacity); let new_offset = (fs.table_state.offset() + 1).min(max_offset); *fs.table_state.offset_mut() = new_offset; } } 
                    else { app.move_down(); update_docker_filter(app); }
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
                        KeyCode::Enter => { if let Some(cmd) = app.filtered_commands.get(app.command_index).cloned() { execute_command(cmd.action, app, docker_module, event_tx.clone()); } app.mode = AppMode::Normal; app.input.clear(); }
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
                _ => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        match key.code {
                            KeyCode::Char('q') => app.running = false,
                            KeyCode::Char('.') => { app.mode = AppMode::CommandPalette; update_commands(app); }
                            KeyCode::Char('f') => app.current_view = CurrentView::Files,
                            KeyCode::Char('p') => app.current_view = CurrentView::System,
                            KeyCode::Char('d') => app.current_view = CurrentView::Docker,
                            KeyCode::Char('h') => { if let Some(fs) = app.current_file_state_mut() { fs.show_hidden = !fs.show_hidden; let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index)); } }
                            _ => {}
                        }
                        return;
                    }
                    match key.code {
                        KeyCode::Down => { app.move_down(); }
                        KeyCode::Up => { app.move_up(); }
                        KeyCode::Left => { if key.modifiers.contains(KeyModifiers::ALT) { if let Some(fs) = app.current_file_state_mut() { navigate_back(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index)); } } else { app.move_left(); } }
                        KeyCode::Right => { if key.modifiers.contains(KeyModifiers::ALT) { if let Some(fs) = app.current_file_state_mut() { navigate_forward(fs); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index)); } } else { app.move_right(); } }
                        KeyCode::Enter => { if let Some(fs) = app.current_file_state_mut() { if let Some(idx) = fs.selected_index { if let Some(path) = fs.files.get(idx).cloned() { if path.is_dir() { fs.current_path = path.clone(); fs.selected_index = Some(0); push_history(fs, path); let _ = event_tx.try_send(AppEvent::RefreshFiles(app.tab_index)); } } } } }
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
        CommandItem { label: "View: Docker".to_string(), action: crate::app::CommandAction::SwitchView(CurrentView::Docker) },
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

fn execute_command(action: crate::app::CommandAction, app: &mut App, docker_module: &Option<Arc<DockerModule>>, _event_tx: mpsc::Sender<AppEvent>) {
    match action {
        crate::app::CommandAction::Quit => { app.running = false; },
        crate::app::CommandAction::ToggleZoom => app.toggle_zoom(),
        crate::app::CommandAction::SwitchView(view) => app.current_view = view,
        crate::app::CommandAction::StartContainer(name) => { if let Some(docker) = docker_module { let docker = docker.clone(); tokio::spawn(async move { let _ = docker.start_container(&name).await; }); } },
        crate::app::CommandAction::StopContainer(name) => { if let Some(docker) = docker_module { let docker = docker.clone(); tokio::spawn(async move { let _ = docker.stop_container(&name).await; }); } },
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
