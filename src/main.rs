use std::{io, time::{Duration, Instant}};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseEventKind, MouseButton},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    Terminal,
};

mod app;
mod ui;
mod modules;
mod event;
mod config;
mod license;

use std::sync::Arc;
use tokio::sync::mpsc;
use crate::app::{App, AppMode, CommandItem, CommandAction, CurrentView};
use crate::modules::docker::DockerModule;
use bollard::models::ContainerSummary;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new();

    // Setup Docker channel
    let (tx, rx) = mpsc::channel(10);
    let docker_module = DockerModule::new().ok().map(Arc::new);

    if let Some(docker) = docker_module.clone() {
        tokio::spawn(async move {
            loop {
                if let Ok(containers) = docker.get_containers().await {
                    let _ = tx.send(containers).await;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }

    let res = run_app(&mut terminal, &mut app, rx, docker_module).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

fn update_docker_filter(app: &mut App) {
    if let Some(file_state) = app.current_file_state() {
        if let Some(path) = file_state.files.get(file_state.selected_index) {
            if path.is_dir() {
                let has_dockerfile = path.join("Dockerfile").exists();
                let has_compose = path.join("docker-compose.yml").exists() || path.join("docker-compose.yaml").exists();
                
                if has_dockerfile || has_compose {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    app.docker_state.filter = Some(name.to_string());
                    return;
                }
            }
        }
    }
    app.docker_state.filter = None;
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>, 
    app: &mut App, 
    mut rx: mpsc::Receiver<Vec<ContainerSummary>>,
    docker_module: Option<Arc<DockerModule>>,
) -> io::Result<()> {
    let tick_rate = Duration::from_millis(250);
    let mut last_tick = Instant::now();

    while app.running {
        terminal.draw(|f| ui::draw(f, app))?;

        while let Ok(containers) = rx.try_recv() {
            app.docker_state.containers = containers;
        }

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            match crossterm::event::read()? {
                Event::Mouse(mouse) => {
                    match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left) => {
                            let (cols, rows) = terminal.size().map(|s| (s.width, s.height)).unwrap_or((0, 0));
                        // Tab Bar (Top Row - 0)
                        if mouse.row == 0 {
                            if mouse.column < 11 { app.current_view = CurrentView::Files; }
                            else if mouse.column < 22 { app.current_view = CurrentView::System; }
                            else if mouse.column < 33 { app.current_view = CurrentView::Docker; }
                        }
                        // Footer Bar (Last Row)
                        else if mouse.row == rows.saturating_sub(1) {
                            // "^. Console | " is roughly 13 columns
                            if mouse.column < 13 {
                                app.mode = AppMode::CommandPalette;
                                app.input.clear();
                                update_commands(app);
                            }
                        }
                        // Workspace Area
                        else if mouse.row < rows.saturating_sub(1) {
                            let sidebar_width = (cols as f32 * 0.2) as u16;
                            
                            if mouse.column < sidebar_width {
                                app.sidebar_focus = true;
                                // Content starts at row 2 (Tab 0, Border 1)
                                let index = mouse.row.saturating_sub(2) as usize;
                                if index < 4 { app.sidebar_index = index; }
                            } else {
                                app.sidebar_focus = false;
                                if app.current_view == CurrentView::Files {
                                    // Tabs(1) + Border(1) + PathBar(3) + Header(1) = items start at row 6
                                    let index = mouse.row.saturating_sub(6) as usize;
                                    if let Some(file_state) = app.current_file_state_mut() {
                                        if index < file_state.files.len() {
                                            file_state.selected_index = index;
                                        }
                                    }
                                } else if app.current_view == CurrentView::System {
                                    // Tabs(1) + Border(1) + CPU(3) + MEM(3) + Disk(6) + Header(1) = row 15
                                    let index = mouse.row.saturating_sub(15) as usize;
                                    if index < app.system_state.processes.len() {
                                        app.system_state.selected_process_index = index;
                                    }
                                } else if app.current_view == CurrentView::Docker {
                                    // Tabs(1) + Border(1) = Row 2
                                    let index = mouse.row.saturating_sub(2) as usize;
                                    if index < app.docker_state.containers.len() {
                                        app.docker_state.selected_index = index;
                                    }
                                }
                            }
                        }
                        }
                        MouseEventKind::ScrollUp => { app.move_up(); update_docker_filter(app); }
                        MouseEventKind::ScrollDown => { app.move_down(); update_docker_filter(app); }
                        _ => {}
                    }
                }
                Event::Key(key) => {
                    if matches!(app.mode, AppMode::Location) {
                        match key.code {
                            KeyCode::Esc => app.mode = AppMode::Normal,
                            KeyCode::Char(c) => app.input.push(c),
                            KeyCode::Backspace => { app.input.pop(); }
                            KeyCode::Enter => {
                                let path = std::path::PathBuf::from(&app.input);
                                if path.exists() {
                                    if let Some(file_state) = app.current_file_state_mut() {
                                        file_state.current_path = path;
                                        file_state.selected_index = 0;
                                        crate::modules::files::update_files(file_state);
                                    }
                                }
                                app.mode = AppMode::Normal;
                            }
                            _ => {}
                        }
                    } else if matches!(app.mode, AppMode::Rename) {
                        match key.code {
                            KeyCode::Esc => app.mode = AppMode::Normal,
                            KeyCode::Char(c) => app.input.push(c),
                            KeyCode::Backspace => { app.input.pop(); }
                            KeyCode::Enter => {
                                let new_name = app.input.clone();
                                if let Some(file_state) = app.current_file_state_mut() {
                                    if let Some(old_path) = file_state.files.get(file_state.selected_index) {
                                        let mut new_path = old_path.clone();
                                        new_path.set_file_name(&new_name);
                                        let _ = std::fs::rename(old_path, new_path);
                                        crate::modules::files::update_files(file_state);
                                    }
                                }
                                app.mode = AppMode::Normal;
                            }
                            _ => {}
                        }
                    } else if matches!(app.mode, AppMode::NewFolder) {
                        match key.code {
                            KeyCode::Esc => app.mode = AppMode::Normal,
                            KeyCode::Char(c) => app.input.push(c),
                            KeyCode::Backspace => { app.input.pop(); }
                            KeyCode::Enter => {
                                let folder_name = app.input.clone();
                                if let Some(file_state) = app.current_file_state_mut() {
                                    let path = file_state.current_path.join(folder_name);
                                    let _ = std::fs::create_dir_all(path);
                                    crate::modules::files::update_files(file_state);
                                }
                                app.mode = AppMode::Normal;
                            }
                            _ => {}
                        }
                    } else if matches!(app.mode, AppMode::ColumnSetup) {
                        if key.code == KeyCode::Esc || key.code == KeyCode::Enter {
                            app.mode = AppMode::Normal;
                        } else if let Some(file_state) = app.current_file_state_mut() {
                            match key.code {
                                KeyCode::Char('n') => toggle_column(file_state, crate::app::FileColumn::Name),
                                KeyCode::Char('s') => toggle_column(file_state, crate::app::FileColumn::Size),
                                KeyCode::Char('m') => toggle_column(file_state, crate::app::FileColumn::Modified),
                                KeyCode::Char('c') => toggle_column(file_state, crate::app::FileColumn::Created),
                                KeyCode::Char('p') => toggle_column(file_state, crate::app::FileColumn::Permissions),
                                KeyCode::Char('e') => toggle_column(file_state, crate::app::FileColumn::Extension),
                                _ => {}
                            }
                        }
                    } else if matches!(app.mode, AppMode::Delete) {
                        if key.code == KeyCode::Char('y') || key.code == KeyCode::Enter {
                            match app.current_view {
                                CurrentView::Files => {
                                    if let Some(file_state) = app.current_file_state_mut() {
                                        if let Some(path) = file_state.files.get(file_state.selected_index) {
                                            let _ = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
                                            crate::modules::files::update_files(file_state);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        app.mode = AppMode::Normal;
                    } else if matches!(app.mode, AppMode::CommandPalette) {
                        match key.code {
                            KeyCode::Esc => app.mode = AppMode::Normal,
                            KeyCode::Char(c) => { app.input.push(c); update_commands(app); }
                            KeyCode::Backspace => { app.input.pop(); update_commands(app); }
                            KeyCode::Up => { if app.command_index > 0 { app.command_index -= 1; } }
                            KeyCode::Down => { if app.command_index < app.filtered_commands.len().saturating_sub(1) { app.command_index += 1; } }
                            KeyCode::Enter => {
                                if let Some(cmd) = app.filtered_commands.get(app.command_index).cloned() {
                                    execute_command(cmd.action, app, &docker_module);
                                }
                                app.mode = AppMode::Normal;
                                app.input.clear();
                            }
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('q') => app.running = false,
                            KeyCode::Char('f') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => app.current_view = CurrentView::Files,
                            KeyCode::Char('p') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => app.current_view = CurrentView::System,
                            KeyCode::Char('d') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => app.current_view = CurrentView::Docker,
                            KeyCode::Char('.') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                app.mode = AppMode::CommandPalette;
                                app.input.clear();
                                update_commands(app);
                            }
                            KeyCode::Char('l') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                let path_opt = app.current_file_state().map(|fs| fs.current_path.to_string_lossy().to_string());
                                if let Some(path_str) = path_opt {
                                    app.mode = AppMode::Location;
                                    app.input = path_str;
                                }
                            }
                            KeyCode::Char('h') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                if let Some(file_state) = app.current_file_state_mut() {
                                    file_state.show_hidden = !file_state.show_hidden;
                                    crate::modules::files::update_files(file_state);
                                }
                            }
                            KeyCode::Char('b') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                if let Some(file_state) = app.current_file_state_mut() {
                                    if let Some(path) = file_state.files.get(file_state.selected_index).cloned() {
                                        if !file_state.starred.insert(path.clone()) { file_state.starred.remove(&path); }
                                    }
                                }
                            }
                            KeyCode::Char('t') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                if let Some(current) = app.current_file_state() {
                                    let mut new_state = crate::app::FileState {
                                        current_path: current.current_path.clone(),
                                        selected_index: 0,
                                        files: Vec::new(),
                                        show_hidden: current.show_hidden,
                                        git_status: std::collections::HashMap::new(),
                                        clipboard: None,
                                        search_filter: String::new(),
                                        starred: current.starred.clone(),
                                        columns: current.columns.clone(),
                                    };
                                    crate::modules::files::update_files(&mut new_state);
                                    app.file_tabs.push(new_state);
                                    app.tab_index = app.file_tabs.len() - 1;
                                }
                            }
                            KeyCode::Char('w') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                if app.file_tabs.len() > 1 {
                                    app.file_tabs.remove(app.tab_index);
                                    app.tab_index = app.tab_index.min(app.file_tabs.len() - 1);
                                } else { app.running = false; }
                            }
                            KeyCode::Tab if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                app.tab_index = (app.tab_index + 1) % app.file_tabs.len();
                            }
                            KeyCode::Char('C') if key.modifiers.contains(crossterm::event::KeyModifiers::ALT) => {
                                if app.current_view == CurrentView::Files { app.mode = AppMode::ColumnSetup; }
                            }
                            KeyCode::Down | KeyCode::Char('j') => { app.move_down(); update_docker_filter(app); }
                            KeyCode::Up | KeyCode::Char('k') => { app.move_up(); update_docker_filter(app); }
                            KeyCode::Left | KeyCode::Char('h') => { 
                                let searching = app.current_file_state().map(|s| !s.search_filter.is_empty()).unwrap_or(false);
                                if !searching { app.move_left(); }
                            }
                            KeyCode::Right | KeyCode::Char('l') => {
                                let searching = app.current_file_state().map(|s| !s.search_filter.is_empty()).unwrap_or(false);
                                if !searching { app.move_right(); }
                            }
                            KeyCode::F(5) => { if let Some(fs) = app.current_file_state_mut() { crate::modules::files::update_files(fs); } }
                            KeyCode::F(2) => {
                                let name_opt = app.current_file_state().and_then(|fs| {
                                    fs.files.get(fs.selected_index).map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
                                });
                                if let Some(name) = name_opt {
                                    app.mode = AppMode::Rename;
                                    app.input = name;
                                }
                            }
                            KeyCode::Delete => { app.mode = AppMode::Delete; }
                            KeyCode::Enter if key.modifiers.contains(crossterm::event::KeyModifiers::ALT) => { app.mode = AppMode::Properties; }
                            KeyCode::Enter => {
                                if app.sidebar_focus {
                                    let path = match app.sidebar_index {
                                        0 => dirs::home_dir(), 1 => dirs::download_dir(),
                                        2 => dirs::document_dir(), 3 => dirs::picture_dir(), _ => None,
                                    };
                                    if let Some(p) = path {
                                        if let Some(fs) = app.current_file_state_mut() {
                                            fs.current_path = p; fs.selected_index = 0; fs.search_filter.clear();
                                            crate::modules::files::update_files(fs); app.sidebar_focus = false;
                                        }
                                    }
                                } else if let Some(fs) = app.current_file_state_mut() {
                                    if let Some(path) = fs.files.get(fs.selected_index).cloned() {
                                        if path.is_dir() {
                                            fs.current_path = path; fs.selected_index = 0; fs.search_filter.clear();
                                            crate::modules::files::update_files(fs);
                                        }
                                    }
                                }
                            }
                            KeyCode::Backspace => {
                                if let Some(fs) = app.current_file_state_mut() {
                                    if !fs.search_filter.is_empty() { fs.search_filter.pop(); crate::modules::files::update_files(fs); }
                                    else if let Some(p) = fs.current_path.parent() {
                                        fs.current_path = p.to_path_buf(); fs.selected_index = 0;
                                        crate::modules::files::update_files(fs);
                                    }
                                }
                            }
                            KeyCode::Esc => {
                                if let Some(fs) = app.current_file_state_mut() {
                                    if !fs.search_filter.is_empty() { fs.search_filter.clear(); crate::modules::files::update_files(fs); }
                                }
                            }
                            KeyCode::Char(c) => {
                                if app.current_view == CurrentView::Files {
                                    if let Some(fs) = app.current_file_state_mut() {
                                        fs.search_filter.push(c); fs.selected_index = 0;
                                        crate::modules::files::update_files(fs);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.system_module.update(&mut app.system_state);
            last_tick = Instant::now();
        }
    }
    Ok(())
}

fn update_commands(app: &mut App) {
    let mut commands = vec![
        CommandItem { label: "Quit".to_string(), action: CommandAction::Quit },
        CommandItem { label: "Toggle Zoom".to_string(), action: CommandAction::ToggleZoom },
        CommandItem { label: "View: Files".to_string(), action: CommandAction::SwitchView(CurrentView::Files) },
        CommandItem { label: "View: Docker".to_string(), action: CommandAction::SwitchView(CurrentView::Docker) },
        CommandItem { label: "View: System".to_string(), action: CommandAction::SwitchView(CurrentView::System) },
    ];
    for container in &app.docker_state.containers {
         let name = container.names.as_ref().map(|n| n.first().map(|s| s.as_str()).unwrap_or("")).unwrap_or("").trim_start_matches('/');
         if name.is_empty() { continue; }
         commands.push(CommandItem { label: format!("Start Container: {}", name), action: CommandAction::StartContainer(name.to_string()) });
         commands.push(CommandItem { label: format!("Stop Container: {}", name), action: CommandAction::StopContainer(name.to_string()) });
    }
    app.filtered_commands = commands.into_iter().filter(|cmd| cmd.label.to_lowercase().contains(&app.input.to_lowercase())).collect();
    app.command_index = app.command_index.min(app.filtered_commands.len().saturating_sub(1));
}

fn execute_command(action: CommandAction, app: &mut App, docker_module: &Option<Arc<DockerModule>>) {
    match action {
        CommandAction::Quit => { app.running = false; },
        CommandAction::ToggleZoom => app.toggle_zoom(),
        CommandAction::SwitchView(view) => app.current_view = view,
        CommandAction::StartContainer(name) => { if let Some(docker) = docker_module { let docker = docker.clone(); tokio::spawn(async move { let _ = docker.start_container(&name).await; }); } },
        CommandAction::StopContainer(name) => { if let Some(docker) = docker_module { let docker = docker.clone(); tokio::spawn(async move { let _ = docker.stop_container(&name).await; }); } },
    }
}

fn toggle_column(file_state: &mut crate::app::FileState, col: crate::app::FileColumn) {
    if file_state.columns.contains(&col) { file_state.columns.retain(|c| *c != col); }
    else { file_state.columns.push(col); }
}